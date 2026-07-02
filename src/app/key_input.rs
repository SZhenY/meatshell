use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use parking_lot::Mutex;

use slint::{ComponentHandle, SharedString, VecModel};

use crate::ssh::SessionHandle;
use super::*;

const IME_BACKSPACE_WINDOW_MS: u128 = 1500;
const RESIZE_DEBOUNCE_MS: u64 = 150;

// ---------------------------------------------------------------------------
// Raw keystroke forwarding and PTY resize
// ---------------------------------------------------------------------------

pub(crate) fn wire_key_input(
    window: &AppWindow,
    handles: Rc<RefCell<HashMap<String, SessionHandle>>>,
    bufs: TermBuffers,
    last_term_size: Arc<Mutex<(u32, u32)>>,
    store: Rc<RefCell<ConfigStore>>,
    ctx: ConnectCtx,
) {
    // --- Command bar (#55): run command + quick-command management ---------
    {
        let handles_rc = handles.clone();
        let store_rc = store.clone();
        let weak = window.as_weak();
        window.on_run_command(move |tab_id: SharedString, cmd: SharedString, to_all: bool| {
            let line = cmd.trim_end().to_string();
            if line.is_empty() {
                return;
            }
            let mut bytes = line.clone().into_bytes();
            bytes.push(b'\n');
            {
                let h = handles_rc.borrow();
                if to_all {
                    for handle in h.values() {
                        handle.send_raw(bytes.clone());
                    }
                } else if let Some(handle) = h.get(tab_id.as_str()) {
                    handle.send_raw(bytes);
                }
            }
            {
                let mut s = store_rc.borrow_mut();
                s.push_command_history(line);
                let _ = s.save();
            }
            if let Some(w) = weak.upgrade() {
                w.set_command_history(history_model(&store_rc.borrow()));
            }
        });
    }
    // Copy a history command to the clipboard (#96).
    {
        window.on_copy_text(move |text: SharedString| {
            let t = text.to_string();
            std::thread::spawn(move || clipboard_set_text(t));
        });
    }
    // Delete a history entry (#96). The model is in storage order now (#113),
    // so the row index maps straight through.
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        window.on_delete_history(move |i: i32| {
            {
                let mut s = store_rc.borrow_mut();
                let idx = i as usize;
                if idx < s.command_history().len() {
                    s.remove_command_history(idx);
                    let _ = s.save();
                }
            }
            if let Some(w) = weak.upgrade() {
                w.set_command_history(history_model(&store_rc.borrow()));
            }
        });
    }
    // History search (#101): filter the dropdown by a case-insensitive substring.
    // The current query is shared so a delete from a filtered view re-filters.
    let hist_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let hist_query = hist_query.clone();
        window.on_search_history(move |query: SharedString| {
            *hist_query.borrow_mut() = query.to_string();
            if let Some(w) = weak.upgrade() {
                w.set_history_view(history_view_model(&store_rc.borrow(), &query));
            }
        });
    }
    // Delete a history entry by its command text (#101) — index-free so it works
    // from the filtered dropdown view.
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let hist_query = hist_query.clone();
        window.on_delete_history_cmd(move |cmd: SharedString| {
            {
                let mut s = store_rc.borrow_mut();
                if let Some(idx) = s.command_history().iter().position(|c| c == cmd.as_str()) {
                    s.remove_command_history(idx);
                    let _ = s.save();
                }
            }
            if let Some(w) = weak.upgrade() {
                let s = store_rc.borrow();
                w.set_command_history(history_model(&s));
                w.set_history_view(history_view_model(&s, &hist_query.borrow()));
            }
        });
    }
    // Runtime-only collapse state for quick-command groups (#55) — like the
    // welcome session groups, this is not persisted across restarts. Starts with
    // every group collapsed (default-collapsed view).
    let collapsed_quick_groups: Rc<RefCell<std::collections::HashSet<String>>> =
        Rc::new(RefCell::new(all_quick_group_names(&store.borrow())));
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_add_quick_command(
            move |name: SharedString, command: SharedString, group: SharedString, send_enter: bool| {
                let name = name.trim().to_string();
                let command = command.to_string();
                let group = group.trim().to_string();
                if name.is_empty() || command.trim().is_empty() {
                    return;
                }
                {
                    let mut s = store_rc.borrow_mut();
                    let mut v = s.quick_commands().to_vec();
                    v.push(crate::config::QuickCommand {
                        name,
                        command,
                        group,
                        send_enter,
                    });
                    s.set_quick_commands(v);
                    let _ = s.save();
                }
                if let Some(w) = weak.upgrade() {
                    w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
                }
            },
        );
    }
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_delete_quick_command(move |index: i32| {
            {
                let mut s = store_rc.borrow_mut();
                let mut v = s.quick_commands().to_vec();
                let i = index as usize;
                if i < v.len() {
                    v.remove(i);
                }
                s.set_quick_commands(v);
                let _ = s.save();
            }
            if let Some(w) = weak.upgrade() {
                w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
            }
        });
    }
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_toggle_quick_group(move |group: SharedString| {
            let g = group.to_string();
            {
                let mut set = collapsed.borrow_mut();
                if !set.remove(&g) {
                    set.insert(g);
                }
            }
            if let Some(w) = weak.upgrade() {
                w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
            }
        });
    }
    // Edit (#55): load the entry into the manage form in edit mode.
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        window.on_edit_quick_command(move |index: i32| {
            let i = index as usize;
            let cmd = store_rc.borrow().quick_commands().get(i).cloned();
            if let (Some(c), Some(w)) = (cmd, weak.upgrade()) {
                w.set_qcm_name(c.name.into());
                w.set_qcm_command(c.command.into());
                w.set_qcm_group(c.group.into());
                w.set_qcm_send_enter(c.send_enter);
                w.set_qcm_edit_index(index);
                w.set_quick_cmd_manage_open(true);
            }
        });
    }
    // Save an edited entry (#55).
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_save_quick_command(
            move |index: i32, name: SharedString, command: SharedString, group: SharedString, send_enter: bool| {
                let name = name.trim().to_string();
                let command = command.to_string();
                let group = group.trim().to_string();
                if name.is_empty() || command.trim().is_empty() {
                    return;
                }
                {
                    let mut s = store_rc.borrow_mut();
                    s.update_quick_command(
                        index as usize,
                        crate::config::QuickCommand {
                            name,
                            command,
                            group,
                            send_enter,
                        },
                    );
                    let _ = s.save();
                }
                if let Some(w) = weak.upgrade() {
                    w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
                }
            },
        );
    }
    // Duplicate (#55): clone the entry as a starting point.
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_duplicate_quick_command(move |index: i32| {
            {
                let mut s = store_rc.borrow_mut();
                let mut v = s.quick_commands().to_vec();
                if let Some(c) = v.get(index as usize).cloned() {
                    let dup = crate::config::QuickCommand {
                        name: format!("{} (copy)", c.name),
                        command: c.command,
                        group: c.group,
                        send_enter: c.send_enter,
                    };
                    v.insert(index as usize + 1, dup);
                    s.set_quick_commands(v);
                    let _ = s.save();
                }
            }
            if let Some(w) = weak.upgrade() {
                w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
            }
        });
    }
    // Move to a group (#55): "default" maps to the empty (ungrouped) group.
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_move_quick_command(move |index: i32, group: SharedString| {
            let target = group.to_string();
            let target = if target == "default" { String::new() } else { target };
            {
                let mut s = store_rc.borrow_mut();
                let mut v = s.quick_commands().to_vec();
                if let Some(c) = v.get_mut(index as usize) {
                    c.group = target;
                }
                s.set_quick_commands(v);
                let _ = s.save();
            }
            if let Some(w) = weak.upgrade() {
                w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
            }
        });
    }
    // Quick-group create / rename (#55).
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_submit_quick_group(move |orig: SharedString, name: SharedString| {
            {
                let mut s = store_rc.borrow_mut();
                if orig.is_empty() {
                    s.add_quick_group(name.to_string());
                } else {
                    s.rename_quick_group(&orig.to_string(), name.to_string());
                }
                let _ = s.save();
            }
            if let Some(w) = weak.upgrade() {
                w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
            }
        });
    }
    // Quick-group delete (#55) — UI only offers this on empty groups.
    {
        let store_rc = store.clone();
        let weak = window.as_weak();
        let collapsed = collapsed_quick_groups.clone();
        window.on_delete_quick_group(move |name: SharedString| {
            {
                let mut s = store_rc.borrow_mut();
                s.remove_quick_group(&name.to_string());
                let _ = s.save();
            }
            if let Some(w) = weak.upgrade() {
                w.set_quick_commands(quick_cmd_model(&store_rc.borrow(), &collapsed.borrow()));
            }
        });
    }

    // Session sync / broadcast input: when on, a keystroke in any terminal is
    // mirrored to every online session (Xshell-style; #78 pt.4). Read on the hot
    // keystroke path, so use an AtomicBool rather than a window-property lookup.
    let sync_input = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let flag = sync_input.clone();
        window.on_set_sync_input(move |on| {
            flag.store(on, std::sync::atomic::Ordering::Relaxed);
        });
    }

    // Forward each keystroke as raw bytes to the SSH PTY. The server's bash /
    // readline handles echo, history (↑↓), Tab completion, Ctrl+C, etc.
    {
        let handles = handles.clone();
        let bufs = bufs.clone();
        let sync_input = sync_input.clone();
        // Shared timestamp: the last time the Shift key alone was pressed
        // (key="", shift=true).  Used by the time-based Backspace filter below.
        let last_shift_time: Arc<Mutex<Option<std::time::Instant>>> =
            Arc::new(Mutex::new(None));
        window.on_send_key(move |tab_id: SharedString, key: SharedString, ctrl: bool, alt: bool, shift: bool| {
            // ── Enter on a disconnected tab → reconnect in place (#79) ──────
            // FinalShell-style: the tab shows "连接已断开,按 Enter 重新连接";
            // pressing Enter re-spawns the shell + SFTP workers in the SAME tab
            // with a fresh screen instead of forcing the user to open a new one.
            if key.as_str() == "\n" && !ctrl && !alt {
                let dead_session = {
                    let statuses = ctx.tab_statuses.lock();
                    statuses
                        .get(tab_id.as_str())
                        .filter(|st| st.state == 2)
                        .map(|st| st.session_id.clone())
                };
                if let Some(session_id) = dead_session {
                    let Some(session) = store.borrow().get(&session_id).cloned() else {
                        return;
                    };
                    // Drop the dead shell/SFTP handles for this tab.
                    ctx.handles.borrow_mut().remove(tab_id.as_str());
                    if let Some(h) =
                        ctx.sftp_handles.lock().remove(tab_id.as_str())
                    {
                        h.close();
                    }
                    // Fresh screen: new parser, cleared history/selection.
                    {
                        let mut map = ctx.bufs.lock();
                        if let Some(b) = map.get_mut(tab_id.as_str()) {
                            let (rows, cols) = b.parser.screen().size();
                            b.parser = vt100::Parser::new(rows, cols, 5000);
                            b.history.clear();
                            b.prev.clear();
                            b.displayed_text.clear();
                            b.view_offset = 0;
                            b.sel_anchor = None;
                            b.sel_focus = None;
                            b.raw.clear();
                        }
                    }
                    if let Some(st) =
                        ctx.tab_statuses.lock().get_mut(tab_id.as_str())
                    {
                        st.state = 0;
                    }
                    // Fresh session: the first OSC 7 after reconnect follows.
                    ctx.sftp_last_cwd.lock().remove(tab_id.as_str());
                    if let Some(w) = ctx.weak.upgrade() {
                        set_terminal_row(&w, tab_id.as_str(), |t| {
                            t.status =
                                crate::i18n::t("重连中...", "Reconnecting...").into();
                        });
                    }
                    start_session_in_tab(tab_id.as_str(), session, &ctx);
                    return;
                }
            }
            // Check whether the remote PTY switched to application cursor mode
            // (DECCKM, set by nano/vim via \x1b[?1h). In that mode the terminal
            // must send \x1bOA/B/C/D instead of \x1b[A/B/C/D.
            let app_cursor = {
                let mut map = bufs.lock();
                match map.get_mut(tab_id.as_str()) {
                    Some(b) => {
                        // Typing snaps the view back to the live bottom so the
                        // user always sees what they're entering.
                        b.view_offset = 0;
                        b.parser.screen().application_cursor()
                    }
                    None => false,
                }
            };
            // Never log the raw key string — it can be a password character
            // (#15). redact_key keeps control codes but masks printable text.
            tracing::debug!(
                "send_key tab={} key={} ctrl={} alt={} shift={} app_cursor={}",
                tab_id, redact_key(key.as_str()), ctrl, alt, shift, app_cursor
            );

            // ── Shift / Backspace 诊断日志 (info 级, 无需 RUST_LOG=debug) ─────
            // 每个 Shift 相关事件都打印 key 的 Unicode 码位，方便对比
            // 左Shift / 右Shift 是否产生不同的 key 字符串。
            if shift || key.as_str() == "\u{0008}" {
                // INFO level (no RUST_LOG needed) — must not leak the key text.
                // redact_key reveals only control code points (the IME markers
                // this diagnostic cares about), masking any printable char that
                // could be part of a Shift-typed password symbol (#15).
                let codepoints = redact_key(key.as_str());
                let elapsed_ms = (*last_shift_time.lock())
                    .map(|t| format!("{}ms ago", t.elapsed().as_millis()))
                    .unwrap_or_else(|| "never".to_string());
                tracing::info!(
                    "[KEY_DIAG] key={} shift={} ctrl={} alt={} | last_shift={}",
                    codepoints, shift, ctrl, alt, elapsed_ms
                );
            }

            // ── Track lone-Shift presses for the time-based Backspace filter ──
            // Slint sends key="" (empty string) when a bare modifier key (Shift,
            // Ctrl, Alt) is pressed.  We record the timestamp whenever Shift
            // alone fires so the filter below can catch IME-injected Backspace
            // events even if they arrive with shift=false.
            if key.as_str().is_empty() && shift && !ctrl && !alt {
                *last_shift_time.lock() = Some(std::time::Instant::now());
                tracing::info!("[KEY_DIAG] lone-Shift recorded → timestamp saved");
            }

            // ── 拦截百度拼音注入的 Shift 标记字符（核心修复）────────────────────
            // 诊断日志证实，百度拼音通过 WH_KEYBOARD_LL 钩子，在 Shift 键按下时
            // 向消息队列注入一个 C0 控制字符，而非空字符串：
            //
            //   左 Shift → U+0015 (Ctrl+U / NAK), shift=true, ctrl=false
            //   右 Shift → U+0010 (Ctrl+P / DLE), shift=true, ctrl=false
            //              紧接着注入: U+0008 (Backspace), shift=false
            //
            // 这些字符绝对不应送入 PTY：
            //   0x15 (Ctrl+U) 在 bash/vim 中会清空当前输入行 → "左Shift替换字符"
            //   0x10 (Ctrl+P) 在 vim 中翻历史/触发补全     → "右Shift乱跳"
            //   0x08 (Backspace) 紧随其后                   → "右Shift删除字符"
            //
            // 合法独立 C0 键（Backspace=0x08, Tab=0x09, LF=0x0A, CR=0x0D,
            // ESC=0x1B）不受此过滤影响，由下方代码单独处理。
            //
            // 检测到 IME Shift 标记后，记录时间戳，让 Layer 2 在 1500ms 内
            // 拦截随后可能到来的 Backspace（右Shift场景，日志显示间隔约 914ms）。
            if !ctrl && !alt {
                if let Some(c) = key.as_str().chars().next() {
                    let cp = c as u32;
                    let is_standalone = matches!(cp, 0x08 | 0x09 | 0x0A | 0x0D | 0x1B);
                    if key.as_str().chars().count() == 1
                        && (0x01..=0x1f).contains(&cp)
                        && !is_standalone
                    {
                        *last_shift_time.lock() = Some(std::time::Instant::now());
                        tracing::info!(
                            "[KEY_DIAG] DROPPED IME C0 marker U+{:04X} (shift={}) → timestamp saved",
                            cp, shift
                        );
                        return;
                    }
                }
            }

            // ── Windows: filter synthetic Ctrl+char injections ──────────────
            // Some keyboards / IME drivers (e.g. Aula F99 + Baidu Pinyin)
            // inject a synthetic WM_CHAR 0x11 (Ctrl+Q) when Left Ctrl is
            // briefly tapped, WITHOUT sending a WM_KEYDOWN VK_Q beforehand.
            //
            // FinalShell avoids this because it builds Ctrl+letter from
            // WM_KEYDOWN (virtual-key codes).  Slint uses WM_CHAR, so it
            // sees the injected byte and forwards it straight to us.
            //
            // Fix: for C0 control chars (Ctrl+A…Ctrl+Z, i.e. 0x01–0x1A),
            // use GetKeyState — which returns the key state *as of the last
            // processed message*, not the live hardware state — to verify
            // the corresponding letter VK was actually queued as a keydown
            // before this WM_CHAR arrived.  If Q was never keyed down,
            // GetKeyState(VK_Q) = 0 → the event is synthetic → drop it.
            #[cfg(windows)]
            if ctrl {
                if let Some(ch) = key.as_str().chars().next() {
                    let cp = ch as u32;
                    // Always let Enter / Tab pass through regardless of Ctrl
                    // state.  These C0 codes (0x09 Tab, 0x0a LF, 0x0d CR) are
                    // "double-duty" keys: pressing Enter while Ctrl is still
                    // physically held (e.g. just after Ctrl+O in nano) generates
                    // Ctrl+M (0x0d) with ctrl=true — but GetKeyState(VK_M) is 0
                    // because the user never pressed M.  Without this exemption
                    // the filter would silently drop the Enter, making it
                    // impossible to confirm nano's "File Name to Write:" prompt.
                    let always_pass = matches!(cp, 0x09 | 0x0a | 0x0d);
                    if !always_pass
                        && key.as_str().chars().count() == 1
                        && (0x01..=0x1a).contains(&cp)
                        && !c0_letter_key_down(cp)
                    {
                        tracing::debug!(
                            "send_key: dropped synthetic Ctrl+{} \
                             (VK_{:02X} not down per GetKeyState)",
                            (0x40u8 + cp as u8) as char,
                            cp + 0x40
                        );
                        return;
                    }
                }
            }

            // ── Filter synthetic Backspace injected by Chinese IME ────────────
            // Baidu Pinyin (and similar Chinese IMEs) hooks the keyboard at the
            // driver level via WH_KEYBOARD_LL, below Win32's ImmDisableIME.
            // When the user presses Shift to switch from Chinese to English mode
            // while a pinyin syllable is in-flight, the IME:
            //   1. Cancels the composition (discards the syllable).
            //   2. Posts WM_KEYDOWN VK_BACK + WM_CHAR 0x08 to erase whatever
            //      character it had already forwarded to the app.
            //
            // Three-layer defence:
            //
            //   Layer 1 – shift=true guard.
            //     The synthetic Backspace arrives during Shift keydown, so
            //     GetKeyState(VK_SHIFT) is still "down" → Slint reports shift=true.
            //     Drop any Backspace (0x08) arriving while Shift is flagged.
            //
            //   Layer 2 – time-based guard.
            //     Baidu Pinyin posts WM_CHAR 0x08 asynchronously, so by the time
            //     the message is dequeued Shift may already read as "up"
            //     → shift=false defeats Layer 1.
            //     Mitigation: we recorded the timestamp when the Shift key alone
            //     was pressed (key="", shift=true) a few lines above.  Drop any
            //     Backspace arriving within 200 ms of that moment.
            //
            //   Layer 3 – GetKeyState guard (belt-and-suspenders).
            //     If VK_BACK is not actually "down" (i.e. no real WM_KEYDOWN
            //     VK_BACK was ever queued), the Backspace must be synthetic.
            if key.as_str() == "\u{0008}" && !ctrl && !alt {
                // Layer 1
                if shift {
                    tracing::info!("[KEY_DIAG] Backspace DROPPED by layer-1 (shift=true)");
                    return;
                }
                // Layer 2 — 时间窗口 1500ms
                // 日志显示百度拼音注入 U+0010(右Shift标记) 到 Backspace 之间
                // 间隔约 914ms，因此窗口设为 1500ms 以覆盖该场景。
                let (shift_just_pressed, elapsed_ms) = {
                    let guard = last_shift_time.lock();
                    match *guard {
                        Some(t) => {
                            let ms = t.elapsed().as_millis();
                            (ms < IME_BACKSPACE_WINDOW_MS, ms)
                        }
                        None => (false, 0),
                    }
                };
                if shift_just_pressed {
                    tracing::info!(
                        "[KEY_DIAG] Backspace DROPPED by layer-2 ({}ms after IME Shift marker)",
                        elapsed_ms
                    );
                    return;
                }
                // Layer 3
                #[cfg(windows)]
                if !is_vk_back_down() {
                    tracing::info!("[KEY_DIAG] Backspace DROPPED by layer-3 (VK_BACK not down)");
                    return;
                }
                tracing::info!("[KEY_DIAG] Backspace PASSED all filters → sent to PTY");
            }

            let bytes = key_to_pty_bytes(key.as_str(), ctrl, alt, app_cursor);
            // Log only the length — never the keystroke bytes, which can be
            // password characters (#15).
            tracing::debug!(
                "send_key len={} handle_exists={}",
                bytes.len(),
                handles.borrow().contains_key(tab_id.as_str()),
            );
            if !bytes.is_empty() {
                let h = handles.borrow();
                if sync_input.load(std::sync::atomic::Ordering::Relaxed) {
                    // Broadcast the same bytes to every online session (#78 pt.4).
                    for handle in h.values() {
                        handle.send_raw(bytes.clone());
                    }
                } else if let Some(handle) = h.get(tab_id.as_str()) {
                    handle.send_raw(bytes);
                }
            }
        });
    }

    // Propagate PTY resize to the SSH worker and vt100 parser. Pixel
    // dimensions come from Slint; we approximate col/row counts using
    // Consolas 13px metrics.
    //
    // terminal_view.slint now passes the FocusScope height (not the full
    // TerminalView height), so the SFTP panel is already excluded.
    // Layout breakdown for the FocusScope:
    //   16 px  – bottom strip (TouchArea for focus-regain)
    //    8 px  – y-offset of the output Text element inside the Flickable
    // = 24 px  total vertical chrome within FocusScope
    //
    // Consolas 13 px renders at ≈ 8 px wide × 16 px tall per cell.
    {
        let handles = handles.clone();
        let bufs_resize = bufs.clone(); // keep bufs alive for the copy handler below
        let weak_resize = window.as_weak();
        // The Slint side now measures the real Consolas cell size (via a hidden
        // probe Text) and passes whole column/row counts directly, so there is
        // no pixel→cell guesswork here.  This keeps full-screen programs like
        // nano from over-counting rows and clipping their bottom shortcut bar.
        // Debounce PTY resizes (#163): a layout reflow (a tab becoming visible,
        // the SFTP panel docking, a window drag) can momentarily report a
        // near-zero width, which collapses term-cols to its 10-col floor.
        // Applying that to the remote PTY immediately resizes the server to 10
        // columns and reflows vt100 — garbling running output (e.g. a `git clone`
        // progress meter wraps at 10 chars). Coalesce rapid changes and apply
        // only the size that's still set after a short quiet period, so a
        // transient bad value never reaches the server.
        let pending_size: Rc<RefCell<HashMap<String, (u32, u32)>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let resize_debounce = Rc::new(slint::Timer::default());
        window.on_terminal_resize(move |tab_id: SharedString, cols_f: f32, rows_f: f32| {
            // A hidden terminal (inactive tab, or a split sibling not currently
            // shown) reports 0 width/height. Ignore those: flooring 0 to the 10-col
            // minimum and applying it would shrink that tab's PTY *and* poison
            // `last_term_size`, so the next connection (e.g. "Duplicate connection")
            // would start at 10 cols and wrap its first output to ~10 chars (#v0.5).
            // Only genuine, visible sizes drive a resize.
            if cols_f < 1.0 || rows_f < 1.0 {
                return;
            }
            let cols = (cols_f as u32).max(10);
            let rows = (rows_f as u32).max(5);
            pending_size
                .borrow_mut()
                .insert(tab_id.to_string(), (cols, rows));
            let pending = pending_size.clone();
            let handles = handles.clone();
            let bufs = bufs_resize.clone();
            let last = last_term_size.clone();
            let weak = weak_resize.clone();
            // (Re)arm the single-shot timer; rapid changes keep resetting it so
            // only the final, settled size is applied.
            resize_debounce.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(RESIZE_DEBOUNCE_MS),
                move || {
                    let settled: Vec<(String, (u32, u32))> =
                        pending.borrow_mut().drain().collect();
                    for (tab, (cols, rows)) in settled {
                        tracing::debug!("terminal_resize tab={} cols={} rows={}", tab, cols, rows);
                        apply_terminal_resize(&handles, &bufs, &last, &tab, cols, rows);
                        // Re-render so the reflowed (or resized) grid shows at once
                        // instead of waiting for the next remote output (#169).
                        if let Some(win) = weak.upgrade() {
                            rebuild_tab_display(&win, &bufs, &tab);
                        }
                    }
                },
            );
        });
    }

    // Ctrl+Shift+C: copy current terminal screen to clipboard.
    {
        let bufs = bufs.clone();
        window.on_copy_terminal_text(move |tab_id: SharedString| {
            let text = {
                let map = bufs.lock();
                match map.get(tab_id.as_str()) {
                    Some(buf) => {
                        // Copy the drag-selection when there is one, else the
                        // whole displayed screen.
                        let sel = buf.extract_selection_text();
                        if sel.is_empty() {
                            buf.displayed_text.join("\n")
                        } else {
                            sel
                        }
                    }
                    None => String::new(),
                }
            };
            // Run the clipboard write on a dedicated OS thread.  arboard's
            // Windows backend opens the clipboard and pumps Win32 messages;
            // doing that on the Slint/winit event-loop thread re-enters the
            // message loop and dead-locks the whole UI.
            std::thread::spawn(move || clipboard_set_text(text));
        });
    }

    // Middle-click / Ctrl+Shift+V: paste clipboard text into PTY.
    {
        let handles = handles.clone();
        window.on_paste_from_clipboard(move |tab_id: SharedString| {
            // Clone the (Send) command sender for this tab so the clipboard read
            // can run off the UI thread.  Reading arboard on the event-loop
            // thread is what froze the app on middle-click / paste — see the
            // copy handler above for the deadlock explanation.
            let sender = handles
                .borrow()
                .get(tab_id.as_str())
                .map(|h| h.commands.clone());
            let Some(sender) = sender else { return };
            std::thread::spawn(move || {
                match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    Ok(text) => {
                        // Normalise line endings to a single CR so multi-line and
                        // backslash-continued commands paste correctly (see the
                        // function doc for the failure mode this prevents).
                        let bytes = normalize_pasted_newlines(&text).into_bytes();
                        let _ = sender.send(SessionCommand::RawInput(bytes));
                    }
                    Err(e) => tracing::warn!("paste_from_clipboard: clipboard error: {}", e),
                }
            });
        });
    }

    // Context menu → 清空缓存: reset the local vt100 buffer (drops scrollback),
    // wipe the displayed screen, then nudge the remote to redraw a fresh prompt.
    {
        let bufs_clear = bufs.clone();
        let handles_clear = handles.clone();
        let weak = window.as_weak();
        window.on_clear_terminal(move |tab_id: SharedString| {
            let tid = tab_id.to_string();
            if let Some(buf) = bufs_clear.lock().get_mut(&tid) {
                let (rows, cols) = buf.parser.screen().size();
                buf.parser = vt100::Parser::new(rows, cols, 5000);
                buf.find_query.clear();
                buf.history = Vec::new(); // recycle the session scrollback
                buf.prev = Vec::new();
                buf.view_offset = 0;
                buf.sel_anchor = None;
                buf.sel_focus = None;
                buf.displayed_text = Vec::new();
                buf.raw.clear();
            }
            if let Some(win) = weak.upgrade() {
                set_terminal_row(&win, &tid, |row| {
                    row.spans =
                        ModelRc::from(Rc::new(VecModel::<TermSpan>::default()));
                    row.find_matches =
                        ModelRc::from(Rc::new(VecModel::<TermMatch>::default()));
                    row.selection =
                        ModelRc::from(Rc::new(VecModel::<TermMatch>::default()));
                    row.cursor_row = 0;
                    row.cursor_col = 0;
                    row.rows_used = 0;
                    row.scroll_max = 0;
                    row.scroll_offset = 0;
                });
            }
            if let Some(h) = handles_clear.borrow().get(&tid) {
                h.send_raw(vec![0x0c]); // Ctrl+L → shell clears + redraws prompt
            }
        });
    }

    // Context menu → 查找: store the query and recompute highlight rectangles.
    {
        let bufs_find = bufs.clone();
        let weak = window.as_weak();
        window.on_find_query_changed(move |tab_id: SharedString, query: SharedString| {
            let tid = tab_id.to_string();
            let q = query.to_string();
            let matches = {
                let mut map = bufs_find.lock();
                if let Some(buf) = map.get_mut(&tid) {
                    buf.find_query = q.clone();
                    compute_find_matches(&buf.displayed_text, &q)
                } else {
                    Vec::new()
                }
            };
            if let Some(win) = weak.upgrade() {
                let model = ModelRc::from(Rc::new(VecModel::from(matches)));
                set_terminal_row(&win, &tid, |row| {
                    row.find_matches = model.clone();
                });
            }
        });
    }

    // Mouse-wheel → scroll the scrollback history.
    {
        let bufs_scroll = bufs.clone();
        let weak = window.as_weak();
        window.on_terminal_scroll(move |tab_id: SharedString, delta: i32| {
            let tid = tab_id.to_string();
            {
                let mut map = bufs_scroll.lock();
                let Some(buf) = map.get_mut(&tid) else { return };
                // Scroll within our own session scrollback (history lines above
                // the live screen).  Offset 0 = live bottom.
                let max_off = buf.history.len() as i64;
                let cur = buf.view_offset as i64;
                buf.view_offset = (cur + delta as i64).clamp(0, max_off) as usize;
            }
            if let Some(win) = weak.upgrade() {
                rebuild_tab_display(&win, &bufs_scroll, &tid);
            }
        });
    }

    // Wheel inside an alt-screen program (tmux / less / vim): forward it to the PTY
    // so the program scrolls, instead of doing nothing (#170 — FinalShell /
    // MobaXterm behave this way). If the app is tracking the mouse (e.g. tmux with
    // `mouse on`), send a real wheel mouse-event in the encoding it asked for;
    // otherwise fall back to arrow keys (xterm "alternate scroll"), which scrolls
    // less / man / vim.
    {
        let bufs_wheel = bufs.clone();
        let handles_wheel = handles.clone();
        window.on_terminal_wheel(move |tab_id: SharedString, dir: i32, col: i32, row: i32| {
            let tid = tab_id.to_string();
            let bytes = {
                let map = bufs_wheel.lock();
                let Some(buf) = map.get(&tid) else { return };
                let screen = buf.parser.screen();
                if screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None {
                    // 1-based cell under the cursor, clamped to the screen.
                    let (rows, cols) = screen.size();
                    let c = (col.clamp(0, cols.saturating_sub(1) as i32) as u16) + 1;
                    let r = (row.clamp(0, rows.saturating_sub(1) as i32) as u16) + 1;
                    let btn: u16 = if dir > 0 { 64 } else { 65 }; // wheel up / down
                    if screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr {
                        format!("\x1b[<{btn};{c};{r}M").into_bytes()
                    } else {
                        // Legacy X10 encoding: ESC [ M  Cb Cx Cy  (each value + 32).
                        let cb = (btn + 32) as u8;
                        let cx = (c.min(223) + 32) as u8;
                        let cy = (r.min(223) + 32) as u8;
                        vec![0x1b, b'[', b'M', cb, cx, cy]
                    }
                } else {
                    // alternate-scroll: 3 arrow presses per notch, app-cursor aware.
                    let one: &[u8] = if dir > 0 {
                        if screen.application_cursor() { b"\x1bOA" } else { b"\x1b[A" }
                    } else if screen.application_cursor() {
                        b"\x1bOB"
                    } else {
                        b"\x1b[B"
                    };
                    one.repeat(3)
                }
            };
            if let Some(h) = handles_wheel.borrow().get(&tid) {
                h.send_raw(bytes);
            }
        });
    }

    // Scrollbar drag → jump to an absolute scrollback offset (#103).
    {
        let bufs_scroll = bufs.clone();
        let weak = window.as_weak();
        window.on_terminal_scroll_to(move |tab_id: SharedString, offset: i32| {
            let tid = tab_id.to_string();
            {
                let mut map = bufs_scroll.lock();
                let Some(buf) = map.get_mut(&tid) else { return };
                let max_off = buf.history.len() as i64;
                buf.view_offset = (offset as i64).clamp(0, max_off) as usize;
            }
            if let Some(win) = weak.upgrade() {
                rebuild_tab_display(&win, &bufs_scroll, &tid);
            }
        });
    }

    // Drag-selection lifecycle.
    {
        let bufs_sel = bufs.clone();
        let weak = window.as_weak();
        window.on_term_select_start(move |tab_id: SharedString, row: i32, col: i32| {
            let tid = tab_id.to_string();
            {
                let mut map = bufs_sel.lock();
                let Some(buf) = map.get_mut(&tid) else { return };
                let (rows, cols) = buf.parser.screen().size();
                let r = row.clamp(0, rows.saturating_sub(1) as i32) as u16;
                let c = col.clamp(0, cols.saturating_sub(1) as i32) as u16;
                // Anchor + focus in absolute scrollback coordinates.
                let abs = buf.vis_to_abs(r);
                buf.sel_anchor = Some((abs, c));
                buf.sel_focus = Some((abs, c));
            }
            if let Some(win) = weak.upgrade() {
                rebuild_tab_display(&win, &bufs_sel, &tid);
            }
        });
    }
    {
        let bufs_sel = bufs.clone();
        let weak = window.as_weak();
        window.on_term_select_update(move |tab_id: SharedString, row: i32, col: i32| {
            let tid = tab_id.to_string();
            {
                let mut map = bufs_sel.lock();
                let Some(buf) = map.get_mut(&tid) else { return };
                let (rows, cols) = buf.parser.screen().size();
                let r = row.clamp(0, rows.saturating_sub(1) as i32) as u16;
                let c = col.clamp(0, cols.saturating_sub(1) as i32) as u16;
                if buf.sel_anchor.is_some() {
                    let abs = buf.vis_to_abs(r);
                    buf.sel_focus = Some((abs, c));
                }
            }
            if let Some(win) = weak.upgrade() {
                rebuild_tab_display(&win, &bufs_sel, &tid);
            }
        });
    }
    {
        let bufs_sel = bufs.clone();
        let weak = window.as_weak();
        window.on_term_select_end(move |tab_id: SharedString| {
            let tid = tab_id.to_string();
            // Extract the selected text; a zero-area selection (a plain click)
            // is cleared instead of copied.
            let text = {
                let mut map = bufs_sel.lock();
                let Some(buf) = map.get_mut(&tid) else { return };
                let extracted = buf.extract_selection_text();
                if extracted.is_empty() {
                    // Zero-area selection (a plain click) → clear it.
                    buf.sel_anchor = None;
                    buf.sel_focus = None;
                    None
                } else {
                    Some(extracted)
                }
            };
            match text {
                Some(t) if !t.is_empty() => {
                    // Auto-copy on release (select-to-copy, PuTTY style).
                    std::thread::spawn(move || clipboard_set_text(t));
                }
                _ => {}
            }
            if let Some(win) = weak.upgrade() {
                rebuild_tab_display(&win, &bufs_sel, &tid);
            }
        });
    }
    // Auto-scroll while drag-selecting past the visible top/bottom edge.  The
    // anchor is in absolute coordinates so it stays pinned no matter how far the
    // view moves; we only advance the scrollback view and re-point the focus at
    // the absolute row now sitting on the edge the mouse is parked against.
    {
        let bufs_sel = bufs.clone();
        let weak = window.as_weak();
        window.on_term_select_autoscroll(move |tab_id: SharedString, dir: i32| {
            let tid = tab_id.to_string();
            {
                let mut map = bufs_sel.lock();
                let Some(buf) = map.get_mut(&tid) else { return };
                // No scrollback on the alternate screen (vim/btop own the view).
                if buf.parser.screen().alternate_screen() {
                    return;
                }
                if buf.sel_anchor.is_none() {
                    return;
                }
                let rows = buf.parser.screen().size().0;
                let last = rows.saturating_sub(1);
                let max_off = buf.history.len();
                let step = 2usize;
                // Keep the focus column the user last dragged to.
                let focus_col = buf.sel_focus.map(|f| f.1).unwrap_or(0);
                let edge_vis = if dir < 0 {
                    // Mouse above the top → reveal older lines.
                    let new_off = (buf.view_offset + step).min(max_off);
                    if new_off == buf.view_offset {
                        return; // already at the oldest line
                    }
                    buf.view_offset = new_off;
                    0u16
                } else if dir > 0 {
                    // Mouse below the bottom → move toward the live tail.
                    let new_off = buf.view_offset.saturating_sub(step);
                    if new_off == buf.view_offset {
                        return; // already at the live bottom
                    }
                    buf.view_offset = new_off;
                    last
                } else {
                    return;
                };
                let abs = buf.vis_to_abs(edge_vis);
                buf.sel_focus = Some((abs, focus_col));
            }
            if let Some(win) = weak.upgrade() {
                rebuild_tab_display(&win, &bufs_sel, &tid);
            }
        });
    }
}


#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn bare_alt_is_not_forwarded() {
        // Slint sends Alt-alone as key=0x12 with alt=true. It must produce no
        // bytes — otherwise it becomes ESC+0x12 and clears the input (issue #43).
        assert_eq!(key_to_pty_bytes("\u{0012}", false, true, false), Vec::<u8>::new());
    }

    #[test]
    fn bare_modifier_codes_are_dropped() {
        // Shift..MetaR (0x10..=0x18) pressed alone (ctrl=false) → nothing sent.
        for cp in 0x10u32..=0x18 {
            let s = char::from_u32(cp).unwrap().to_string();
            assert_eq!(
                key_to_pty_bytes(&s, false, false, false),
                Vec::<u8>::new(),
                "code point {:#04x} should be dropped",
                cp
            );
        }
    }

    #[test]
    fn ctrl_letter_c0_still_passes() {
        // A real Ctrl+R encoded as the C0 byte 0x12 with ctrl=true must still be
        // forwarded — the !ctrl guard keeps the #43 fix from breaking it.
        assert_eq!(key_to_pty_bytes("\u{0012}", true, false, false), vec![0x12]);
        // Ctrl+X as C0 0x18.
        assert_eq!(key_to_pty_bytes("\u{0018}", true, false, false), vec![0x18]);
    }

    #[test]
    fn alt_letter_still_sends_esc_prefix() {
        // Alt+a (a real Meta combo) must still send ESC + 'a'.
        assert_eq!(key_to_pty_bytes("a", false, true, false), vec![0x1b, b'a']);
    }

    #[test]
    fn split_proxy_recognises_schemes() {
        assert_eq!(split_proxy(""), ("none".into(), "".into()));
        assert_eq!(
            split_proxy("http://10.0.0.1:1022"),
            ("http".into(), "10.0.0.1:1022".into())
        );
        assert_eq!(
            split_proxy("socks5://127.0.0.1:1080"),
            ("socks5".into(), "127.0.0.1:1080".into())
        );
        // user:pass survive in the host:port part.
        assert_eq!(
            split_proxy("http://u:p@host:8080"),
            ("http".into(), "u:p@host:8080".into())
        );
        // bare host:port (legacy) → treated as socks5.
        assert_eq!(
            split_proxy("127.0.0.1:1080"),
            ("socks5".into(), "127.0.0.1:1080".into())
        );
    }

    #[test]
    fn paste_normalizes_newlines_to_cr() {
        // CRLF (Windows clipboard) and LF both collapse to a single CR so a
        // backslash-continued multi-line command pastes intact.
        assert_eq!(
            normalize_pasted_newlines("sudo apt install \\\r\n  docker-ce"),
            "sudo apt install \\\r  docker-ce"
        );
        assert_eq!(normalize_pasted_newlines("a\nb\nc"), "a\rb\rc");
        // A lone CR is left as-is; no doubling.
        assert_eq!(normalize_pasted_newlines("a\rb"), "a\rb");
        // No newlines → unchanged.
        assert_eq!(normalize_pasted_newlines("echo hi"), "echo hi");
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn hist_line(s: &str) -> Line {
        (s.to_string(), Vec::new())
    }

    /// A TermBuffer whose live screen (rows×cols) shows `live_lines`, with the
    /// given `history` above it, viewed at `view_offset` (0 = live bottom).
    fn make_buf(
        rows: u16,
        cols: u16,
        history: &[&str],
        live_lines: &[&str],
        view_offset: usize,
    ) -> TermBuffer {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(live_lines.join("\r\n").as_bytes());
        TermBuffer {
            parser,
            find_query: String::new(),
            is_dark: false,
            sel_anchor: None,
            sel_focus: None,
            history: history.iter().map(|s| hist_line(s)).collect(),
            prev: Vec::new(),
            view_offset,
            displayed_text: Vec::new(),
            csi_state: CsiState::Normal,
            raw: std::collections::VecDeque::new(),
            cached_spans: Vec::new(),
            cached_displayed: Vec::new(),
        }
    }

    #[test]
    fn vis_to_abs_maps_live_and_scrolled_consistently() {
        // history H0..H2 (3 lines), live LIVE0/LIVE1 → combined len 5.
        let live = make_buf(5, 20, &["H0", "H1", "H2"], &["LIVE0", "LIVE1"], 0);
        assert_eq!(live.vis_to_abs(0), 3, "live row 0 is first live line");
        assert_eq!(live.vis_to_abs(1), 4);

        // Scrolled to the very top (offset = history len).
        let top = make_buf(5, 20, &["H0", "H1", "H2"], &["LIVE0", "LIVE1"], 3);
        assert_eq!(top.vis_to_abs(0), 0, "top row 0 is oldest history line");
        assert_eq!(top.vis_to_abs(2), 2);
        assert_eq!(top.vis_to_abs(3), 3, "row 3 crosses into live content");
    }

    #[test]
    fn extract_spans_history_and_live() {
        let mut buf = make_buf(5, 20, &["HIST0", "HIST1", "HIST2"], &["LIVE0", "LIVE1"], 3);
        buf.sel_anchor = Some((0, 0)); // top of history
        buf.sel_focus = Some((4, 19)); // end of last live line
        assert_eq!(
            buf.extract_selection_text(),
            "HIST0\nHIST1\nHIST2\nLIVE0\nLIVE1"
        );
    }

    #[test]
    fn extract_is_view_independent() {
        // The same absolute selection copies identically whether the view is
        // scrolled to the top or sitting at the live bottom — this is the whole
        // point of the fix (a top-to-bottom selection survives auto-scrolling).
        let sel = |off| {
            let mut b = make_buf(5, 20, &["HIST0", "HIST1", "HIST2"], &["LIVE0", "LIVE1"], off);
            b.sel_anchor = Some((0, 0));
            b.sel_focus = Some((4, 19));
            b.extract_selection_text()
        };
        assert_eq!(sel(3), sel(0));
        assert_eq!(sel(3), "HIST0\nHIST1\nHIST2\nLIVE0\nLIVE1");
    }

    #[test]
    fn highlight_clipped_to_current_view() {
        // Scrolled to the top: a history selection is on-screen and highlighted.
        let mut top = make_buf(5, 20, &["HIST0", "HIST1", "HIST2"], &["LIVE0", "LIVE1"], 3);
        top.sel_anchor = Some((0, 2));
        top.sel_focus = Some((2, 4));
        let rects = top.selection_rects_visible(20);
        assert_eq!(rects.len(), 3, "rows 0,1,2 (the 3 history lines) highlighted");
        assert_eq!(rects[0].row, 0);
        assert_eq!(rects[2].row, 2);

        // At the live bottom the same history selection is scrolled off → none.
        let mut live = make_buf(5, 20, &["HIST0", "HIST1", "HIST2"], &["LIVE0", "LIVE1"], 0);
        live.sel_anchor = Some((0, 2));
        live.sel_focus = Some((2, 4));
        assert!(live.selection_rects_visible(20).is_empty());
    }

    #[test]
    fn extract_handles_wide_cjk_columns() {
        // Regression for #132: copying after CJK glyphs drifted right by the
        // number of wide chars before the selection (e.g. selecting "1pctl"
        // yielded "ctl…"). The history line lays out on the grid as:
        //   提(0-1) 示(2-3) :(4) space(5) 1(6) p(7) c(8) t(9) l(10)
        let mut buf = make_buf(5, 20, &["提示: 1pctl"], &["x"], 0);

        // The "1pctl" run sits at grid cols 6..=10.
        buf.sel_anchor = Some((0, 6));
        buf.sel_focus = Some((0, 10));
        assert_eq!(buf.extract_selection_text(), "1pctl");

        // Selecting from the second CJK glyph through the end.
        buf.sel_anchor = Some((0, 2));
        buf.sel_focus = Some((0, 10));
        assert_eq!(buf.extract_selection_text(), "示: 1pctl");

        // Anchoring on the *second* cell of a wide glyph still grabs the whole
        // glyph — you can't half-select a CJK char.
        buf.sel_anchor = Some((0, 3));
        buf.sel_focus = Some((0, 10));
        assert_eq!(buf.extract_selection_text(), "示: 1pctl");
    }

    #[test]
    fn find_matches_report_grid_columns_past_cjk() {
        // Highlight rects must sit at the GRID column, not the char index, so
        // they line up over the text after CJK glyphs (#132).
        let rows = vec!["提示: 1pctl".to_string()];
        let m = compute_find_matches(&rows, "1pctl");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].col, 6, "grid column 6, not char index 4");
        assert_eq!(m[0].len, 5);

        // A CJK query spans two grid cells per glyph.
        let m2 = compute_find_matches(&rows, "提示");
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].col, 0);
        assert_eq!(m2[0].len, 4, "two wide glyphs span four grid cells");
    }
}
