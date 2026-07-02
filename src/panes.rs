//! Split-pane layout tree (v0.5, IDEA-style nested splits).
//!
//! Slint can't render recursive components, so the nestable split layout lives
//! here as a tree. We walk it to assign every leaf pane (and every splitter) an
//! absolute rectangle in content-area coordinates, then flatten the result into
//! plain lists the UI renders directly with `for`. Any structural change (split,
//! close, move a tab, drag a splitter) mutates the tree and re-flattens.

/// Split orientation. `Horizontal` places the two children side by side
/// (`first` on the left), `Vertical` stacks them (`first` on top).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Horizontal,
    Vertical,
}

/// A node: either a binary split or a leaf pane holding a tab group.
#[derive(Clone, Debug)]
pub enum Node {
    Split {
        id: u64,
        dir: Dir,
        /// Fraction of the long axis given to `first` (0..1).
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
    Leaf(Leaf),
}

/// A leaf pane: its tab group (ids, in order) and which tab is active.
#[derive(Clone, Debug)]
pub struct Leaf {
    pub id: u64,
    pub tabs: Vec<String>,
    pub active: String,
}

/// The whole layout plus an id allocator and which leaf currently has focus.
#[derive(Debug)]
pub struct Layout {
    pub root: Node,
    pub focused: u64,
    next_id: u64,
}

/// A leaf pane flattened to an absolute rect (content-area coordinates).
#[derive(Clone, Debug, PartialEq)]
pub struct PaneRect {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub tabs: Vec<String>,
    pub active: String,
    pub focused: bool,
}

/// A draggable splitter between the two children of a `Split` node.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitterRect {
    /// Id of the `Split` node this resizes.
    pub split_id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// True when the handle is vertical (a Horizontal split → drag left/right).
    pub vertical: bool,
    /// Start of the split's axis (x for a Horizontal split, y for a Vertical
    /// one) and its length — the `[start, start+len]` window `set_ratio` maps a
    /// drag position into. Lets the drag handler recover the ratio without
    /// tracking the parent rect separately.
    pub axis_start: f32,
    pub axis_len: f32,
}

/// Visible thickness of a splitter handle, in px.
pub const SPLITTER: f32 = 6.0;
/// Smallest a pane is allowed to get along the split axis when dragging, in px.
const MIN_PANE: f32 = 80.0;

impl Layout {
    /// A fresh single-pane layout owning `tabs` with `active` selected.
    pub fn new(tabs: Vec<String>, active: String) -> Self {
        let root = Node::Leaf(Leaf { id: 1, tabs, active });
        Layout { root, focused: 1, next_id: 2 }
    }

    fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Flatten the tree into pane + splitter rects for the given content area.
    pub fn flatten(&self, x: f32, y: f32, w: f32, h: f32) -> (Vec<PaneRect>, Vec<SplitterRect>) {
        let mut panes = Vec::new();
        let mut splits = Vec::new();
        layout_node(&self.root, x, y, w, h, self.focused, &mut panes, &mut splits);
        (panes, splits)
    }

    /// Find a leaf by id (immutable / mutable).
    pub fn leaf(&self, id: u64) -> Option<&Leaf> {
        find_leaf(&self.root, id)
    }
    pub fn leaf_mut(&mut self, id: u64) -> Option<&mut Leaf> {
        find_leaf_mut(&mut self.root, id)
    }

    /// The leaf that currently owns tab `tab_id`, if any.
    pub fn leaf_of_tab(&self, tab_id: &str) -> Option<u64> {
        let mut found = None;
        for_each_leaf(&self.root, &mut |l| {
            if found.is_none() && l.tabs.iter().any(|t| t == tab_id) {
                found = Some(l.id);
            }
        });
        found
    }

    /// Split leaf `leaf_id` along `dir`, moving `tab_id` into the new pane on the
    /// `before`/after side. Returns the new leaf's id. The tab is removed from its
    /// old pane; if that empties the pane it is collapsed afterwards by the caller
    /// via [`Self::prune`]. Focus moves to the new pane.
    pub fn split(&mut self, leaf_id: u64, dir: Dir, tab_id: &str, before: bool) -> Option<u64> {
        // The moved tab must come from somewhere; detach it first.
        let from = self.leaf_of_tab(tab_id)?;
        let new_id = self.alloc();
        let split_id = self.alloc();
        // Build the replacement subtree for the target leaf.
        let target = take_leaf(&mut self.root, leaf_id)?;
        let new_leaf = Node::Leaf(Leaf {
            id: new_id,
            tabs: vec![tab_id.to_string()],
            active: tab_id.to_string(),
        });
        let (first, second) = if before {
            (new_leaf, target)
        } else {
            (target, new_leaf)
        };
        let replacement = Node::Split {
            id: split_id,
            dir,
            ratio: 0.5,
            first: Box::new(first),
            second: Box::new(second),
        };
        put_node(&mut self.root, leaf_id, replacement);
        // Remove the tab from its original pane (unless it WAS the target's only
        // content that we just wrapped — detach handles that uniformly).
        if from != new_id {
            if let Some(src) = self.leaf_mut(from) {
                src.tabs.retain(|t| t != tab_id);
                if src.active == tab_id {
                    src.active = src.tabs.last().cloned().unwrap_or_default();
                }
            }
        }
        self.focused = new_id;
        self.prune();
        Some(new_id)
    }

    /// Move `tab_id` into existing leaf `to` (e.g. dropped onto another pane's tab
    /// strip). No-op if it's already there. Collapses an emptied source pane.
    pub fn move_tab(&mut self, tab_id: &str, to: u64) {
        let from = match self.leaf_of_tab(tab_id) {
            Some(f) => f,
            None => return,
        };
        if from == to {
            return;
        }
        if let Some(src) = self.leaf_mut(from) {
            src.tabs.retain(|t| t != tab_id);
            if src.active == tab_id {
                src.active = src.tabs.last().cloned().unwrap_or_default();
            }
        }
        if let Some(dst) = self.leaf_mut(to) {
            dst.tabs.push(tab_id.to_string());
            dst.active = tab_id.to_string();
        }
        self.focused = to;
        self.prune();
    }

    /// Remove `tab_id` from whatever pane holds it (tab closed). Collapses the pane
    /// if it becomes empty.
    pub fn remove_tab(&mut self, tab_id: &str) {
        if let Some(id) = self.leaf_of_tab(tab_id) {
            if let Some(l) = self.leaf_mut(id) {
                l.tabs.retain(|t| t != tab_id);
                if l.active == tab_id {
                    l.active = l.tabs.last().cloned().unwrap_or_default();
                }
            }
            self.prune();
        }
    }

    /// Add a brand-new tab to the focused pane (or the first pane) and select it.
    pub fn add_tab(&mut self, tab_id: String) {
        let target = if self.leaf(self.focused).is_some() {
            self.focused
        } else {
            first_leaf_id(&self.root)
        };
        if let Some(l) = self.leaf_mut(target) {
            l.tabs.push(tab_id.clone());
            l.active = tab_id;
        }
    }

    /// Adjust the ratio of split `split_id` so the boundary sits at `pos` px along
    /// the split's axis, given the split occupies `[start, start+len]`.
    pub fn set_ratio(&mut self, split_id: u64, start: f32, len: f32, pos: f32) {
        if len <= 0.0 {
            return;
        }
        let min = MIN_PANE / len;
        let r = ((pos - start) / len).clamp(min, 1.0 - min);
        set_split_ratio(&mut self.root, split_id, r);
    }

    /// Collapse any `Split` whose child became an empty leaf, replacing the split
    /// with its surviving child. Keeps the tree minimal after closes/moves.
    pub fn prune(&mut self) {
        prune_node(&mut self.root);
        // Focus may have pointed at a removed leaf; re-home it.
        if find_leaf(&self.root, self.focused).is_none() {
            self.focused = first_leaf_id(&self.root);
        }
    }
}

// --- tree walks ------------------------------------------------------------

fn layout_node(
    node: &Node,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    focused: u64,
    panes: &mut Vec<PaneRect>,
    splits: &mut Vec<SplitterRect>,
) {
    match node {
        Node::Leaf(l) => panes.push(PaneRect {
            id: l.id,
            x,
            y,
            w,
            h,
            tabs: l.tabs.clone(),
            active: l.active.clone(),
            focused: l.id == focused,
        }),
        Node::Split { id, dir, ratio, first, second } => {
            let half = SPLITTER / 2.0;
            match dir {
                Dir::Horizontal => {
                    let fw = (w - SPLITTER).max(0.0) * ratio;
                    let sw = (w - SPLITTER).max(0.0) - fw;
                    layout_node(first, x, y, fw, h, focused, panes, splits);
                    splits.push(SplitterRect {
                        split_id: *id,
                        x: x + fw,
                        y,
                        w: SPLITTER,
                        h,
                        vertical: true,
                        axis_start: x,
                        axis_len: w,
                    });
                    layout_node(second, x + fw + SPLITTER, y, sw, h, focused, panes, splits);
                    let _ = half;
                }
                Dir::Vertical => {
                    let fh = (h - SPLITTER).max(0.0) * ratio;
                    let sh = (h - SPLITTER).max(0.0) - fh;
                    layout_node(first, x, y, w, fh, focused, panes, splits);
                    splits.push(SplitterRect {
                        split_id: *id,
                        x,
                        y: y + fh,
                        w,
                        h: SPLITTER,
                        vertical: false,
                        axis_start: y,
                        axis_len: h,
                    });
                    layout_node(second, x, y + fh + SPLITTER, w, sh, focused, panes, splits);
                }
            }
        }
    }
}

fn find_leaf(node: &Node, id: u64) -> Option<&Leaf> {
    match node {
        Node::Leaf(l) if l.id == id => Some(l),
        Node::Leaf(_) => None,
        Node::Split { first, second, .. } => {
            find_leaf(first, id).or_else(|| find_leaf(second, id))
        }
    }
}

fn find_leaf_mut(node: &mut Node, id: u64) -> Option<&mut Leaf> {
    match node {
        Node::Leaf(l) if l.id == id => Some(l),
        Node::Leaf(_) => None,
        Node::Split { first, second, .. } => {
            // Try first, then second (can't chain `or_else` on &mut easily).
            if find_leaf_mut(first, id).is_some() {
                return find_leaf_mut(first, id);
            }
            find_leaf_mut(second, id)
        }
    }
}

fn for_each_leaf(node: &Node, f: &mut impl FnMut(&Leaf)) {
    match node {
        Node::Leaf(l) => f(l),
        Node::Split { first, second, .. } => {
            for_each_leaf(first, f);
            for_each_leaf(second, f);
        }
    }
}

fn first_leaf_id(node: &Node) -> u64 {
    match node {
        Node::Leaf(l) => l.id,
        Node::Split { first, .. } => first_leaf_id(first),
    }
}

/// Remove and return the leaf `id` as an owned `Node`, leaving a placeholder the
/// caller immediately overwrites via [`put_node`].
fn take_leaf(node: &mut Node, id: u64) -> Option<Node> {
    match node {
        Node::Leaf(l) if l.id == id => Some(Node::Leaf(l.clone())),
        Node::Leaf(_) => None,
        Node::Split { first, second, .. } => {
            take_leaf(first, id).or_else(|| take_leaf(second, id))
        }
    }
}

/// Replace the leaf `id` subtree with `replacement`.
fn put_node(node: &mut Node, id: u64, replacement: Node) -> bool {
    match node {
        Node::Leaf(l) if l.id == id => {
            *node = replacement;
            true
        }
        Node::Leaf(_) => false,
        Node::Split { first, second, .. } => {
            // Move replacement into whichever branch contains the leaf.
            if contains_leaf(first, id) {
                put_node(first, id, replacement)
            } else {
                put_node(second, id, replacement)
            }
        }
    }
}

fn contains_leaf(node: &Node, id: u64) -> bool {
    find_leaf(node, id).is_some()
}

fn set_split_ratio(node: &mut Node, split_id: u64, ratio: f32) -> bool {
    match node {
        Node::Leaf(_) => false,
        Node::Split { id, ratio: r, first, second, .. } => {
            if *id == split_id {
                *r = ratio;
                true
            } else {
                set_split_ratio(first, split_id, ratio)
                    || set_split_ratio(second, split_id, ratio)
            }
        }
    }
}

/// Collapse splits with an empty leaf child, replacing the split with the other
/// child. Recurses bottom-up.
fn prune_node(node: &mut Node) {
    if let Node::Split { first, second, .. } = node {
        prune_node(first);
        prune_node(second);
        let first_empty = matches!(first.as_ref(), Node::Leaf(l) if l.tabs.is_empty());
        let second_empty = matches!(second.as_ref(), Node::Leaf(l) if l.tabs.is_empty());
        if first_empty {
            let keep = std::mem::replace(second.as_mut(), placeholder());
            *node = keep;
        } else if second_empty {
            let keep = std::mem::replace(first.as_mut(), placeholder());
            *node = keep;
        }
    }
}

fn placeholder() -> Node {
    Node::Leaf(Leaf { id: 0, tabs: Vec::new(), active: String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(panes: &[PaneRect]) -> Vec<u64> {
        panes.iter().map(|p| p.id).collect()
    }

    #[test]
    fn single_pane_fills_area() {
        let l = Layout::new(vec!["a".into(), "b".into()], "a".into());
        let (panes, splits) = l.flatten(0.0, 0.0, 1000.0, 600.0);
        assert_eq!(panes.len(), 1);
        assert!(splits.is_empty());
        assert_eq!((panes[0].x, panes[0].y, panes[0].w, panes[0].h), (0.0, 0.0, 1000.0, 600.0));
        assert!(panes[0].focused);
    }

    #[test]
    fn horizontal_split_halves_width_and_emits_splitter() {
        let mut l = Layout::new(vec!["a".into(), "b".into()], "a".into());
        let new = l.split(1, Dir::Horizontal, "b", false).unwrap();
        let (panes, splits) = l.flatten(0.0, 0.0, 1006.0, 600.0);
        assert_eq!(panes.len(), 2);
        assert_eq!(splits.len(), 1);
        // (1006 - 6) * 0.5 = 500 each.
        assert_eq!(panes[0].w, 500.0);
        assert_eq!(panes[1].x, 506.0);
        assert!(splits[0].vertical);
        // 'b' moved out of pane 1 into the new pane; focus followed.
        assert_eq!(l.leaf(1).unwrap().tabs, vec!["a".to_string()]);
        assert_eq!(l.leaf(new).unwrap().tabs, vec!["b".to_string()]);
        assert_eq!(l.focused, new);
    }

    #[test]
    fn closing_last_tab_collapses_the_split() {
        let mut l = Layout::new(vec!["a".into(), "b".into()], "a".into());
        let new = l.split(1, Dir::Vertical, "b", false).unwrap();
        // Close 'b' → its pane empties → split collapses back to a single pane.
        l.remove_tab("b");
        let (panes, splits) = l.flatten(0.0, 0.0, 800.0, 600.0);
        assert_eq!(panes.len(), 1);
        assert!(splits.is_empty());
        assert_eq!(panes[0].tabs, vec!["a".to_string()]);
        let _ = new;
    }

    #[test]
    fn move_tab_between_panes() {
        let mut l = Layout::new(vec!["a".into(), "b".into(), "c".into()], "a".into());
        let right = l.split(1, Dir::Horizontal, "c", false).unwrap();
        // Move 'b' from the left pane into the right pane.
        l.move_tab("b", right);
        assert_eq!(l.leaf(1).unwrap().tabs, vec!["a".to_string()]);
        let mut rtabs = l.leaf(right).unwrap().tabs.clone();
        rtabs.sort();
        assert_eq!(rtabs, vec!["b".to_string(), "c".to_string()]);
        assert_eq!(ids(&l.flatten(0.0, 0.0, 800.0, 600.0).0).len(), 2);
    }

    #[test]
    fn nested_splits_partition_without_overlap() {
        let mut l = Layout::new(vec!["a".into(), "b".into(), "c".into()], "a".into());
        let r = l.split(1, Dir::Horizontal, "b", false).unwrap();
        l.split(r, Dir::Vertical, "c", false).unwrap();
        let (panes, _) = l.flatten(0.0, 0.0, 1006.0, 606.0);
        assert_eq!(panes.len(), 3);
        // Total covered area equals the content area (minus splitter gaps).
        let area: f32 = panes.iter().map(|p| p.w * p.h).sum();
        assert!(area > 0.0 && area <= 1006.0 * 606.0);
    }
}
