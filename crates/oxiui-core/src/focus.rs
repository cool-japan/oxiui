//! Keyboard focus management over a [`WidgetTree`].
//!
//! [`FocusManager`] tracks which node currently holds focus and moves focus in
//! tab order (the DFS order returned by [`WidgetTree::focus_order`]). It
//! supports:
//!
//! - **Tab / Shift-Tab cycling** with wrap-around.
//! - **Programmatic** `focus(id)` / `blur()`.
//! - **Focus traps** — while a trap is active, focus is confined to the subtree
//!   rooted at the trap node (used for modal dialogs/popups so Tab cannot escape
//!   behind the modal).
//! - **Autofocus** — focus the first node flagged for autofocus on activation.
//!
//! The manager stores only a [`WidgetId`]; it borrows the tree per call, so the
//! caller is free to rebuild the tree between focus operations. After structural
//! changes call [`FocusManager::reconcile`] to drop focus if the focused node
//! disappeared.

use crate::tree::{WidgetId, WidgetTree};

/// Tracks and moves keyboard focus within a [`WidgetTree`].
#[derive(Debug, Default, Clone)]
pub struct FocusManager {
    focused: Option<WidgetId>,
    /// When set, focus is confined to the subtree rooted here.
    trap: Option<WidgetId>,
}

impl FocusManager {
    /// Create a manager with nothing focused and no active trap.
    pub fn new() -> Self {
        Self::default()
    }

    /// The currently focused node, if any.
    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    /// The active focus-trap root, if any.
    pub fn trap(&self) -> Option<WidgetId> {
        self.trap
    }

    /// The ordered list of focusable nodes that focus may currently move
    /// between, honouring any active trap (only the trap's focusable
    /// descendants — and the trap node itself if focusable — are included).
    pub fn focusable_set(&self, tree: &WidgetTree) -> Vec<WidgetId> {
        let order = tree.focus_order();
        match self.trap {
            None => order,
            Some(trap) => order
                .into_iter()
                .filter(|&id| id == trap || tree.is_descendant(id, trap))
                .collect(),
        }
    }

    /// Focus `id` if it is currently focusable (and inside the trap, if any).
    /// Returns `true` on success.
    pub fn focus(&mut self, tree: &WidgetTree, id: WidgetId) -> bool {
        if self.focusable_set(tree).contains(&id) {
            self.focused = Some(id);
            true
        } else {
            false
        }
    }

    /// Clear focus.
    pub fn blur(&mut self) {
        self.focused = None;
    }

    /// Activate a focus trap rooted at `trap`. If the current focus falls
    /// outside the trap, focus moves to the first focusable node within it.
    /// Returns the node focused after activation, if any.
    pub fn push_trap(&mut self, tree: &WidgetTree, trap: WidgetId) -> Option<WidgetId> {
        self.trap = Some(trap);
        let inside = self.focusable_set(tree);
        let still_valid = self.focused.map(|f| inside.contains(&f)).unwrap_or(false);
        if !still_valid {
            self.focused = inside.first().copied();
        }
        self.focused
    }

    /// Release the active focus trap. Focus is left where it is.
    pub fn pop_trap(&mut self) {
        self.trap = None;
    }

    /// Move focus to the next focusable node in tab order (wraps around).
    /// Returns the newly focused node, or `None` if nothing is focusable.
    pub fn focus_next(&mut self, tree: &WidgetTree) -> Option<WidgetId> {
        self.step(tree, true)
    }

    /// Move focus to the previous focusable node in tab order (wraps around).
    pub fn focus_prev(&mut self, tree: &WidgetTree) -> Option<WidgetId> {
        self.step(tree, false)
    }

    fn step(&mut self, tree: &WidgetTree, forward: bool) -> Option<WidgetId> {
        let order = self.focusable_set(tree);
        if order.is_empty() {
            self.focused = None;
            return None;
        }
        let next = match self
            .focused
            .and_then(|f| order.iter().position(|&id| id == f))
        {
            Some(idx) => {
                let n = order.len();
                if forward {
                    (idx + 1) % n
                } else {
                    (idx + n - 1) % n
                }
            }
            // Nothing currently focused (or focus not in set): land on the first
            // node going forward, the last going backward.
            None => {
                if forward {
                    0
                } else {
                    order.len() - 1
                }
            }
        };
        self.focused = order.get(next).copied();
        self.focused
    }

    /// Focus the first node in tab order whose `autofocus` flag (as supplied by
    /// `is_autofocus`) is `true`. Returns the focused node, if one matched.
    ///
    /// The tree node has no dedicated `autofocus` field, so the predicate lets
    /// callers decide (e.g. by inspecting a node's `label` or an external map).
    pub fn autofocus(
        &mut self,
        tree: &WidgetTree,
        is_autofocus: impl Fn(WidgetId) -> bool,
    ) -> Option<WidgetId> {
        let target = self
            .focusable_set(tree)
            .into_iter()
            .find(|&id| is_autofocus(id));
        if let Some(id) = target {
            self.focused = Some(id);
        }
        self.focused
    }

    /// Drop focus if the focused node is no longer present or no longer
    /// focusable (call after removing/reparenting nodes). Returns `true` if
    /// focus was cleared as a result.
    pub fn reconcile(&mut self, tree: &WidgetTree) -> bool {
        if let Some(f) = self.focused {
            if !self.focusable_set(tree).contains(&f) {
                self.focused = None;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    /// root → {a, b, modal → {m1, m2}}; everything focusable except root.
    fn focus_tree() -> (WidgetTree, [WidgetId; 5]) {
        let mut t = WidgetTree::new(Rect::ZERO);
        let a = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        let b = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        let modal = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        let m1 = t.insert(modal, Rect::ZERO).expect("modal");
        let m2 = t.insert(modal, Rect::ZERO).expect("modal");
        for id in [a, b, m1, m2] {
            if let Some(n) = t.get_mut(id) {
                n.focusable = true;
            }
        }
        // modal container itself is not focusable (only its contents are).
        (t, [a, b, modal, m1, m2])
    }

    #[test]
    fn tab_cycles_with_wraparound() {
        let (tree, [a, b, _modal, m1, m2]) = focus_tree();
        let mut fm = FocusManager::new();
        // Forward from nothing -> first (a).
        assert_eq!(fm.focus_next(&tree), Some(a));
        assert_eq!(fm.focus_next(&tree), Some(b));
        assert_eq!(fm.focus_next(&tree), Some(m1));
        assert_eq!(fm.focus_next(&tree), Some(m2));
        // Wrap back to a.
        assert_eq!(fm.focus_next(&tree), Some(a));
    }

    #[test]
    fn shift_tab_goes_backward_and_wraps() {
        let (tree, [a, _b, _modal, _m1, m2]) = focus_tree();
        let mut fm = FocusManager::new();
        // Backward from nothing -> last (m2).
        assert_eq!(fm.focus_prev(&tree), Some(m2));
        fm.focus(&tree, a);
        // Backward from a wraps to m2.
        assert_eq!(fm.focus_prev(&tree), Some(m2));
    }

    #[test]
    fn focus_trap_confines_tabbing() {
        let (tree, [a, _b, modal, m1, m2]) = focus_tree();
        let mut fm = FocusManager::new();
        fm.focus(&tree, a);
        // Activate the modal trap. Focus moves into the modal (a is outside).
        let landed = fm.push_trap(&tree, modal);
        assert_eq!(landed, Some(m1));
        // Tabbing now only cycles m1 <-> m2.
        assert_eq!(fm.focus_next(&tree), Some(m2));
        assert_eq!(fm.focus_next(&tree), Some(m1)); // wraps within trap
                                                    // Cannot focus an outside node while trapped.
        assert!(!fm.focus(&tree, a));
        // Release the trap; outside nodes are reachable again.
        fm.pop_trap();
        assert!(fm.focus(&tree, a));
    }

    #[test]
    fn autofocus_picks_first_match() {
        let (tree, [_a, b, _modal, _m1, _m2]) = focus_tree();
        let mut fm = FocusManager::new();
        let focused = fm.autofocus(&tree, |id| id == b);
        assert_eq!(focused, Some(b));
    }

    #[test]
    fn reconcile_drops_removed_focus() {
        let (mut tree, [a, _b, _modal, _m1, _m2]) = focus_tree();
        let mut fm = FocusManager::new();
        fm.focus(&tree, a);
        assert_eq!(fm.focused(), Some(a));
        tree.remove(a);
        assert!(fm.reconcile(&tree));
        assert_eq!(fm.focused(), None);
    }

    #[test]
    fn blur_clears_focus() {
        let (tree, [a, ..]) = focus_tree();
        let mut fm = FocusManager::new();
        fm.focus(&tree, a);
        fm.blur();
        assert_eq!(fm.focused(), None);
    }
}
