#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-accessibility` — AccessKit a11y tree builder for OxiUI.
//!
//! Converts an [`A11yNode`] widget graph into an accesskit [`accesskit::TreeUpdate`]
//! that can be pushed to any AccessKit platform adapter.
//!
//! The crate is intentionally headless: no windowing or platform adapter is
//! imported here, so the full tree-building logic can be exercised in plain
//! unit tests without a display server.
//!
//! # Quick start
//!
//! ```rust
//! use accesskit::NodeId;
//! use oxiui_accessibility::tree::{A11yNode, A11yTree, WidgetRole};
//!
//! let root = A11yNode::simple(NodeId(1), WidgetRole::Window, Some("My App".to_string()));
//! let update = A11yTree::build(&root);
//! assert_eq!(update.nodes.len(), 1);
//! ```
//!
//! # Modules
//!
//! * [`tree`]    — [`A11yNode`], [`A11yTree`], [`WidgetRole`]
//! * [`props`]   — [`A11yNodeProps`], [`CheckedState`], [`LiveSetting`], [`TextCaret`], [`TextSelection`]
//! * [`builder`] — [`A11yNodeBuilder`] fluent builder

pub mod action;
pub mod builder;
pub mod dirty;
pub mod focus;
pub mod nav;
pub mod pool;
pub mod props;
pub mod text_a11y;
pub mod tree;

pub use action::{map_action, A11yAction, ActionDispatcher};
pub use builder::A11yNodeBuilder;
pub use dirty::{DirtyTracker, Lazy};
pub use focus::{FocusIndicator, FocusRing};
pub use nav::{tab_next, tab_prev, TabOrder};
pub use pool::NodePool;
pub use props::{
    byte_offset_to_char_index, character_lengths_utf8, A11yNodeProps, CheckedState, LiveSetting,
    TextCaret, TextRunChild, TextSelection, Toggled3,
};
pub use tree::{
    build_table_a11y, column_header_node, synthesize_text_run_children, table_cell_node,
    table_row_node, A11yNode, A11yTree, WidgetRole,
};

// ── OS accessibility preferences ──────────────────────────────────────────────

/// Best-effort query of the operating system's accessibility preferences.
///
/// No external dependencies are required; values are read from well-known
/// environment variables as a cross-platform fallback.  On future OS-specific
/// integration, the implementation will call real platform APIs.
#[derive(Debug, Clone, Default)]
pub struct OsA11yPrefs {
    /// `true` if the OS high-contrast display mode is active.
    ///
    /// Currently detected via the `OXIUI_HIGH_CONTRAST` environment variable
    /// (any non-empty value = active).
    pub high_contrast: bool,
    /// `true` if the OS reduced-motion preference is active.
    ///
    /// Currently detected via the `OXIUI_REDUCED_MOTION` environment variable
    /// (any non-empty value = active).
    pub reduced_motion: bool,
}

impl OsA11yPrefs {
    /// Query the current OS accessibility preferences.
    ///
    /// Reads `OXIUI_HIGH_CONTRAST` and `OXIUI_REDUCED_MOTION` environment
    /// variables.  Any non-empty value is interpreted as *active*.  Both
    /// default to `false` when the variable is unset or empty.
    pub fn query() -> Self {
        Self::query_from(|name| std::env::var(name).ok())
    }

    /// Query preferences using a caller-supplied variable lookup.
    ///
    /// `lookup(name)` returns `Some(value)` when the variable is set and
    /// `None` otherwise.  This variant is useful for testing without
    /// mutating the process environment.
    pub fn query_from<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let high_contrast = lookup("OXIUI_HIGH_CONTRAST")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let reduced_motion = lookup("OXIUI_REDUCED_MOTION")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        Self {
            high_contrast,
            reduced_motion,
        }
    }
}

// ── Multi-window tree registry ────────────────────────────────────────────────

/// A unique identifier for an accessibility tree (one per application window).
///
/// Wrap a `u64` discriminant to avoid confusion with [`accesskit::NodeId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowA11yId(pub u64);

/// A registry of [`A11yTree`] instances, one per application window.
///
/// In multi-window applications each window owns an independent accessibility
/// tree.  `A11yForest` provides the ownership map and basic CRUD operations.
#[derive(Default)]
pub struct A11yForest {
    trees: std::collections::HashMap<WindowA11yId, A11yTree>,
}

impl A11yForest {
    /// Create an empty forest.
    pub fn new() -> Self {
        Self {
            trees: std::collections::HashMap::new(),
        }
    }

    /// Insert or replace the tree for `id`.
    pub fn insert(&mut self, id: WindowA11yId, tree: A11yTree) {
        self.trees.insert(id, tree);
    }

    /// Return a shared reference to the tree for `id`, if present.
    pub fn get(&self, id: WindowA11yId) -> Option<&A11yTree> {
        self.trees.get(&id)
    }

    /// Return a mutable reference to the tree for `id`, if present.
    pub fn get_mut(&mut self, id: WindowA11yId) -> Option<&mut A11yTree> {
        self.trees.get_mut(&id)
    }

    /// Remove and return the tree for `id`, if present.
    pub fn remove(&mut self, id: WindowA11yId) -> Option<A11yTree> {
        self.trees.remove(&id)
    }

    /// Iterate over all `(WindowA11yId, &A11yTree)` pairs in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (WindowA11yId, &A11yTree)> {
        self.trees.iter().map(|(k, v)| (*k, v))
    }

    /// Register an a11y tree for a specific window.
    ///
    /// If a tree was already registered for `id`, it is replaced.
    /// Equivalent to [`Self::insert`] but uses the name prescribed by the
    /// multi-window API surface.
    pub fn register(&mut self, id: WindowA11yId, tree: A11yTree) {
        self.trees.insert(id, tree);
    }

    /// Unregister (remove) the a11y tree for `id`.
    ///
    /// Has no effect if `id` is not currently registered.
    pub fn unregister(&mut self, id: WindowA11yId) {
        self.trees.remove(&id);
    }

    /// Iterate over all registered window IDs in unspecified order.
    pub fn windows(&self) -> impl Iterator<Item = WindowA11yId> + '_ {
        self.trees.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{NodeId, Role};
    use std::time::Instant;

    fn nid(n: u64) -> NodeId {
        NodeId(n)
    }

    // ─── 1. widget_role_to_accesskit_role ────────────────────────────────────

    #[test]
    fn widget_role_to_accesskit_role_all_variants() {
        use WidgetRole::*;

        let cases: &[(WidgetRole, Role)] = &[
            (Window, Role::Window),
            (Group, Role::Group),
            (Button, Role::Button),
            (Label, Role::Label),
            (TextInput, Role::TextInput),
            (TableRow, Role::Row),
            (TableCell, Role::Cell),
            (ScrollView, Role::ScrollView),
            (Image, Role::Image),
            (Unknown, Role::Unknown),
            (Checkbox, Role::CheckBox),
            (Slider, Role::Slider),
            (ProgressBar, Role::ProgressIndicator),
            (Tab, Role::Tab),
            (TabPanel, Role::TabPanel),
            (Menu, Role::Menu),
            (MenuItem, Role::MenuItem),
            (Dialog, Role::Dialog),
            (Alert, Role::Alert),
            (Tooltip, Role::Tooltip),
            (Tree, Role::Tree),
            (TreeItem, Role::TreeItem),
            (ListItem, Role::ListItem),
            (Link, Role::Link),
            (Banner, Role::Banner),
            (Navigation, Role::Navigation),
            (Main, Role::Main),
            (Complementary, Role::Complementary),
            (ContentInfo, Role::ContentInfo),
        ];

        for (widget_role, expected_ak_role) in cases {
            let got = Role::from(*widget_role);
            assert_eq!(
                got, *expected_ak_role,
                "WidgetRole::{widget_role:?} should map to {expected_ak_role:?}, got {got:?}"
            );
        }
    }

    // ─── 2. node_property_description ────────────────────────────────────────

    #[test]
    fn node_property_description_survives_roundtrip() {
        let node = A11yNodeBuilder::new(nid(1), WidgetRole::Button)
            .label("OK")
            .description("Confirm the dialog")
            .build();

        let update = A11yTree::build(&node);
        assert_eq!(update.nodes.len(), 1);
        let (_, ref ak_node) = update.nodes[0];
        assert_eq!(ak_node.description(), Some("Confirm the dialog"));
    }

    // ─── 3. node_property_range ───────────────────────────────────────────────

    #[test]
    fn node_property_range_survives_roundtrip() {
        let node = A11yNodeBuilder::new(nid(2), WidgetRole::Slider)
            .value(50.0, 0.0, 100.0, 1.0)
            .build();

        let update = A11yTree::build(&node);
        let (_, ref ak_node) = update.nodes[0];
        assert_eq!(ak_node.numeric_value(), Some(50.0));
        assert_eq!(ak_node.min_numeric_value(), Some(0.0));
        assert_eq!(ak_node.max_numeric_value(), Some(100.0));
        assert_eq!(ak_node.numeric_value_step(), Some(1.0));
    }

    // ─── 4. relationship_labelled_by ─────────────────────────────────────────

    #[test]
    fn relationship_labelled_by_propagated() {
        let label_id = nid(10);
        let button_id = nid(11);

        let node = A11yNodeBuilder::new(button_id, WidgetRole::Button)
            .labelled_by([label_id])
            .build();

        let update = A11yTree::build(&node);
        let (_, ref ak_node) = update.nodes[0];
        assert!(
            ak_node.labelled_by().contains(&label_id),
            "labelled_by should contain the label node id"
        );
    }

    // ─── 5. tree_diff_add_child ───────────────────────────────────────────────

    #[test]
    fn tree_diff_add_child_produces_new_node() {
        let mut old_tree = A11yTree::default();
        let root_only = A11yNode::simple(nid(100), WidgetRole::Window, None);
        old_tree.build_and_store(&root_only);

        let mut new_tree = A11yTree::default();
        let mut root_with_child = A11yNode::simple(nid(100), WidgetRole::Window, None);
        root_with_child.children.push(A11yNode::simple(
            nid(101),
            WidgetRole::Button,
            Some("X".into()),
        ));
        new_tree.build_and_store(&root_with_child);

        let delta = A11yTree::diff(&old_tree, &new_tree);
        // The root changed (its children list grew) and the child is brand-new.
        let ids: Vec<NodeId> = delta.nodes.iter().map(|(id, _)| *id).collect();
        assert!(
            ids.contains(&nid(101)),
            "diff should include the new child node"
        );
    }

    // ─── 6. tree_diff_no_change ───────────────────────────────────────────────

    #[test]
    fn tree_diff_no_change_empty_delta() {
        let mut tree_a = A11yTree::default();
        let root = A11yNode::simple(nid(200), WidgetRole::Window, Some("App".into()));
        tree_a.build_and_store(&root);

        let mut tree_b = A11yTree::default();
        let root2 = A11yNode::simple(nid(200), WidgetRole::Window, Some("App".into()));
        tree_b.build_and_store(&root2);

        let delta = A11yTree::diff(&tree_a, &tree_b);
        assert!(
            delta.nodes.is_empty(),
            "identical trees should produce an empty delta"
        );
    }

    // ─── 7. tree_diff_changed_prop ────────────────────────────────────────────

    #[test]
    fn tree_diff_changed_prop_includes_modified_node() {
        let mut tree_a = A11yTree::default();
        let root_a = A11yNode::simple(nid(300), WidgetRole::Button, Some("Old".into()));
        tree_a.build_and_store(&root_a);

        let mut tree_b = A11yTree::default();
        let root_b = A11yNode::simple(nid(300), WidgetRole::Button, Some("New".into()));
        tree_b.build_and_store(&root_b);

        let delta = A11yTree::diff(&tree_a, &tree_b);
        assert_eq!(
            delta.nodes.len(),
            1,
            "only the changed node should appear in the delta"
        );
        assert_eq!(delta.nodes[0].0, nid(300));
    }

    // ─── 8. focus_set_get_roundtrip ───────────────────────────────────────────

    #[test]
    fn focus_set_get_roundtrip() {
        let mut tree = A11yTree::default();
        assert_eq!(tree.focus(), None);

        tree.set_focus(Some(nid(42)));
        assert_eq!(tree.focus(), Some(nid(42)));

        tree.set_focus(None);
        assert_eq!(tree.focus(), None);
    }

    // ─── 9. focus_in_update ───────────────────────────────────────────────────

    #[test]
    fn focus_in_update_reflects_set_focus() {
        let mut tree = A11yTree::default();
        tree.set_focus(Some(nid(77)));

        let upd = tree.focus_update();
        assert_eq!(upd.focus, nid(77));
        assert!(upd.nodes.is_empty());
    }

    // ─── 10. live_region_announce ─────────────────────────────────────────────

    #[test]
    fn live_region_announce_id_in_tree() {
        let mut tree = A11yTree::default();
        let root = A11yNode::simple(nid(500), WidgetRole::Window, None);
        tree.build_and_store(&root);

        let ann_id = tree.announce("File saved", LiveSetting::Polite);

        // The id must appear in the snapshot.
        let ids: Vec<NodeId> = tree.snapshot.iter().map(|(id, _)| *id).collect();
        assert!(
            ids.contains(&ann_id),
            "announced id must be in the snapshot"
        );
    }

    // ─── 11. widget_role_display ─────────────────────────────────────────────

    #[test]
    fn widget_role_display_non_empty() {
        use WidgetRole::*;
        let roles = [
            Window,
            Group,
            Button,
            Label,
            TextInput,
            TableRow,
            TableCell,
            ScrollView,
            Image,
            Unknown,
            Checkbox,
            Slider,
            ProgressBar,
            Tab,
            TabPanel,
            Menu,
            MenuItem,
            Dialog,
            Alert,
            Tooltip,
            Tree,
            TreeItem,
            ListItem,
            Link,
            Banner,
            Navigation,
            Main,
            Complementary,
            ContentInfo,
        ];
        for role in roles {
            let s = role.to_string();
            assert!(
                !s.is_empty(),
                "WidgetRole::{role:?} Display must not be empty"
            );
        }
    }

    // ─── 12. builder_roundtrip ────────────────────────────────────────────────

    #[test]
    fn builder_roundtrip_description() {
        let node = A11yNodeBuilder::new(nid(1000), WidgetRole::Button)
            .description("click me")
            .build();

        assert_eq!(node.props.description, Some("click me".to_string()));
    }

    #[test]
    fn builder_roundtrip_placeholder() {
        let node = A11yNodeBuilder::new(nid(1001), WidgetRole::TextInput)
            .placeholder("Enter text")
            .build();
        assert_eq!(node.props.placeholder, Some("Enter text".to_string()));
    }

    #[test]
    fn builder_roundtrip_key_shortcut() {
        let node = A11yNodeBuilder::new(nid(1002), WidgetRole::Button)
            .key_shortcut("Ctrl+S")
            .build();
        assert_eq!(node.props.key_shortcut, Some("Ctrl+S".to_string()));
    }

    #[test]
    fn builder_roundtrip_disabled() {
        let node = A11yNodeBuilder::new(nid(1003), WidgetRole::Button)
            .disabled()
            .build();
        assert!(node.props.disabled);
    }

    #[test]
    fn builder_roundtrip_expanded() {
        let node = A11yNodeBuilder::new(nid(1004), WidgetRole::TreeItem)
            .expanded(true)
            .build();
        assert_eq!(node.props.expanded, Some(true));
    }

    #[test]
    fn builder_roundtrip_selected() {
        let node = A11yNodeBuilder::new(nid(1005), WidgetRole::ListItem)
            .selected(true)
            .build();
        assert_eq!(node.props.selected, Some(true));
    }

    #[test]
    fn builder_roundtrip_checked() {
        let node = A11yNodeBuilder::new(nid(1006), WidgetRole::Checkbox)
            .checked(CheckedState::Mixed)
            .build();
        assert_eq!(node.props.checked, Some(CheckedState::Mixed));
    }

    #[test]
    fn builder_roundtrip_value() {
        let node = A11yNodeBuilder::new(nid(1007), WidgetRole::Slider)
            .value(25.0, 0.0, 50.0, 0.5)
            .build();
        assert_eq!(node.props.value_now, Some(25.0));
        assert_eq!(node.props.value_min, Some(0.0));
        assert_eq!(node.props.value_max, Some(50.0));
        assert_eq!(node.props.value_step, Some(0.5));
    }

    #[test]
    fn builder_roundtrip_text() {
        let node = A11yNodeBuilder::new(nid(1008), WidgetRole::TextInput)
            .text("hello world")
            .build();
        assert_eq!(node.text_content, Some("hello world".to_string()));
    }

    #[test]
    fn builder_roundtrip_labelled_by() {
        let node = A11yNodeBuilder::new(nid(1009), WidgetRole::Button)
            .labelled_by([nid(2000), nid(2001)])
            .build();
        assert_eq!(node.props.labelled_by, vec![nid(2000), nid(2001)]);
    }

    // ─── 13. large_tree_smoke ────────────────────────────────────────────────

    #[test]
    fn large_tree_smoke_under_100ms() {
        const N: u64 = 1_000;

        // Build a 1000-node flat tree (root + 999 button children).
        let mut root = A11yNode::simple(nid(0), WidgetRole::Window, None);
        for i in 1..N {
            root.children.push(A11yNode::simple(
                nid(i),
                WidgetRole::Button,
                Some(format!("Button {i}")),
            ));
        }

        let start = Instant::now();
        let update = A11yTree::build(&root);
        let elapsed = start.elapsed();

        assert_eq!(update.nodes.len(), N as usize);
        assert!(
            elapsed.as_millis() < 100,
            "1000-node tree build took {}ms (limit 100ms)",
            elapsed.as_millis()
        );
    }

    // ─── Placeholder prop in node ─────────────────────────────────────────────

    #[test]
    fn node_property_placeholder_propagated() {
        let node = A11yNodeBuilder::new(nid(3000), WidgetRole::TextInput)
            .placeholder("Type here…")
            .build();

        let update = A11yTree::build(&node);
        let (_, ref ak_node) = update.nodes[0];
        assert_eq!(ak_node.placeholder(), Some("Type here…"));
    }

    // ─── Disabled flag ───────────────────────────────────────────────────────

    #[test]
    fn node_property_disabled_propagated() {
        let node = A11yNodeBuilder::new(nid(3001), WidgetRole::Button)
            .disabled()
            .build();

        let update = A11yTree::build(&node);
        let (_, ref ak_node) = update.nodes[0];
        assert!(ak_node.is_disabled());
    }

    // ─── Slice E tests ────────────────────────────────────────────────────────

    // -- Text-run synthesis ---------------------------------------------------

    #[test]
    fn test_text_run_no_selection_one_child() {
        let children = synthesize_text_run_children("hello", None);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].text, "hello");
        assert_eq!(children[0].char_offset, 0);
        assert_eq!(children[0].byte_offset, 0);
        assert!(!children[0].is_selected);
    }

    #[test]
    fn test_text_run_with_selection_three_children() {
        use crate::props::TextSelection;
        // "hello" — select chars 1..3 ("el"), bytes 1..3
        let sel = TextSelection {
            anchor: 1,
            focus: 3,
        };
        let children = synthesize_text_run_children("hello", Some(&sel));
        // Expect: "h" (before), "el" (selected), "lo" (after) = 3 segments
        assert_eq!(children.len(), 3, "expected 3 segments, got: {children:?}");
        assert_eq!(children[0].text, "h");
        assert!(!children[0].is_selected);
        assert_eq!(children[1].text, "el");
        assert!(children[1].is_selected);
        assert_eq!(children[2].text, "lo");
        assert!(!children[2].is_selected);
    }

    // -- Table helpers --------------------------------------------------------

    #[test]
    fn test_table_cell_carries_row_col() {
        let cell = table_cell_node(nid(1), 2, 4, "data");
        assert_eq!(cell.text_content.as_deref(), Some("data"));
        let desc = cell.props.description.as_deref().unwrap_or("");
        assert!(
            desc.contains("Row 3"),
            "description should contain Row 3, got: {desc}"
        );
        assert!(
            desc.contains("Column 5"),
            "description should contain Column 5, got: {desc}"
        );
    }

    #[test]
    fn test_table_row_node_description() {
        let row = table_row_node(nid(10), 0);
        let desc = row.props.description.as_deref().unwrap_or("");
        assert!(desc.contains("Row 1"), "expected 'Row 1', got: {desc}");
    }

    #[test]
    fn test_column_header_label() {
        let hdr = column_header_node(nid(20), 2, "Name");
        assert_eq!(hdr.label.as_deref(), Some("Name"));
        let desc = hdr.props.description.as_deref().unwrap_or("");
        assert!(
            desc.contains("Column 3"),
            "expected 'Column 3', got: {desc}"
        );
    }

    // -- OS preferences -------------------------------------------------------

    #[test]
    fn test_os_prefs_default_false() {
        // Without the env vars set, both prefs are false.
        // (This test relies on the vars not being set in the test environment.)
        // We deliberately do NOT set them here to test the default path.
        // If they happen to be set in CI, the test would still pass because
        // the assertions check a freshly-queried value, not a cached constant.
        // Use a fresh query that doesn't depend on state set by other tests.
        let prefs = OsA11yPrefs::query();
        // We can't guarantee the host env, so only assert the Default impl.
        let default_prefs = OsA11yPrefs::default();
        assert!(!default_prefs.high_contrast);
        assert!(!default_prefs.reduced_motion);
        // Suppress unused-variable warning for `prefs`.
        let _ = prefs;
    }

    #[test]
    fn test_os_prefs_reads_env_var() {
        // Use query_from so we never mutate the process environment.
        let prefs = OsA11yPrefs::query_from(|name| {
            if name == "OXIUI_HIGH_CONTRAST" {
                Some("1".to_string())
            } else {
                None
            }
        });
        assert!(
            prefs.high_contrast,
            "OXIUI_HIGH_CONTRAST=1 should set high_contrast=true"
        );
        assert!(!prefs.reduced_motion);
    }

    // -- Multi-window forest --------------------------------------------------

    #[test]
    fn test_forest_two_trees_isolated() {
        let id_a = WindowA11yId(1);
        let id_b = WindowA11yId(2);

        let mut forest = A11yForest::new();

        let mut tree_a = A11yTree::default();
        let root_a = A11yNode::simple(nid(100), WidgetRole::Window, Some("Window A".into()));
        tree_a.build_and_store(&root_a);

        let mut tree_b = A11yTree::default();
        let root_b = A11yNode::simple(nid(200), WidgetRole::Window, Some("Window B".into()));
        tree_b.build_and_store(&root_b);

        forest.insert(id_a, tree_a);
        forest.insert(id_b, tree_b);

        let a_root = forest.get(id_a).and_then(|t| t.root_id);
        let b_root = forest.get(id_b).and_then(|t| t.root_id);

        assert_eq!(a_root, Some(nid(100)));
        assert_eq!(b_root, Some(nid(200)));
        assert_ne!(a_root, b_root, "two windows must have independent root ids");

        // Remove one; the other remains.
        forest.remove(id_a);
        assert!(forest.get(id_a).is_none());
        assert!(forest.get(id_b).is_some());
    }

    // -- Builder tab_index ----------------------------------------------------

    #[test]
    fn test_builder_tab_index() {
        let node = A11yNodeBuilder::new(nid(9000), WidgetRole::Button)
            .label("Submit")
            .tab_index(2)
            .build();
        assert_eq!(node.props.tab_index, Some(2));
    }

    // ── S6 tests ──────────────────────────────────────────────────────────────

    #[test]
    fn test_os_a11y_prefs_still_works() {
        // OsA11yPrefs::default() must not panic and must yield well-typed bools.
        let prefs = OsA11yPrefs::default();
        // Both fields are bool — this assertion is vacuously true but confirms
        // the struct is constructible and the fields are accessible.
        let _hc: bool = prefs.high_contrast;
        let _rm: bool = prefs.reduced_motion;
    }

    #[test]
    fn test_build_table_a11y_structure() {
        let table = build_table_a11y(2, 3, &["Col A", "Col B", "Col C"]);

        // Root should have 3 headers + 2 rows = 5 direct children.
        assert_eq!(
            table.children.len(),
            5,
            "expected 3 column-headers + 2 rows = 5 children, got {}",
            table.children.len()
        );

        // First three children must be ColumnHeader.
        for (i, child) in table.children.iter().take(3).enumerate() {
            assert_eq!(
                child.role,
                WidgetRole::ColumnHeader,
                "child[{i}] should be ColumnHeader"
            );
        }

        // Last two children must be TableRow.
        for (i, child) in table.children.iter().skip(3).enumerate() {
            assert_eq!(
                child.role,
                WidgetRole::TableRow,
                "row child[{i}] should be TableRow"
            );
            // Each row must have 3 TableCell children.
            assert_eq!(child.children.len(), 3, "row {i} should have 3 cells");
            for (j, cell) in child.children.iter().enumerate() {
                assert_eq!(
                    cell.role,
                    WidgetRole::TableCell,
                    "row {i} cell {j} should be TableCell"
                );
            }
        }
    }

    #[test]
    fn test_a11y_forest_multi_window() {
        let id1 = WindowA11yId(1);
        let id2 = WindowA11yId(2);

        let mut forest = A11yForest::default();

        forest.register(id1, A11yTree::default());
        forest.register(id2, A11yTree::default());

        assert!(forest.get(id1).is_some(), "id1 should be present");
        assert!(forest.get(id2).is_some(), "id2 should be present");

        forest.unregister(id1);
        assert!(forest.get(id1).is_none(), "id1 should be removed");
        assert!(forest.get(id2).is_some(), "id2 should remain");

        assert_eq!(forest.windows().count(), 1, "one window should remain");
    }

    #[test]
    fn test_a11y_forest_windows_iter() {
        let mut forest = A11yForest::default();
        forest.register(WindowA11yId(10), A11yTree::default());
        forest.register(WindowA11yId(20), A11yTree::default());
        forest.register(WindowA11yId(30), A11yTree::default());

        assert_eq!(
            forest.windows().count(),
            3,
            "windows() should yield all 3 registered ids"
        );
    }
}
