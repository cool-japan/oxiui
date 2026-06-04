//! Bridge between [`oxiui_core::Widget`] and the OxiUI accessibility tree.
//!
//! The `oxiui-core` `Widget` trait already exposes three a11y hooks:
//! [`Widget::a11y_role`], [`Widget::a11y_label`], and
//! [`Widget::a11y_description`].  This module maps those hooks into the
//! accessibility tree types provided by `oxiui-accessibility`.
//!
//! # Overview
//!
//! * [`widget_to_a11y_node`] — converts a single `&dyn Widget` leaf into an
//!   [`A11yNode`] (no children traversal).
//! * [`A11yWidgetNode`] — an extension trait that adds `a11y_children()` so
//!   composite widgets can expose their sub-widget graph for tree building.
//! * [`build_a11y_tree`] — walks an `A11yWidgetNode` tree depth-first,
//!   allocating `NodeId`s monotonically, and returns the root [`A11yNode`].
//!
//! # A11yRole → WidgetRole mapping
//!
//! [`oxiui_core::A11yRole`] is the lightweight core enum; [`WidgetRole`] is the
//! richer accessibility-layer enum.  [`core_role_to_widget_role`] performs the
//! one-way mapping.  Roles present in `WidgetRole` but absent from `A11yRole`
//! (e.g. landmark roles) are not reachable via this path; they must be
//! constructed directly with [`A11yNodeBuilder`] or [`A11yNode::simple`].

use accesskit::NodeId;
use oxiui_core::{A11yRole, Widget};

use crate::{
    builder::A11yNodeBuilder,
    tree::{A11yNode, WidgetRole},
};

// ── A11yRole → WidgetRole ────────────────────────────────────────────────────

/// Convert an [`oxiui_core::A11yRole`] to the corresponding [`WidgetRole`].
///
/// The mapping is many-to-one: several `A11yRole` variants that map to the
/// same ARIA concept share a `WidgetRole`.  Roles not directly representable
/// in `WidgetRole` default to [`WidgetRole::Unknown`].
///
/// # Note on non-exhaustiveness
///
/// `A11yRole` is `#[non_exhaustive]`; new variants added in future versions of
/// `oxiui-core` will be handled by the catch-all `_ => WidgetRole::Unknown`
/// arm, preserving forward compatibility without a breaking change here.
pub fn core_role_to_widget_role(role: A11yRole) -> WidgetRole {
    match role {
        A11yRole::Group => WidgetRole::Group,
        A11yRole::StaticText => WidgetRole::Label,
        A11yRole::Button => WidgetRole::Button,
        // Heading has no dedicated WidgetRole variant; map to Label (static text)
        // so screen readers still see the text content.
        A11yRole::Heading => WidgetRole::Label,
        A11yRole::TextInput => WidgetRole::TextInput,
        // TextArea is a multi-line text input — map to TextInput.
        A11yRole::TextArea => WidgetRole::TextInput,
        A11yRole::Checkbox => WidgetRole::Checkbox,
        A11yRole::Slider => WidgetRole::Slider,
        A11yRole::ProgressBar => WidgetRole::ProgressBar,
        A11yRole::TabPanel => WidgetRole::TabPanel,
        A11yRole::Tab => WidgetRole::Tab,
        // List maps to Group (closest available container).
        A11yRole::List => WidgetRole::Group,
        A11yRole::ListItem => WidgetRole::ListItem,
        // Table maps to Group (no dedicated Table container role in WidgetRole yet).
        A11yRole::Table => WidgetRole::Group,
        A11yRole::TableRow => WidgetRole::TableRow,
        A11yRole::TableCell => WidgetRole::TableCell,
        A11yRole::ColumnHeader => WidgetRole::ColumnHeader,
        A11yRole::Dialog => WidgetRole::Dialog,
        A11yRole::Image => WidgetRole::Image,
        A11yRole::Link => WidgetRole::Link,
        A11yRole::Menu => WidgetRole::Menu,
        A11yRole::MenuItem => WidgetRole::MenuItem,
        A11yRole::Alert => WidgetRole::Alert,
        A11yRole::Tooltip => WidgetRole::Tooltip,
        A11yRole::Tree => WidgetRole::Tree,
        A11yRole::TreeItem => WidgetRole::TreeItem,
        A11yRole::Unknown => WidgetRole::Unknown,
        // Forward-compatible catch-all: new A11yRole variants added in future
        // oxiui-core releases are mapped to Unknown until this crate is updated.
        _ => WidgetRole::Unknown,
    }
}

// ── Single-widget conversion ─────────────────────────────────────────────────

/// Convert a single `Widget` reference into an [`A11yNode`] leaf (no children).
///
/// Reads the widget's [`Widget::a11y_role`], [`Widget::a11y_label`], and
/// [`Widget::a11y_description`] and populates the corresponding fields of the
/// returned node.
///
/// # Parameters
///
/// * `widget` — the widget to convert.
/// * `id`     — the `NodeId` to assign to this node.  The caller is responsible
///   for uniqueness; [`build_a11y_tree`] allocates IDs automatically.
///
/// # Example
///
/// ```rust
/// use accesskit::NodeId;
/// use oxiui_core::{A11yRole, Widget};
/// use oxiui_accessibility::widget_bridge::widget_to_a11y_node;
///
/// struct OkButton;
/// impl Widget for OkButton {
///     fn render(&mut self, _ui: &mut dyn oxiui_core::UiCtx) {}
///     fn a11y_role(&self) -> A11yRole { A11yRole::Button }
///     fn a11y_label(&self) -> Option<String> { Some("OK".to_string()) }
/// }
///
/// let node = widget_to_a11y_node(&OkButton, NodeId(1));
/// assert_eq!(node.label.as_deref(), Some("OK"));
/// ```
pub fn widget_to_a11y_node(widget: &dyn Widget, id: NodeId) -> A11yNode {
    let widget_role = core_role_to_widget_role(widget.a11y_role());
    let label = widget.a11y_label();
    let description = widget.a11y_description();

    let mut builder = A11yNodeBuilder::new(id, widget_role);

    if let Some(lbl) = label {
        builder = builder.label(&lbl);
    }
    if let Some(desc) = description {
        builder = builder.description(&desc);
    }

    builder.build()
}

// ── A11yWidgetNode extension trait ────────────────────────────────────────────

/// Extension of [`Widget`] that exposes child widgets for accessibility tree
/// construction.
///
/// The base `Widget` trait has no concept of children (it is an immediate-mode
/// interface).  Composite widgets that need to participate in the a11y tree
/// **as a proper subtree** (not just a leaf) implement this trait to return
/// their logical children.
///
/// # Implementing the trait
///
/// ```rust
/// use oxiui_core::{A11yRole, Widget, UiCtx};
/// use oxiui_accessibility::widget_bridge::A11yWidgetNode;
///
/// struct Panel { children: Vec<Box<dyn A11yWidgetNode>> }
///
/// impl Widget for Panel {
///     fn render(&mut self, _ui: &mut dyn UiCtx) {}
///     fn a11y_role(&self) -> A11yRole { A11yRole::Group }
/// }
///
/// impl A11yWidgetNode for Panel {
///     fn a11y_children(&self) -> Vec<&dyn A11yWidgetNode> {
///         self.children.iter().map(|c| c.as_ref()).collect()
///     }
/// }
/// ```
pub trait A11yWidgetNode: Widget {
    /// Return the logical a11y children of this widget.
    ///
    /// Leaf widgets (buttons, labels, inputs) return an empty `Vec`.  Container
    /// widgets return their direct children in layout / document order.
    ///
    /// The default implementation returns an empty `Vec` (leaf node behaviour).
    fn a11y_children(&self) -> Vec<&dyn A11yWidgetNode> {
        Vec::new()
    }
}

// ── Monotonic id allocator (local, private) ──────────────────────────────────

/// Monotonic [`NodeId`] allocator for tree-building helpers.
///
/// Allocates IDs starting from `start`, incrementing by 1 each time.  The
/// allocator is private to this module; callers that need persistent IDs should
/// use [`oxiui_core::WidgetIdAllocator`] instead.
pub struct NodeIdAllocator {
    next: u64,
}

impl NodeIdAllocator {
    /// Create an allocator whose first [`NodeId`] will be `start`.
    pub fn new(start: u64) -> Self {
        Self { next: start }
    }

    /// Allocate the next [`NodeId`].
    pub fn alloc(&mut self) -> NodeId {
        let id = NodeId(self.next);
        self.next += 1;
        id
    }
}

impl Default for NodeIdAllocator {
    /// Start allocating from `NodeId(1)` (0 is conventionally the root).
    fn default() -> Self {
        Self::new(1)
    }
}

// ── Automatic tree generation ────────────────────────────────────────────────

/// Walk an [`A11yWidgetNode`] tree and produce an [`A11yNode`] subtree.
///
/// IDs are allocated monotonically by `alloc`, ensuring uniqueness across the
/// returned subtree.  The root node receives the first allocated ID; children
/// are allocated in pre-order (DFS, left-to-right).
///
/// # Depth guard
///
/// The recursion is bounded by `MAX_DEPTH` (64) to prevent stack overflow on
/// pathologically deep widget trees.  Subtrees beyond that depth are silently
/// truncated (no children emitted for the truncated node).
///
/// # Example
///
/// ```rust
/// use oxiui_core::{A11yRole, Widget, UiCtx};
/// use oxiui_accessibility::widget_bridge::{A11yWidgetNode, NodeIdAllocator, build_a11y_tree};
///
/// struct OkBtn;
/// impl Widget for OkBtn {
///     fn render(&mut self, _ui: &mut dyn UiCtx) {}
///     fn a11y_role(&self) -> A11yRole { A11yRole::Button }
///     fn a11y_label(&self) -> Option<String> { Some("OK".to_string()) }
/// }
/// impl A11yWidgetNode for OkBtn {}
///
/// let mut alloc = NodeIdAllocator::default();
/// let node = build_a11y_tree(&OkBtn, &mut alloc);
/// assert_eq!(node.label.as_deref(), Some("OK"));
/// ```
pub fn build_a11y_tree(root: &dyn A11yWidgetNode, alloc: &mut NodeIdAllocator) -> A11yNode {
    build_node_recursive(root, alloc, 0)
}

/// The maximum recursion depth for [`build_a11y_tree`].
const MAX_DEPTH: usize = 64;

fn build_node_recursive(
    widget: &dyn A11yWidgetNode,
    alloc: &mut NodeIdAllocator,
    depth: usize,
) -> A11yNode {
    let id = alloc.alloc();
    let mut node = widget_to_a11y_node(widget, id);

    // Guard against unbounded recursion on pathological widget trees.
    if depth < MAX_DEPTH {
        for child_widget in widget.a11y_children() {
            let child_node = build_node_recursive(child_widget, alloc, depth + 1);
            node.children.push(child_node);
        }
    }

    node
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::{A11yRole, UiCtx, Widget};

    // ── Minimal test widget stubs ────────────────────────────────────────────

    struct LeafButton {
        label: &'static str,
    }

    impl Widget for LeafButton {
        fn render(&mut self, _ui: &mut dyn UiCtx) {}
        fn a11y_role(&self) -> A11yRole {
            A11yRole::Button
        }
        fn a11y_label(&self) -> Option<String> {
            Some(self.label.to_string())
        }
    }

    impl A11yWidgetNode for LeafButton {}

    struct LeafInput {
        placeholder: &'static str,
    }

    impl Widget for LeafInput {
        fn render(&mut self, _ui: &mut dyn UiCtx) {}
        fn a11y_role(&self) -> A11yRole {
            A11yRole::TextInput
        }
        fn a11y_description(&self) -> Option<String> {
            Some(self.placeholder.to_string())
        }
    }

    impl A11yWidgetNode for LeafInput {}

    struct ContainerPanel {
        label: &'static str,
        children: Vec<Box<dyn A11yWidgetNode>>,
    }

    impl Widget for ContainerPanel {
        fn render(&mut self, _ui: &mut dyn UiCtx) {}
        fn a11y_role(&self) -> A11yRole {
            A11yRole::Group
        }
        fn a11y_label(&self) -> Option<String> {
            if self.label.is_empty() {
                None
            } else {
                Some(self.label.to_string())
            }
        }
    }

    impl A11yWidgetNode for ContainerPanel {
        fn a11y_children(&self) -> Vec<&dyn A11yWidgetNode> {
            self.children.iter().map(|c| c.as_ref()).collect()
        }
    }

    // ── core_role_to_widget_role ─────────────────────────────────────────────

    #[test]
    fn test_core_role_button_maps_to_widget_role_button() {
        assert_eq!(
            core_role_to_widget_role(A11yRole::Button),
            WidgetRole::Button
        );
    }

    #[test]
    fn test_core_role_text_input_maps_correctly() {
        assert_eq!(
            core_role_to_widget_role(A11yRole::TextInput),
            WidgetRole::TextInput
        );
    }

    #[test]
    fn test_core_role_unknown_maps_to_widget_role_unknown() {
        assert_eq!(
            core_role_to_widget_role(A11yRole::Unknown),
            WidgetRole::Unknown
        );
    }

    #[test]
    fn test_core_role_static_text_maps_to_label() {
        assert_eq!(
            core_role_to_widget_role(A11yRole::StaticText),
            WidgetRole::Label
        );
    }

    #[test]
    fn test_core_role_checkbox_maps_correctly() {
        assert_eq!(
            core_role_to_widget_role(A11yRole::Checkbox),
            WidgetRole::Checkbox
        );
    }

    #[test]
    fn test_core_role_heading_maps_to_label() {
        // Heading has no dedicated WidgetRole; Label is the correct fallback.
        assert_eq!(
            core_role_to_widget_role(A11yRole::Heading),
            WidgetRole::Label
        );
    }

    #[test]
    fn test_all_a11y_roles_map_without_panic() {
        // Smoke test: every known A11yRole variant must map without panicking.
        let roles = [
            A11yRole::Group,
            A11yRole::StaticText,
            A11yRole::Button,
            A11yRole::Heading,
            A11yRole::TextInput,
            A11yRole::TextArea,
            A11yRole::Checkbox,
            A11yRole::Slider,
            A11yRole::ProgressBar,
            A11yRole::TabPanel,
            A11yRole::Tab,
            A11yRole::List,
            A11yRole::ListItem,
            A11yRole::Table,
            A11yRole::TableRow,
            A11yRole::TableCell,
            A11yRole::ColumnHeader,
            A11yRole::Dialog,
            A11yRole::Image,
            A11yRole::Link,
            A11yRole::Menu,
            A11yRole::MenuItem,
            A11yRole::Alert,
            A11yRole::Tooltip,
            A11yRole::Tree,
            A11yRole::TreeItem,
            A11yRole::Unknown,
        ];
        for role in roles {
            let _ = core_role_to_widget_role(role);
        }
    }

    // ── widget_to_a11y_node ──────────────────────────────────────────────────

    #[test]
    fn test_widget_to_a11y_node_label_propagated() {
        let widget = LeafButton { label: "Submit" };
        let node = widget_to_a11y_node(&widget, NodeId(42));
        assert_eq!(node.id, NodeId(42));
        assert_eq!(node.label.as_deref(), Some("Submit"));
        assert_eq!(node.role, WidgetRole::Button);
    }

    #[test]
    fn test_widget_to_a11y_node_description_propagated() {
        let widget = LeafInput {
            placeholder: "Enter email",
        };
        let node = widget_to_a11y_node(&widget, NodeId(10));
        assert_eq!(node.props.description.as_deref(), Some("Enter email"));
        assert_eq!(node.role, WidgetRole::TextInput);
    }

    #[test]
    fn test_widget_to_a11y_node_no_label_when_none() {
        struct Silent;
        impl Widget for Silent {
            fn render(&mut self, _ui: &mut dyn UiCtx) {}
        }
        let node = widget_to_a11y_node(&Silent, NodeId(5));
        assert!(node.label.is_none(), "no label should be set");
        assert_eq!(node.role, WidgetRole::Unknown);
    }

    // ── NodeIdAllocator ──────────────────────────────────────────────────────

    #[test]
    fn test_node_id_allocator_increments() {
        let mut alloc = NodeIdAllocator::new(1);
        assert_eq!(alloc.alloc(), NodeId(1));
        assert_eq!(alloc.alloc(), NodeId(2));
        assert_eq!(alloc.alloc(), NodeId(3));
    }

    #[test]
    fn test_node_id_allocator_default_starts_at_1() {
        let mut alloc = NodeIdAllocator::default();
        assert_eq!(alloc.alloc(), NodeId(1));
    }

    // ── build_a11y_tree ──────────────────────────────────────────────────────

    #[test]
    fn test_build_a11y_tree_leaf_single_node() {
        let widget = LeafButton { label: "OK" };
        let mut alloc = NodeIdAllocator::default();
        let node = build_a11y_tree(&widget, &mut alloc);

        assert_eq!(node.label.as_deref(), Some("OK"));
        assert_eq!(node.role, WidgetRole::Button);
        assert!(node.children.is_empty(), "leaf should have no children");
    }

    #[test]
    fn test_build_a11y_tree_container_with_children() {
        let panel = ContainerPanel {
            label: "Form",
            children: vec![
                Box::new(LeafButton { label: "Save" }),
                Box::new(LeafButton { label: "Cancel" }),
            ],
        };

        let mut alloc = NodeIdAllocator::default();
        let root = build_a11y_tree(&panel, &mut alloc);

        assert_eq!(root.role, WidgetRole::Group);
        assert_eq!(root.label.as_deref(), Some("Form"));
        assert_eq!(root.children.len(), 2, "should have 2 children");
        assert_eq!(root.children[0].label.as_deref(), Some("Save"));
        assert_eq!(root.children[1].label.as_deref(), Some("Cancel"));
    }

    #[test]
    fn test_build_a11y_tree_ids_are_unique() {
        let panel = ContainerPanel {
            label: "",
            children: vec![
                Box::new(LeafButton { label: "A" }),
                Box::new(LeafButton { label: "B" }),
                Box::new(LeafButton { label: "C" }),
            ],
        };

        let mut alloc = NodeIdAllocator::default();
        let root = build_a11y_tree(&panel, &mut alloc);

        // Collect all ids in the tree.
        let mut ids = vec![root.id];
        for child in &root.children {
            ids.push(child.id);
        }

        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "all NodeIds in the tree must be unique"
        );
    }

    #[test]
    fn test_build_a11y_tree_nested_containers() {
        // root → panel_a (2 buttons) + panel_b (1 input)
        let panel_a = ContainerPanel {
            label: "PanelA",
            children: vec![
                Box::new(LeafButton { label: "Btn1" }),
                Box::new(LeafButton { label: "Btn2" }),
            ],
        };
        let panel_b = ContainerPanel {
            label: "PanelB",
            children: vec![Box::new(LeafInput {
                placeholder: "Search",
            })],
        };
        let root_panel = ContainerPanel {
            label: "Root",
            children: vec![Box::new(panel_a), Box::new(panel_b)],
        };

        let mut alloc = NodeIdAllocator::default();
        let root = build_a11y_tree(&root_panel, &mut alloc);

        assert_eq!(root.children.len(), 2);
        let child_a = &root.children[0];
        let child_b = &root.children[1];
        assert_eq!(child_a.children.len(), 2, "panel_a should have 2 buttons");
        assert_eq!(child_b.children.len(), 1, "panel_b should have 1 input");
    }

    #[test]
    fn test_build_a11y_tree_integrates_with_a11y_tree_build() {
        // Verify the produced A11yNode can be passed to A11yTree::build without errors.
        use crate::tree::A11yTree;

        let widget = LeafButton { label: "Close" };
        let mut alloc = NodeIdAllocator::default();
        let node = build_a11y_tree(&widget, &mut alloc);

        let update = A11yTree::build(&node);
        assert_eq!(update.nodes.len(), 1, "A11yTree::build should emit 1 node");
        let (_, ref ak_node) = update.nodes[0];
        assert_eq!(ak_node.label(), Some("Close"));
    }
}
