use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use parking_lot::Mutex;

use slint::{ModelRc, VecModel};

use crate::ssh::SessionHandle;

use super::*;
use super::{TermBuffer, TermBuffers, set_terminal_row};

/// How much of the byte stream we retain per tab for resize-reflow (#169).
pub const RAW_CAP: usize = 2 * 1024 * 1024;

/// Minimal CSI-final-byte rewriter state (persists across read chunks).
#[derive(Clone, Copy, PartialEq)]
pub enum CsiState {
    /// Normal text.
    Normal,
    /// Saw ESC (0x1b), waiting to see if it starts a CSI (`[`).
    Esc,
    /// Inside a CSI sequence (after `ESC [`), scanning params/intermediates.
    Csi,
}

/// Cumulative grid columns for a rendered line. The plain text we keep stores
/// ONE char per glyph, but a wide (CJK) glyph occupies TWO grid cells, so a char
/// index is *not* a grid column. `prefix[i]` is the starting grid column of
/// char `i`; `prefix[chars.len()]` is the line's total cell width. Zero-width
/// chars (combining marks) share their base char's column (#132).
pub fn cell_prefix(chars: &[char]) -> Vec<usize> {
    use unicode_width::UnicodeWidthChar;
    let mut prefix = Vec::with_capacity(chars.len() + 1);
    let mut acc = 0usize;
    for &ch in chars {
        prefix.push(acc);
        acc += ch.width().unwrap_or(0);
    }
    prefix.push(acc);
    prefix
}

/// First char index whose cell span contains grid column `target` — i.e. the
/// char a selection STARTING at that column should begin on. Clamps to the end
/// of the line when `target` is past the content (#132).
pub fn char_at_cell_start(prefix: &[usize], target: usize) -> usize {
    let n = prefix.len().saturating_sub(1); // chars.len()
    for i in 0..n {
        if prefix[i] <= target && target < prefix[i + 1] {
            return i;
        }
    }
    n
}

/// Exclusive char index just past grid column `target` — i.e. the slice end for
/// a selection ENDING (inclusive) at that column. Trailing zero-width marks on
/// the last glyph are kept because their start column is not strictly greater
/// than `target` (#132).
pub fn char_after_cell_end(prefix: &[usize], target: usize) -> usize {
    let n = prefix.len().saturating_sub(1); // chars.len()
    for i in 0..n {
        if prefix[i] > target {
            return i;
        }
    }
    n
}

/// Find every (case-insensitive) occurrence of `query` across the currently
/// displayed rows and return highlight rectangles in GRID-COLUMN space (wide
/// CJK glyphs count as two columns, so highlights line up over the text #132).
pub fn compute_find_matches(rows: &[String], query: &str) -> Vec<TermMatch> {
    let mut out: Vec<TermMatch> = Vec::new();
    if query.is_empty() {
        return out;
    }
    let q: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();
    if q.is_empty() {
        return out;
    }
    for (r, line) in rows.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
        let prefix = cell_prefix(&chars);
        let mut i = 0usize;
        while i + q.len() <= lower.len() {
            if lower[i..i + q.len()] == q[..] {
                let col = prefix[i] as i32;
                let len = (prefix[i + q.len()] - prefix[i]) as i32;
                out.push(TermMatch {
                    row: r as i32,
                    col,
                    len,
                });
                i += q.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

/// Apply a settled terminal size to the PTY + vt100 grid. Factored out of the
/// resize callback so that callback can debounce — a layout reflow can briefly
/// report a near-zero width, collapsing term-cols to its 10-col floor; applying
/// that to the remote PTY reflows vt100 and garbles running output like a
/// `git clone` progress meter (#163). Debouncing means only the settled size
/// ever reaches the server.
pub fn apply_terminal_resize(
    handles: &Rc<RefCell<HashMap<String, SessionHandle>>>,
    bufs: &TermBuffers,
    last_term_size: &Arc<Mutex<(u32, u32)>>,
    tab_id: &str,
    cols: u32,
    rows: u32,
) {
    *last_term_size.lock() = (cols, rows);
    if let Some(handle) = handles.borrow().get(tab_id) {
        handle.resize(cols, rows);
    }
    if let Some(buf) = bufs.lock().get_mut(tab_id) {
        let (old_rows, old_cols) = buf.parser.screen().size();
        let (new_rows, new_cols) = (rows as u16, cols as u16);
        if (new_rows, new_cols) != (old_rows, old_cols) {
            if buf.parser.screen().alternate_screen() {
                // Alt-screen (tmux/vim/btop): the remote redraws the whole screen
                // on SIGWINCH, so just resize the grid and let that redraw fill it.
                buf.parser.set_size(new_rows, new_cols);
            } else {
                // Reflow already-printed output to the new width by replaying the
                // byte stream — vt100's set_size only truncates/pads (#169).
                buf.reflow(new_rows, new_cols);
            }
            // The pre/post-resize screens differ; drop the scroll-detection
            // snapshot so the next output isn't mis-read as a scroll.
            buf.prev.clear();
        }
    }
}

/// Recompute spans + cursor + find/selection highlights for one tab from its
/// current vt100 screen (respecting scrollback) and push them to the model.
/// Used by scroll + selection callbacks (Output has its own equivalent inline).
pub fn rebuild_tab_display(win: &AppWindow, bufs: &TermBuffers, tab_id: &str) {
    let data = {
        let mut map = bufs.lock();
        let Some(buf) = map.get_mut(tab_id) else { return };
        let cols = buf.parser.screen().size().1;
        let b = buf.render(); // also refreshes buf.displayed_text
        let matches = compute_find_matches(&buf.displayed_text, &buf.find_query);
        let sel = buf.selection_rects_visible(cols);
        (b, matches, sel)
    };
    let (b, matches, sel) = data;
    let spans = ModelRc::from(Rc::new(VecModel::from(b.spans)));
    let fm = ModelRc::from(Rc::new(VecModel::from(matches)));
    let sm = ModelRc::from(Rc::new(VecModel::from(sel)));
    let (cr, cc, ru, alt) = (b.cursor_row, b.cursor_col, b.rows_used, b.is_alt);
    let (smax, soff) = (b.scroll_max, b.scroll_offset);
    set_terminal_row(win, tab_id, move |row| {
        row.spans = spans.clone();
        row.cursor_row = cr;
        row.cursor_col = cc;
        row.rows_used = ru;
        row.is_alt_screen = alt;
        row.find_matches = fm.clone();
        row.selection = sm.clone();
        row.scroll_max = smax;
        row.scroll_offset = soff;
    });
}

pub fn redact_key(key: &str) -> String {
    if key.is_empty() {
        return "(empty)".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut printable = 0usize;
    for c in key.chars() {
        let cp = c as u32;
        if cp < 0x20 || (0x7f..=0x9f).contains(&cp) {
            parts.push(format!("U+{cp:04X}"));
        } else {
            printable += 1;
        }
    }
    if printable > 0 {
        parts.push(format!("<{printable} printable redacted>"));
    }
    parts.join(",")
}

/// `app_cursor` mirrors the remote terminal's DECCKM mode (`\x1b[?1h/l`):
/// when true the four arrow keys must use SS3 sequences (`\x1bOA`…) instead
/// of the default CSI sequences (`\x1b[A`…).  Full-screen apps like nano and
/// vim set this mode on startup.
/// Build the editor's line-number gutter text: "1\n2\n…\nN", one number per line
/// of `content`, matching its (newline-separated) line count (#81).
pub fn line_numbers_for(content: &str) -> String {
    use std::fmt::Write;
    let lines = content.split('\n').count().max(1);
    let mut s = String::with_capacity(lines * 4);
    for i in 1..=lines {
        if i > 1 {
            s.push('\n');
        }
        let _ = write!(s, "{i}");
    }
    s
}

/// Normalise pasted text's line endings to a single CR (0x0d) — what a terminal
/// expects for Enter.
///
/// The clipboard may hold CRLF (Windows) or LF line breaks. Sending those to the
/// PTY verbatim makes the remote shell see *two* line breaks per line (CR then
/// LF), which prematurely ends a `\`-continued line: pasting
/// `sudo apt install \<newline>  docker-ce` would run `sudo apt install` with no
/// package and drop the rest. Collapsing every CRLF/LF to one CR fixes it.
pub fn normalize_pasted_newlines(text: &str) -> String {
    text.replace("\r\n", "\r").replace('\n', "\r")
}

pub fn key_to_pty_bytes(key: &str, ctrl: bool, alt: bool, app_cursor: bool) -> Vec<u8> {
    // --- Special keys (Slint PUA code points) ------------------------------
    // Arrow keys: respect DECCKM application-cursor mode.
    let special: Option<&[u8]> = match key {
        "\u{F700}" => Some(if app_cursor { b"\x1bOA" } else { b"\x1b[A" }), // Up
        "\u{F701}" => Some(if app_cursor { b"\x1bOB" } else { b"\x1b[B" }), // Down
        "\u{F702}" => Some(if app_cursor { b"\x1bOD" } else { b"\x1b[D" }), // Left
        "\u{F703}" => Some(if app_cursor { b"\x1bOC" } else { b"\x1b[C" }), // Right
        "\u{F729}" => Some(b"\x1b[H"),   // Home
        "\u{F72B}" => Some(b"\x1b[F"),   // End
        "\u{F72C}" => Some(b"\x1b[5~"),  // PageUp
        "\u{F72D}" => Some(b"\x1b[6~"),  // PageDown
        // Forward-Delete. Slint's canonical key code for the Delete key is
        // U+007F (see i-slint-common key_codes: F728 is explicitly *not* used,
        // it collapses to the 0x7f control code). The old F728 mapping never
        // matched on any platform, so Delete fell through to the generic path
        // and behaved like backspace / garbled the char instead of sending the
        // VT "delete forward" sequence (B站 fan report).
        "\u{007F}" | "\u{F728}" => Some(b"\x1b[3~"),  // Delete (forward)
        "\u{F704}" => Some(b"\x1bOP"),   // F1
        "\u{F705}" => Some(b"\x1bOQ"),   // F2
        "\u{F706}" => Some(b"\x1bOR"),   // F3
        "\u{F707}" => Some(b"\x1bOS"),   // F4
        "\u{F708}" => Some(b"\x1b[15~"), // F5
        "\u{F709}" => Some(b"\x1b[17~"), // F6
        "\u{F70A}" => Some(b"\x1b[18~"), // F7
        "\u{F70B}" => Some(b"\x1b[19~"), // F8
        "\u{F70C}" => Some(b"\x1b[20~"), // F9
        "\u{F70D}" => Some(b"\x1b[21~"), // F10
        "\u{F70E}" => Some(b"\x1b[23~"), // F11
        "\u{F70F}" => Some(b"\x1b[24~"), // F12
        _ => None,
    };
    if let Some(seq) = special {
        return seq.to_vec();
    }

    // Slint sometimes sends `\u{0008}` for Backspace; terminals expect DEL.
    if key == "\u{0008}" {
        return vec![0x7f];
    }

    // Slint encodes Key::Return as "\n" (U+000A, LF).  Every real terminal
    // emulator (xterm, WezTerm, PuTTY …) sends 0x0D (CR) for Enter because
    // that is what a physical keyboard generates over a serial line.  bash/
    // readline happens to accept LF too, but ncurses apps in raw mode (nano,
    // vim command-line, passwd prompts …) strictly require CR to confirm input.
    // Ctrl+J (ctrl=true, "\n") intentionally stays 0x0A — it is a distinct
    // control character in some applications.
    if key == "\n" && !ctrl && !alt {
        return vec![0x0d];
    }

    // Empty text (e.g. the Ctrl/Shift/Alt key press itself) — nothing to send.
    if key.is_empty() {
        return vec![];
    }

    // --- Bare modifier keys: never forward to the PTY (issue #43) -----------
    // Slint encodes a lone modifier keypress not as "" but as a C0 code point:
    //   Shift=0x10 Ctrl=0x11 Alt=0x12 AltGr=0x13 CapsLock=0x14
    //   ShiftR=0x15 CtrlR=0x16 Meta=0x17 MetaR=0x18
    // Pressing Alt by itself (e.g. to Alt+Tab away) arrives here as key=0x12
    // with alt=true. Without this guard it would fall through to the Alt branch
    // below, get an ESC (0x1b) prefix, and bash/readline would treat the ESC as
    // Meta and discard the line the user was typing — the "Alt clears the
    // command" bug.
    //
    // The `!ctrl` guard is deliberate: a real Ctrl+P..Ctrl+X is encoded by some
    // Linux/macOS builds directly as the same C0 bytes (0x10..0x18) but with
    // ctrl=true (handled by the Ctrl branch just below), so we must NOT swallow
    // those. A lone modifier never carries ctrl=true except bare Ctrl/CtrlR
    // themselves, which are harmless to pass through as today.
    if !ctrl {
        if let Some(c) = key.chars().next() {
            let cp = c as u32;
            if key.chars().count() == 1 && (0x10..=0x18).contains(&cp) {
                return vec![];
            }
        }
    }

    // --- Ctrl + letter: synthesise C0 control character --------------------
    // Two cases:
    //   A) Platform already encoded the control char in `key` (e.g. "\x18" for
    //      Ctrl+X on some Linux/macOS builds). Pass through directly.
    //   B) Platform sends the letter ("x") with modifiers.control=true.
    //      We synthesise the C0 code ourselves.
    if ctrl {
        // Case A: key is already a C0 control character (0x01..0x1F, not ESC).
        if let Some(c) = key.chars().next() {
            let cp = c as u32;
            if key.chars().count() == 1 && (0x01..=0x1f).contains(&cp) {
                return vec![cp as u8];
            }
        }
        // Case B: letter + ctrl modifier.
        if let Some(c) = key.chars().next() {
            if key.chars().count() == 1 {
                let upper = c.to_ascii_uppercase() as u8;
                let ctrl_char: Option<u8> = match upper {
                    b'A'..=b'Z' => Some(upper - b'A' + 1),      // Ctrl+A=\x01 … Ctrl+Z=\x1A
                    b'[' => Some(0x1b),                           // Ctrl+[ = ESC
                    b'\\' => Some(0x1c),
                    b']' => Some(0x1d),
                    b'^' => Some(0x1e),
                    b'_' => Some(0x1f),
                    b'@' => Some(0x00),
                    _ => None,
                };
                if let Some(byte) = ctrl_char {
                    return vec![byte];
                }
            }
        }
    }

    // --- Skip unknown Private Use Area code points -------------------------
    if key.chars().any(|c| (0xE000..=0xF8FF).contains(&(c as u32))) {
        return vec![];
    }

    // --- Alt + key: prefix with ESC ----------------------------------------
    if alt && !ctrl {
        let mut bytes = vec![0x1b];
        bytes.extend_from_slice(key.as_bytes());
        return bytes;
    }

    // --- Everything else: send UTF-8 bytes as-is ---------------------------
    // This covers printable characters, \r (Enter), \t (Tab), \x1b (Escape),
    // and any C0 control chars the platform already encoded in `key`.
    key.as_bytes().to_vec()
}

/// A coloured, cursor-annotated snapshot ready for the Slint terminal grid.
pub struct BuiltScreen {
    pub spans: Vec<TermSpan>,
    pub cursor_row: i32,
    pub cursor_col: i32,
    pub rows_used: i32,
    pub is_alt: bool,
    /// Scrollback depth (max view_offset = history length) and the current
    /// offset (0 = live bottom), for the terminal scrollbar (#103).
    pub scroll_max: i32,
    pub scroll_offset: i32,
}

/// One coloured run within a line (its grid row is assigned at render time).
/// Colours are stored as raw vt100::Color so the palette (dark vs. light)
/// can be applied at render time rather than at history-capture time.
/// This lets a theme switch retroactively recolour the entire scrollback.
#[derive(Clone)]
pub struct HistSpan {
    pub text: String,
    pub fg: vt100::Color,
    pub bg: vt100::Color,
    pub bold: bool,
    pub col: i32,
    pub cells: i32,
}

/// A rendered line: plain text (one char per cell, for find/selection) + runs.
pub type Line = (String, Vec<HistSpan>);

/// Per-session scrollback cap (recycled on clear / tab close).
pub const MAX_HISTORY: usize = 100_000;

/// Build one screen row into `(plain_text, coloured_runs)`.  `plain` carries one
/// char per cell (space for blanks) so a char index equals the grid column.
/// Effective (contents, fg, bg, bold) for one grid cell, applying reverse-video.
/// `contents` is always one display string (" " for a blank cell).
pub fn cell_attrs(
    screen: &vt100::Screen,
    r: u16,
    c: u16,
) -> (String, vt100::Color, vt100::Color, bool, bool) {
    match screen.cell(r, c) {
        Some(cell) => {
            let (mut fg, mut bg) = (cell.fgcolor(), cell.bgcolor());
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            let s = cell.contents();
            // A CJK / wide glyph spans two cells; vt100 reports the 2nd as a
            // blank continuation. Emit nothing for it — the wide glyph already
            // covers both cells, so substituting a space would push the rest of
            // the line (and the cursor) out of alignment (#60). Genuinely empty
            // cells still become a space.
            let s = if cell.is_wide_continuation() {
                String::new()
            } else if s.is_empty() {
                " ".to_string()
            } else {
                s
            };
            (s, fg, bg, cell.bold(), cell.is_wide())
        }
        None => (
            " ".to_string(),
            vt100::Color::Default,
            vt100::Color::Default,
            false,
            false,
        ),
    }
}

pub fn build_row(screen: &vt100::Screen, r: u16, cols: u16) -> Line {
    let mut plain = String::with_capacity(cols as usize);
    let mut runs: Vec<HistSpan> = Vec::new();
    let mut c = 0u16;
    while c < cols {
        let (s, fg, bg, bold, wide) = cell_attrs(screen, r, c);
        // A wide (CJK) glyph gets its OWN span occupying exactly its two grid
        // cells, so the UI can box + centre + clip it on the monospace grid.
        // Otherwise a run of CJK rendered with a proportional CJK font drifts off
        // the grid — the trailing `/`, `$` or cursor overlaps or gaps the glyph
        // (CJK advance != 2×the Latin cell width).
        if wide {
            plain.push_str(&s);
            runs.push(HistSpan {
                text: s,
                fg,
                bg,
                bold,
                col: c as i32,
                cells: 2,
            });
            c += 2; // skip the wide-continuation cell
            continue;
        }
        // Group consecutive *narrow* cells that share fg + bg + bold into one run.
        // We keep blank cells *inside* a run (so a coloured bar made of spaces
        // still gets a background fill) and break on attribute change or a wide
        // cell (which starts its own span above).
        let start_col = c;
        let mut text = s.clone();
        plain.push_str(&s);
        c += 1;
        while c < cols {
            let (cs, cfg, cbg, cbold, cwide) = cell_attrs(screen, r, c);
            if cwide || cfg != fg || cbg != bg || cbold != bold {
                break;
            }
            plain.push_str(&cs);
            text.push_str(&cs);
            c += 1;
        }
        let cells = (c - start_col) as i32;
        let is_blank = text.chars().all(|ch| ch == ' ');
        let bg_default = matches!(bg, vt100::Color::Default);
        // Skip runs that contribute nothing visible: blank text *and* default bg.
        if is_blank && bg_default {
            continue;
        }
        runs.push(HistSpan {
            text,
            fg, // raw vt100::Color — converted at render time with the live palette
            bg,
            bold,
            col: start_col as i32,
            cells,
        });
    }
    (plain, runs)
}

/// Detect how many lines scrolled off the top between two screen snapshots by
/// finding the vertical shift `k` that best aligns `prev` onto `curr` (longest
/// top-anchored run of equal plain-text lines).  `k` lines left the top.
pub fn detect_scroll(prev: &[Line], curr: &[Line]) -> usize {
    let max_k = prev.len().min(curr.len()).min(20);
    for k in 1..=max_k {
        if prev[k..].iter().zip(curr.iter()).all(|(a, b)| a.0 == b.0) {
            return k;
        }
    }
    0
}

impl TermBuffer {
    // ---- Absolute-coordinate selection helpers (#18 follow-up) -------------
    //
    // The "combined" buffer is `history` (oldest first) followed by the live
    // screen rows.  A visible window of `rows` rows looks at a slice of it whose
    // top index depends on whether we're at the live bottom or scrolled up.

    /// Live screen rows plus the count of non-blank ones at the top.
    pub fn live_rows(&self) -> (Vec<Line>, usize) {
        let s = self.parser.screen();
        let (rows, cols) = s.size();
        let live: Vec<Line> = (0..rows).map(|r| build_row(s, r, cols)).collect();
        let used = live
            .iter()
            .rposition(|(_, runs)| !runs.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        (live, used)
    }

    /// Absolute combined-row index of the top visible row for the current view.
    pub fn view_top_abs(&self, _live_used: usize) -> usize {
        let rows = self.parser.screen().size().0 as usize;
        let hist_len = self.history.len();
        if self.view_offset == 0 {
            // Live view: visible row 0 is live screen row 0 = combined[hist_len].
            hist_len
        } else {
            // Include the screen's full row count (trailing blanks too) so this
            // mapping matches render()'s scroll window — keeping the live and
            // scrolled views continuous after a shrink/grow (#119-followup).
            let combined_len = hist_len + rows;
            combined_len.saturating_sub(rows + self.view_offset)
        }
    }

    /// Map a visible row (0..rows) to its absolute combined-row index.
    pub fn vis_to_abs(&self, vis_row: u16) -> usize {
        let (_, live_used) = self.live_rows();
        self.view_top_abs(live_used) + vis_row as usize
    }

    /// Highlight rectangles for the current selection, clipped to the visible
    /// window of the current view.
    pub fn selection_rects_visible(&self, cols: u16) -> Vec<TermMatch> {
        let (Some((ar, ac)), Some((fr, fc))) = (self.sel_anchor, self.sel_focus) else {
            return Vec::new();
        };
        let (lo_r, lo_c, hi_r, hi_c) = if (ar, ac) <= (fr, fc) {
            (ar, ac, fr, fc)
        } else {
            (fr, fc, ar, ac)
        };
        if (lo_r, lo_c) == (hi_r, hi_c) {
            return Vec::new();
        }
        let (_, live_used) = self.live_rows();
        let top = self.view_top_abs(live_used);
        let rows = self.parser.screen().size().0;
        let mut out = Vec::new();
        for vis in 0..rows {
            let abs = top + vis as usize;
            if abs < lo_r || abs > hi_r {
                continue;
            }
            let (c0, c1) = if abs == lo_r && abs == hi_r {
                (lo_c.min(hi_c), lo_c.max(hi_c))
            } else if abs == lo_r {
                (lo_c, cols.saturating_sub(1))
            } else if abs == hi_r {
                (0, hi_c)
            } else {
                (0, cols.saturating_sub(1))
            };
            out.push(TermMatch {
                row: vis as i32,
                col: c0 as i32,
                len: (c1.saturating_sub(c0) + 1) as i32,
            });
        }
        out
    }

    /// Extract the selected text from the combined buffer (whole selection,
    /// even the parts currently scrolled out of view).
    pub fn extract_selection_text(&self) -> String {
        let (Some((ar, ac)), Some((fr, fc))) = (self.sel_anchor, self.sel_focus) else {
            return String::new();
        };
        let (lo_r, lo_c, hi_r, hi_c) = if (ar, ac) <= (fr, fc) {
            (ar, ac, fr, fc)
        } else {
            (fr, fc, ar, ac)
        };
        let (live, live_used) = self.live_rows();
        let hist_len = self.history.len();
        let combined_len = hist_len + live_used;
        // Clamp into real content so a focus parked on a blank row below the
        // prompt doesn't emit trailing empty lines.
        let hi_r = hi_r.min(combined_len.saturating_sub(1));
        let mut out = String::new();
        for r in lo_r..=hi_r {
            let line: &str = if r < hist_len {
                &self.history[r].0
            } else if r - hist_len < live.len() {
                &live[r - hist_len].0
            } else {
                ""
            };
            let chars: Vec<char> = line.chars().collect();
            // `c0`/`c1` are GRID COLUMNS (inclusive). The plain text keeps one
            // char per glyph, so wide (CJK) glyphs make char index != column;
            // map columns → char indices via the cell prefix so the copied text
            // doesn't drift by the number of wide glyphs before it (#132).
            let (c0, c1) = if r == lo_r && r == hi_r {
                (lo_c.min(hi_c), lo_c.max(hi_c))
            } else if r == lo_r {
                (lo_c, u16::MAX)
            } else if r == hi_r {
                (0, hi_c)
            } else {
                (0, u16::MAX)
            };
            let prefix = cell_prefix(&chars);
            let start = char_at_cell_start(&prefix, c0 as usize);
            let end = char_after_cell_end(&prefix, c1 as usize);
            let seg: String = if start < end {
                chars[start..end].iter().collect()
            } else {
                String::new()
            };
            out.push_str(seg.trim_end());
            if r != hi_r {
                out.push('\n');
            }
        }
        out
    }

    /// Feed bytes to vt100 and capture scrolled-off lines into history.
    ///
    /// We detect scroll by diffing the screen before/after a `process`, which
    /// can only recover up to one screen of shift per call.  A single large
    /// burst can scroll many screens at once, so we split the input at newline
    /// boundaries into batches of at most ~half a screen of lines and capture
    /// after each — that way no batch ever scrolls more than the diff can see,
    /// and nothing is lost.  (Splitting only on `\n` is safe: VT escape
    /// sequences never contain a newline.)
    pub fn ingest(&mut self, input: &[u8]) {
        // Rewrite HVP (`ESC [ … f`) → CUP (`ESC [ … H`) so vt100 (which only
        // implements `H`) honours btop/htop's absolute cursor positioning.
        let bytes = self.rewrite_hvp(input);
        // Retain the (post-rewrite) stream, capped, so a resize can replay it at
        // the new width and reflow already-printed output (#169).
        self.raw.extend(bytes.iter().copied());
        self.cap_raw();
        self.feed_batched(&bytes);
    }

    /// Feed a (already HVP-rewritten) byte slice to vt100 in newline-bounded
    /// batches, capturing scrolled-off lines into history after each (see the
    /// `ingest` doc comment). Does NOT touch `self.raw`, so it is reused by both
    /// live ingest and resize-reflow replay.
    pub fn feed_batched(&mut self, bytes: &[u8]) {
        let rows = self.parser.screen().size().0 as usize;
        let batch_lines = (rows / 2).max(1);
        let mut start = 0usize;
        let mut nl = 0usize;
        for i in 0..bytes.len() {
            if bytes[i] == b'\n' {
                nl += 1;
                if nl >= batch_lines {
                    self.ingest_chunk(&bytes[start..=i]);
                    start = i + 1;
                    nl = 0;
                }
            }
        }
        if start < bytes.len() {
            self.ingest_chunk(&bytes[start..]);
        }
    }

    /// Trim the retained stream to `RAW_CAP`, dropping from the front up to the
    /// next line boundary so a replay never starts mid-escape / mid-wrapped-line.
    pub fn cap_raw(&mut self) {
        if self.raw.len() <= RAW_CAP {
            return;
        }
        let overflow = self.raw.len() - RAW_CAP;
        self.raw.drain(0..overflow);
        while let Some(&b) = self.raw.front() {
            self.raw.pop_front();
            if b == b'\n' {
                break;
            }
        }
    }

    /// Resize-reflow (#169): rebuild the screen + scrollback at a new width by
    /// replaying the retained byte stream through a fresh parser. vt100 itself
    /// can't reflow (`set_size` just truncates/pads each row), and we only keep
    /// rendered grid rows in `history`, so replaying the raw stream is what lets
    /// long lines rewrap to the new width like FinalShell. Used only on the normal
    /// screen — alt-screen programs (tmux/vim) get a SIGWINCH redraw from the
    /// remote instead.
    pub fn reflow(&mut self, new_rows: u16, new_cols: u16) {
        let stream: Vec<u8> = self.raw.iter().copied().collect();
        self.parser = vt100::Parser::new(new_rows, new_cols, 5000);
        self.history.clear();
        self.prev.clear();
        self.view_offset = 0;
        // Scrollback line count changes, so absolute selection coords no longer map.
        self.sel_anchor = None;
        self.sel_focus = None;
        self.feed_batched(&stream);
    }

    /// Translate every CSI sequence terminated by `f` (HVP) into the identical
    /// sequence terminated by `H` (CUP).  The scanner state persists across
    /// calls, so a sequence split across read chunks is still handled.  Only the
    /// final byte of a CSI sequence is ever touched; text bytes pass through.
    pub fn rewrite_hvp(&mut self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        for &b in input {
            match self.csi_state {
                CsiState::Normal => {
                    if b == 0x1b {
                        self.csi_state = CsiState::Esc;
                    }
                    out.push(b);
                }
                CsiState::Esc => {
                    if b == b'[' {
                        self.csi_state = CsiState::Csi;
                    } else {
                        // Not a CSI (could be another ESC, OSC, etc.).  Re-arm on
                        // a fresh ESC, otherwise fall back to normal text.
                        self.csi_state = if b == 0x1b { CsiState::Esc } else { CsiState::Normal };
                    }
                    out.push(b);
                }
                CsiState::Csi => {
                    // Final bytes are 0x40..=0x7e; params/intermediates are
                    // 0x20..=0x3f.  Rewrite an `f` final into `H`.
                    if (0x40..=0x7e).contains(&b) {
                        out.push(if b == b'f' { b'H' } else { b });
                        self.csi_state = CsiState::Normal;
                    } else {
                        out.push(b);
                    }
                }
            }
        }
        out
    }

    /// Process one bounded batch and capture any lines that scrolled off the top
    /// (skipped for alt-screen programs like vim/nano).
    pub fn ingest_chunk(&mut self, bytes: &[u8]) {
        // Detect full-screen-clear sequences *before* processing so we can
        // suppress history for programs that redraw without alt-screen (e.g.
        // btop configured with `alt-screen = false`).
        // We look for \033[H (cursor-home) and \033[2J / \033[J (erase display)
        // as indicators that the program is doing a full-screen refresh.
        let has_cursor_home   = bytes.windows(3).any(|w| w == b"\x1b[H");
        let has_erase_display = bytes.windows(4).any(|w| w == b"\x1b[2J")
                             || bytes.windows(3).any(|w| w == b"\x1b[J");
        let is_fullscreen_refresh = has_cursor_home && has_erase_display;

        self.parser.process(bytes);
        let (is_alt, rows, cols) = {
            let s = self.parser.screen();
            let (r, c) = s.size();
            (s.alternate_screen(), r, c)
        };
        if is_alt {
            // Snap to live view whenever we're on the alt screen — this
            // prevents old history (accumulated before alt-screen was entered)
            // from mixing with the full-screen program's output after a scroll.
            self.view_offset = 0;
            self.prev.clear();
            return;
        }
        if is_fullscreen_refresh {
            // Non-alt-screen full-screen refresh (btop, htop with alt disabled…).
            // Don't capture lines into history; they'd mix with the next frame.
            self.view_offset = 0;
            self.prev.clear();
            return;
        }
        let curr: Vec<Line> = {
            let s = self.parser.screen();
            (0..rows).map(|r| build_row(s, r, cols)).collect()
        };
        if !self.prev.is_empty() {
            let k = detect_scroll(&self.prev, &curr);
            if k > 0 {
                let drained: Vec<Line> = self.prev.drain(..k).collect();
                self.history.extend(drained);
            }
            if self.history.len() > MAX_HISTORY {
                let drop = self.history.len() - MAX_HISTORY;
                self.history.drain(0..drop);
            }
        }
        self.prev = curr;
    }

    /// Render the terminal grid for the current scrollback `view_offset`
    /// (0 = live).  Caches the displayed plain text for find/selection.
    pub fn render(&mut self) -> BuiltScreen {
        let (is_alt, rows, cols, cur_row, cur_col) = {
            let s = self.parser.screen();
            let (r, c) = s.size();
            let (cr, cc) = s.cursor_position();
            (s.alternate_screen(), r, c, cr, cc)
        };

        self.cached_spans.clear();
        self.cached_displayed.clear();

        // --- Live view (also alt-screen): render the current grid -----------
        if is_alt || self.view_offset == 0 {
            self.cached_displayed.reserve(rows as usize);
            let mut last_content = 0i32;
            let s = self.parser.screen();
            for r in 0..rows {
                let (plain, runs) = build_row(s, r, cols);
                if !runs.is_empty() {
                    last_content = r as i32;
                }
                for hs in runs {
                    self.cached_spans.push(TermSpan {
                        cjk: contains_cjk(&hs.text),
                        text: hs.text.into(),
                        fg: vt_color_to_slint(hs.fg, hs.bold, self.is_dark),
                        bg: vt_bg_to_slint(hs.bg, self.is_dark),
                        bold: hs.bold,
                        row: r as i32,
                        col: hs.col,
                        cells: hs.cells,
                    });
                }
                self.cached_displayed.push(plain.trim_end().to_string());
            }
            self.displayed_text = std::mem::take(&mut self.cached_displayed);
            let rows_used = if is_alt { rows as i32 } else { last_content + 1 };
            return BuiltScreen {
                spans: std::mem::take(&mut self.cached_spans),
                cursor_row: cur_row as i32,
                cursor_col: cur_col as i32,
                rows_used,
                is_alt,
                scroll_max: if is_alt { 0 } else { self.history.len() as i32 },
                scroll_offset: 0,
            };
        }

        // --- Scrolled view: window into history ++ live content -------------
        let live: Vec<Line> = {
            let s = self.parser.screen();
            (0..rows).map(|r| build_row(s, r, cols)).collect()
        };
        let hist_len = self.history.len();
        // Include the screen's trailing blank rows in the scroll range so this
        // scrolled view stays continuous with the live view (view_offset 0).
        // Trimming to only the used rows made the two views misalign after a
        // shrink-then-grow (dragging the SFTP panel over the terminal and back),
        // so scrolling back jumped at the bottom instead of moving line-by-line
        // (#119-followup).
        let combined_len = hist_len + live.len();
        let win = rows as usize;
        let start = combined_len.saturating_sub(win + self.view_offset);
        let end = (start + win).min(combined_len);

        self.cached_displayed.reserve(win);
        for (d, idx) in (start..end).enumerate() {
            let line: &Line = if idx < hist_len {
                &self.history[idx]
            } else {
                &live[idx - hist_len]
            };
            for hs in &line.1 {
                self.cached_spans.push(TermSpan {
                    text: hs.text.clone().into(),
                    fg: vt_color_to_slint(hs.fg, hs.bold, self.is_dark),
                    bg: vt_bg_to_slint(hs.bg, self.is_dark),
                    bold: hs.bold,
                    row: d as i32,
                    col: hs.col,
                    cells: hs.cells,
                    cjk: contains_cjk(&hs.text),
                });
            }
            self.cached_displayed.push(line.0.trim_end().to_string());
        }
        while self.cached_displayed.len() < win {
            self.cached_displayed.push(String::new());
        }
        self.displayed_text = std::mem::take(&mut self.cached_displayed);
        BuiltScreen {
            spans: std::mem::take(&mut self.cached_spans),
            cursor_row: -1, // hide the live cursor while viewing history
            cursor_col: 0,
            rows_used: win as i32,
            is_alt: false,
            scroll_max: self.history.len() as i32,
            scroll_offset: self.view_offset as i32,
        }
    }
}

/// True if a terminal span contains any CJK character — ideograph, kana, or
/// (crucially) CJK punctuation like 。，. The mono terminal font has no CJK
/// glyphs and Slint's per-script fallback tofu's *isolated* CJK punctuation
/// (it renders fine only when adjacent to a Han char), so these spans are drawn
/// with the CJK-capable UI font instead (#54). Box-drawing / powerline glyphs
/// are deliberately excluded so they keep the aligned monospace font.
pub fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c as u32,
            0x2E80..=0x2EFF       // CJK radicals
            | 0x3000..=0x303F     // CJK symbols & punctuation (、。「」…)
            | 0x3040..=0x30FF     // hiragana + katakana
            | 0x3100..=0x312F     // bopomofo
            | 0x3400..=0x4DBF     // CJK ext A
            | 0x4E00..=0x9FFF     // CJK unified ideographs
            | 0xF900..=0xFAFF     // CJK compatibility ideographs
            | 0xFF00..=0xFFEF     // fullwidth / halfwidth forms (，！？：；)
            | 0x20000..=0x2FA1F)  // CJK ext B–F + compat supplement
    })
}

/// 16-colour ANSI palette for **dark** terminals (VS Code "Dark+" values).
pub const ANSI16_DARK: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00), // 0  black
    (0xcd, 0x31, 0x31), // 1  red
    (0x0d, 0xbc, 0x79), // 2  green
    (0xe5, 0xe5, 0x10), // 3  yellow
    (0x24, 0x72, 0xc8), // 4  blue
    (0xbc, 0x3f, 0xbc), // 5  magenta
    (0x11, 0xa8, 0xcd), // 6  cyan
    (0xe5, 0xe5, 0xe5), // 7  white        (light grey on dark bg)
    (0x66, 0x66, 0x66), // 8  bright black
    (0xf1, 0x4c, 0x4c), // 9  bright red
    (0x23, 0xd1, 0x8b), // 10 bright green
    (0xf5, 0xf5, 0x43), // 11 bright yellow
    (0x3b, 0x8e, 0xea), // 12 bright blue
    (0xd6, 0x70, 0xd6), // 13 bright magenta
    (0x29, 0xb8, 0xdb), // 14 bright cyan
    (0xff, 0xff, 0xff), // 15 bright white
];

/// 16-colour ANSI palette for **light** terminal **foreground** (text) use.
///
/// On a near-white (#fafafa) background, the standard "white" (slot 7) and
/// "bright white" (slot 15) are nearly invisible.  We remap them to dark greys
/// so `ls`, `git` and other tools that use colour 7 for regular text stay
/// perfectly readable.  Saturated hues are darkened for contrast.
pub const ANSI16_LIGHT: [(u8, u8, u8); 16] = [
    (0x1c, 0x1c, 0x1e), // 0  black        → Apple near-black
    (0xc0, 0x39, 0x2b), // 1  red
    (0x1a, 0x7f, 0x37), // 2  green        → darker for white bg
    (0x85, 0x64, 0x04), // 3  yellow       → dark amber, readable
    (0x04, 0x51, 0xa5), // 4  blue         → VS Code light blue
    (0x80, 0x00, 0x80), // 5  magenta
    (0x0e, 0x72, 0x5c), // 6  cyan         → darker teal
    (0x3a, 0x3a, 0x3c), // 7  white        → dark grey (was 0xe5e5e5, near-invisible)
    (0x55, 0x55, 0x55), // 8  bright black
    (0xe7, 0x4c, 0x3c), // 9  bright red
    (0x27, 0xae, 0x60), // 10 bright green
    (0xd4, 0xac, 0x0d), // 11 bright yellow
    (0x2e, 0x86, 0xc1), // 12 bright blue
    (0x9b, 0x59, 0xb6), // 13 bright magenta
    (0x1a, 0xbc, 0x9c), // 14 bright cyan
    (0x2c, 0x2c, 0x2e), // 15 bright white → dark (was 0xffffff, near-invisible)
];

/// 16-colour ANSI palette for **light** terminal **background** (fill) use.
///
/// When TUI programs (btop, htop, vim) paint cell backgrounds in light mode,
/// each colour maps to a light-tinted variant so the overall UI feels light.
/// "Black" (slot 0) becomes a very light grey rather than near-black, so
/// dark-background TUI apps naturally inherit a light appearance.  Foreground
/// text always uses `ANSI16_LIGHT` so readability is unaffected.
pub const ANSI16_LIGHT_BG: [(u8, u8, u8); 16] = [
    (0xe8, 0xe8, 0xed), // 0  black        → Apple system-grey-6 (very light)
    (0xff, 0xd5, 0xd5), // 1  red          → light rose
    (0xd5, 0xf5, 0xd5), // 2  green        → light mint
    (0xff, 0xf8, 0xd5), // 3  yellow       → light cream
    (0xd5, 0xe8, 0xf8), // 4  blue         → light sky
    (0xf5, 0xd5, 0xf5), // 5  magenta      → light lilac
    (0xd5, 0xf5, 0xf8), // 6  cyan         → light aqua
    (0xf5, 0xf5, 0xf7), // 7  white        → Apple bg (near-white)
    (0xd1, 0xd1, 0xd6), // 8  bright black → Apple system-grey-4
    (0xff, 0xbe, 0xbe), // 9  bright red   → light salmon
    (0xbe, 0xf5, 0xbe), // 10 bright green
    (0xf5, 0xf5, 0xbe), // 11 bright yellow
    (0xbe, 0xdd, 0xff), // 12 bright blue  → light periwinkle
    (0xf0, 0xbe, 0xff), // 13 bright magenta → light violet
    (0xbe, 0xf5, 0xff), // 14 bright cyan
    (0xff, 0xff, 0xff), // 15 bright white → white
];

/// Convert a vt100 foreground colour (+ bold) to a Slint colour.
/// Bold + a base colour (0–7) maps to the bright variant (8–15), matching
/// how terminals render `ls --color` (bold-green executables, bold-blue dirs).
///
/// In light mode, true-colour RGB foregrounds that are light (HSL lightness
/// ≥ 0.55) are darkened so they remain readable on a near-white background.
pub fn vt_color_to_slint(color: vt100::Color, bold: bool, is_dark: bool) -> slint::Color {
    let (r, g, b) = match color {
        vt100::Color::Default => {
            if is_dark { (0xd4, 0xd4, 0xd4) } else { (0x2d, 0x2d, 0x2f) }
        }
        vt100::Color::Idx(i) => idx_to_rgb(i, bold, is_dark),
        vt100::Color::Rgb(r, g, b) => {
            if is_dark { (r, g, b) } else { darken_light_fg(r, g, b) }
        }
    };
    slint::Color::from_rgb_u8(r, g, b)
}

/// In light mode, remap light true-colour foregrounds to dark so they are
/// readable on a near-white background.  Colours already dark (L < 0.55)
/// pass through unchanged.
pub fn darken_light_fg(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (h, s, l) = rgb_to_hsl(r, g, b);
    if l < 0.55 {
        return (r, g, b);
    }
    // L=0.55 → 0.40 (readable dark grey), L=1.0 (white) → ~0.15 (near-black).
    let new_l = (0.40 - (l - 0.55) * 0.56).max(0.10);
    hsl_to_rgb(h, s, new_l)
}

/// Convert a vt100 *background* colour to Slint.  The default background maps
/// to fully transparent so we don't paint a fill over the terminal's own bg.
/// Non-default backgrounds (btop/htop bars, selected rows) become opaque.
///
/// In light mode:
/// - ANSI 16 colours use `ANSI16_LIGHT_BG` (light pastels).
/// - True-colour RGB backgrounds that are dark (HSL lightness < 0.45) are
///   remapped to light pastels so programs like btop feel light-themed.
pub fn vt_bg_to_slint(color: vt100::Color, is_dark: bool) -> slint::Color {
    match color {
        vt100::Color::Default => slint::Color::from_argb_u8(0, 0, 0, 0), // transparent
        vt100::Color::Idx(i) => {
            let (r, g, b) = idx_to_rgb_bg(i, is_dark);
            slint::Color::from_rgb_u8(r, g, b)
        }
        vt100::Color::Rgb(r, g, b) => {
            if is_dark {
                slint::Color::from_rgb_u8(r, g, b)
            } else {
                let (nr, ng, nb) = lighten_dark_bg(r, g, b);
                slint::Color::from_rgb_u8(nr, ng, nb)
            }
        }
    }
}

/// In light mode, remap dark true-colour backgrounds to light pastels.
/// Colours whose HSL lightness is already ≥ 0.45 pass through unchanged
/// (the program chose a light colour deliberately).
pub fn lighten_dark_bg(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (h, s, l) = rgb_to_hsl(r, g, b);
    if l >= 0.45 {
        return (r, g, b);
    }
    // Remap: darkest (l≈0) → very light (l≈0.92); l=0.45 → l≈0.84.
    // Reduce saturation to pastel so colours don't look garish on white.
    let new_l = 0.92 - l * 0.18;
    let new_s = (s * 0.35).min(0.25);
    hsl_to_rgb(h, new_s, new_l)
}

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < 1e-6 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, s, l)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    if s < 1e-6 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| -> f32 {
        if t < 0.0 { t += 1.0; }
        if t > 1.0 { t -= 1.0; }
        if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
        if t < 0.5 { return q; }
        if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
        p
    };
    (
        (hue(h + 1.0 / 3.0) * 255.0).round() as u8,
        (hue(h) * 255.0).round() as u8,
        (hue(h - 1.0 / 3.0) * 255.0).round() as u8,
    )
}

/// Map an xterm-256 palette index to RGB (16 ANSI + 6×6×6 cube + grayscale).
pub fn idx_to_rgb(i: u8, bold: bool, is_dark: bool) -> (u8, u8, u8) {
    let i = if bold && i < 8 { i + 8 } else { i };
    let palette = if is_dark { &ANSI16_DARK } else { &ANSI16_LIGHT };
    match i {
        0..=15 => palette[i as usize],
        16..=231 => {
            let n = i - 16;
            let to = |v: u8| -> u8 {
                if v == 0 { 0 } else { 55 + v * 40 }
            };
            (to(n / 36), to((n % 36) / 6), to(n % 6))
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
    }
}

/// Same as [`idx_to_rgb`] but for **background** fills in light mode: the 16
/// ANSI base colours use `ANSI16_LIGHT_BG` (light pastels) so TUI program
/// backgrounds feel light.  256-colour cube / grayscale are used as-is.
pub fn idx_to_rgb_bg(i: u8, is_dark: bool) -> (u8, u8, u8) {
    if !is_dark && i < 16 {
        return ANSI16_LIGHT_BG[i as usize];
    }
    idx_to_rgb(i, false, is_dark)
}
