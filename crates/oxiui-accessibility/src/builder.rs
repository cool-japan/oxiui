//! Fluent builder for [`crate::tree::A11yNode`].
//!
//! Provides [`A11yNodeBuilder`], a chainable builder that accumulates
//! [`crate::props::A11yNodeProps`] and child lists before constructing a
//! complete [`crate::tree::A11yNode`].
//!
//! # Example
//!
//! ```rust
//! use accesskit::NodeId;
//! use oxiui_accessibility::builder::A11yNodeBuilder;
//! use oxiui_accessibility::tree::WidgetRole;
//! use oxiui_accessibility::props::CheckedState;
//!
//! let node = A11yNodeBuilder::new(NodeId(1), WidgetRole::Checkbox)
//!     .label("Accept terms")
//!     .description("Check to accept the terms and conditions")
//!     .checked(CheckedState::False)
//!     .build();
//!
//! assert_eq!(node.label, Some("Accept terms".to_string()));
//! assert!(node.props.description.is_some());
//! ```

use accesskit::NodeId;

use crate::props::{A11yNodeProps, CheckedState, TextSelection};
use crate::tree::{A11yNode, WidgetRole};

// ── Builder ──────────────────────────────────────────────────────────────────

/// Fluent builder for [`A11yNode`].
///
/// Construct via [`A11yNodeBuilder::new`], chain the desired setters, then
/// call [`A11yNodeBuilder::build`] to obtain the finished node.
pub struct A11yNodeBuilder {
    id: NodeId,
    role: WidgetRole,
    label: Option<String>,
    props: A11yNodeProps,
    children: Vec<NodeId>,
    text_content: Option<String>,
}

impl A11yNodeBuilder {
    /// Create a new builder with the required `id` and `role`.
    pub fn new(id: NodeId, role: WidgetRole) -> Self {
        Self {
            id,
            role,
            label: None,
            props: A11yNodeProps::default(),
            children: Vec::new(),
            text_content: None,
        }
    }

    // ── Label ────────────────────────────────────────────────────────────────

    /// Set the visible label / accessible name.
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }

    // ── Text / description ────────────────────────────────────────────────────

    /// Set the longer description (`aria-description` equivalent).
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.props.description = Some(s.into());
        self
    }

    /// Set placeholder text for empty text inputs.
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.props.placeholder = Some(s.into());
        self
    }

    /// Set the keyboard shortcut string (e.g. `"Ctrl+S"`).
    pub fn key_shortcut(mut self, s: impl Into<String>) -> Self {
        self.props.key_shortcut = Some(s.into());
        self
    }

    // ── State ────────────────────────────────────────────────────────────────

    /// Mark the widget as disabled (non-interactive).
    pub fn disabled(mut self) -> Self {
        self.props.disabled = true;
        self
    }

    /// Set the expanded/collapsed state (`None` = not expandable).
    pub fn expanded(mut self, v: bool) -> Self {
        self.props.expanded = Some(v);
        self
    }

    /// Set the selected state (`None` = not selectable).
    pub fn selected(mut self, v: bool) -> Self {
        self.props.selected = Some(v);
        self
    }

    /// Set the checked / toggle state.
    pub fn checked(mut self, v: CheckedState) -> Self {
        self.props.checked = Some(v);
        self
    }

    // ── Range values ─────────────────────────────────────────────────────────

    /// Set all four numeric range properties at once.
    pub fn value(mut self, now: f64, min: f64, max: f64, step: f64) -> Self {
        self.props.value_now = Some(now);
        self.props.value_min = Some(min);
        self.props.value_max = Some(max);
        self.props.value_step = Some(step);
        self
    }

    // ── Text content ─────────────────────────────────────────────────────────

    /// Set the text content / string value of the node.
    pub fn text(mut self, v: impl Into<String>) -> Self {
        self.text_content = Some(v.into());
        self
    }

    /// Set the text selection (anchor + focus byte offsets).
    pub fn text_selection(mut self, sel: TextSelection) -> Self {
        self.props.text_selection = Some(sel);
        self
    }

    // ── Relationships ─────────────────────────────────────────────────────────

    /// Set the `labelled_by` relationship (replaces any existing list).
    pub fn labelled_by(mut self, ids: impl IntoIterator<Item = NodeId>) -> Self {
        self.props.labelled_by = ids.into_iter().collect();
        self
    }

    /// Set the `described_by` relationship (replaces any existing list).
    pub fn described_by(mut self, ids: impl IntoIterator<Item = NodeId>) -> Self {
        self.props.described_by = ids.into_iter().collect();
        self
    }

    /// Set the `controlled_by` relationship (replaces any existing list).
    pub fn controlled_by(mut self, ids: impl IntoIterator<Item = NodeId>) -> Self {
        self.props.controlled_by = ids.into_iter().collect();
        self
    }

    /// Set the `owns` relationship (replaces any existing list).
    pub fn owns(mut self, ids: impl IntoIterator<Item = NodeId>) -> Self {
        self.props.owns = ids.into_iter().collect();
        self
    }

    // ── Keyboard navigation ───────────────────────────────────────────────────

    /// Set the explicit tab index for keyboard-focus order.
    ///
    /// `0` = natural document order; `n > 0` = explicit position (lower values
    /// receive focus before higher values).  Nodes with no tab index set
    /// participate in natural document order.
    pub fn tab_index(mut self, i: u32) -> Self {
        self.props.tab_index = Some(i);
        self
    }

    // ── Children ─────────────────────────────────────────────────────────────

    /// Append a child node id.
    ///
    /// Note: the builder only records child *ids*. For the full node subtree to
    /// appear in the tree update you must also pass those child [`A11yNode`]s
    /// as children of this node when assembling the tree manually, or add them
    /// to the [`crate::tree::A11yTree`] separately.
    pub fn child(mut self, id: NodeId) -> Self {
        self.children.push(id);
        self
    }

    // ── Build ─────────────────────────────────────────────────────────────────

    /// Consume the builder and produce an [`A11yNode`].
    ///
    /// The `children` field of the returned node will be empty (child subtrees
    /// must be attached separately); however the AccessKit children-id list is
    /// written into the node's properties during tree serialisation.
    ///
    /// # Note
    ///
    /// If you need the produced node to carry its own subtree of [`A11yNode`]
    /// values, use [`A11yNodeBuilder::build_with_children`].
    pub fn build(self) -> A11yNode {
        A11yNode {
            id: self.id,
            role: self.role,
            label: self.label,
            children: Vec::new(),
            props: self.props,
            text_content: self.text_content,
        }
    }

    /// Consume the builder and attach `children` as the node's subtree.
    ///
    /// The ids in `children` must match the builder's `.child(id)` calls;
    /// this method does not validate them. Prefer this variant when you have
    /// already built the child subtrees.
    pub fn build_with_children(self, children: Vec<A11yNode>) -> A11yNode {
        A11yNode {
            id: self.id,
            role: self.role,
            label: self.label,
            children,
            props: self.props,
            text_content: self.text_content,
        }
    }
}
