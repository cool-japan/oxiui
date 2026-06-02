//! Tab-order computation for OxiUI a11y trees.
//!
//! Computes the keyboard navigation order (Tab / Shift-Tab focus sequence) for
//! an [`crate::tree::A11yNode`] subtree.  Nodes with an explicit
//! `tab_index > 0` are placed before all naturally-ordered nodes; among those,
//! they are sorted by `tab_index` ascending.  Nodes with `tab_index == 0` (or
//! `None`, treated as 0) follow in document order.  Disabled nodes are excluded.

use accesskit::NodeId;

use crate::tree::{A11yNode, WidgetRole};

// ── Focusable role predicate ──────────────────────────────────────────────────

/// Return `true` for widget roles that are keyboard-focusable by default.
fn is_focusable_role(role: WidgetRole) -> bool {
    matches!(
        role,
        WidgetRole::Button
            | WidgetRole::TextInput
            | WidgetRole::Checkbox
            | WidgetRole::Slider
            | WidgetRole::Tab
            | WidgetRole::Link
            | WidgetRole::MenuItem
    )
}

// ── Tab-order walk ────────────────────────────────────────────────────────────

/// Computed tab-order for an [`A11yNode`] subtree.
///
/// Nodes are ordered according to the ARIA/HTML tab-order algorithm:
/// nodes with an explicit `tab_index > 0` appear first, sorted ascending;
/// focusable nodes with `tab_index == None` or `0` follow in document order.
/// Disabled nodes are never included.
pub struct TabOrder {
    /// Focusable node IDs in tab order.
    pub order: Vec<NodeId>,
}

impl TabOrder {
    /// Compute the tab order from the OxiUI node tree rooted at `root`.
    ///
    /// # Algorithm
    ///
    /// 1. Walk the subtree depth-first, collecting every focusable,
    ///    non-disabled node.
    /// 2. Partition into *explicit* (tab_index > 0) and *natural* (tab_index
    ///    == None or 0) buckets.
    /// 3. Sort the explicit bucket by tab_index ascending (stable, preserving
    ///    document order within equal tab_index values).
    /// 4. Concatenate: explicit first, then natural.
    pub fn compute(root: &A11yNode) -> Self {
        let mut explicit: Vec<(u32, NodeId)> = Vec::new();
        let mut natural: Vec<NodeId> = Vec::new();

        collect_focusable(root, &mut explicit, &mut natural);

        // Stable sort preserves document order for equal tab_index values.
        explicit.sort_by_key(|(idx, _)| *idx);

        let mut order: Vec<NodeId> = explicit.into_iter().map(|(_, id)| id).collect();
        order.extend(natural);

        Self { order }
    }

    /// Return the `NodeId` that should receive focus after `current`.
    ///
    /// Wraps around: calling `next_focus` when `current` is the last node
    /// returns the first node in the order.  Returns `None` if the order is
    /// empty.
    pub fn next_focus(&self, current: Option<NodeId>) -> Option<NodeId> {
        if self.order.is_empty() {
            return None;
        }
        match current {
            None => self.order.first().copied(),
            Some(id) => {
                let pos = self.order.iter().position(|&n| n == id);
                match pos {
                    None => self.order.first().copied(),
                    Some(i) => {
                        let next = (i + 1) % self.order.len();
                        self.order.get(next).copied()
                    }
                }
            }
        }
    }

    /// Return the `NodeId` that should receive focus before `current`.
    ///
    /// Wraps around: calling `prev_focus` when `current` is the first node
    /// returns the last node in the order.  Returns `None` if the order is
    /// empty.
    pub fn prev_focus(&self, current: Option<NodeId>) -> Option<NodeId> {
        if self.order.is_empty() {
            return None;
        }
        match current {
            None => self.order.last().copied(),
            Some(id) => {
                let pos = self.order.iter().position(|&n| n == id);
                match pos {
                    None => self.order.last().copied(),
                    Some(0) => self.order.last().copied(),
                    Some(i) => self.order.get(i - 1).copied(),
                }
            }
        }
    }
}

// ── Free navigation helpers ───────────────────────────────────────────────────

/// Advance focus to the next node in `tab_order`, wrapping around at the end.
///
/// * `current = None`  → returns the **first** focusable node.
/// * `current = Some(last)` → wraps around and returns the **first** node.
/// * `current` not found in the order → returns the first node.
/// * Empty tab order → returns `None`.
///
/// Delegates to [`TabOrder::next_focus`].
pub fn tab_next(tab_order: &TabOrder, current: Option<NodeId>) -> Option<NodeId> {
    tab_order.next_focus(current)
}

/// Move focus to the previous node in `tab_order`, wrapping around at the start.
///
/// * `current = None`  → returns the **last** focusable node.
/// * `current = Some(first)` → wraps around and returns the **last** node.
/// * `current` not found in the order → returns the last node.
/// * Empty tab order → returns `None`.
///
/// Delegates to [`TabOrder::prev_focus`].
pub fn tab_prev(tab_order: &TabOrder, current: Option<NodeId>) -> Option<NodeId> {
    tab_order.prev_focus(current)
}

// ── Internal walk ─────────────────────────────────────────────────────────────

/// Recursively collect focusable, non-disabled nodes into `explicit` and
/// `natural` buckets.
fn collect_focusable(
    node: &A11yNode,
    explicit: &mut Vec<(u32, NodeId)>,
    natural: &mut Vec<NodeId>,
) {
    let focusable = is_focusable_role(node.role) && !node.props.disabled;
    if focusable {
        match node.props.tab_index {
            Some(idx) if idx > 0 => explicit.push((idx, node.id)),
            _ => natural.push(node.id),
        }
    }
    for child in &node.children {
        collect_focusable(child, explicit, natural);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{A11yNode, WidgetRole};
    use accesskit::NodeId;

    fn nid(n: u64) -> NodeId {
        NodeId(n)
    }

    fn btn(id: u64) -> A11yNode {
        A11yNode::simple(nid(id), WidgetRole::Button, Some(format!("Btn{id}")))
    }

    fn btn_with_tab_index(id: u64, idx: u32) -> A11yNode {
        let mut n = btn(id);
        n.props.tab_index = Some(idx);
        n
    }

    #[test]
    fn test_tab_order_natural() {
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        root.children.push(btn(1));
        root.children.push(btn(2));
        root.children.push(btn(3));

        let order = TabOrder::compute(&root);
        assert_eq!(order.order, vec![nid(1), nid(2), nid(3)]);
    }

    #[test]
    fn test_tab_order_explicit_tab_index() {
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        // btn(3) has tab_index=1 → must come first
        root.children.push(btn(1));
        root.children.push(btn(2));
        root.children.push(btn_with_tab_index(3, 1));

        let order = TabOrder::compute(&root);
        assert_eq!(order.order[0], nid(3), "tab_index=1 node must be first");
        // btn(1) and btn(2) follow in document order
        assert_eq!(order.order[1], nid(1));
        assert_eq!(order.order[2], nid(2));
    }

    #[test]
    fn test_next_focus_wraps() {
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        root.children.push(btn(1));
        root.children.push(btn(2));
        root.children.push(btn(3));

        let order = TabOrder::compute(&root);
        // next from last → first
        assert_eq!(order.next_focus(Some(nid(3))), Some(nid(1)));
    }

    #[test]
    fn test_prev_focus_wraps() {
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        root.children.push(btn(1));
        root.children.push(btn(2));
        root.children.push(btn(3));

        let order = TabOrder::compute(&root);
        // prev from first → last
        assert_eq!(order.prev_focus(Some(nid(1))), Some(nid(3)));
    }

    #[test]
    fn test_tab_order_disabled_excluded() {
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        let mut disabled_btn = btn(1);
        disabled_btn.props.disabled = true;
        root.children.push(disabled_btn);
        root.children.push(btn(2));

        let order = TabOrder::compute(&root);
        assert_eq!(order.order, vec![nid(2)]);
    }

    #[test]
    fn test_tab_order_non_focusable_excluded() {
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        // Labels are not focusable
        root.children
            .push(A11yNode::simple(nid(1), WidgetRole::Label, None));
        root.children.push(btn(2));

        let order = TabOrder::compute(&root);
        assert_eq!(order.order, vec![nid(2)]);
    }
}
