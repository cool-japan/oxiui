//! Integration tests: OxiUI accessibility bridge with egui.
//!
//! Tests that `oxiui-accessibility::A11yTree` converts correctly to AccessKit
//! `TreeUpdate` objects and that the `A11yEguiBridge` stateful wrapper handles
//! first-frame full updates and subsequent-frame diffs correctly.
//!
//! The `oxiui-accessibility` crate is a dev-dependency, so these run as
//! integration tests only.

use accesskit::NodeId;
use oxiui_accessibility::tree::{A11yNode, A11yTree, WidgetRole};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn simple_root(id: u64) -> A11yNode {
    A11yNode::simple(NodeId(id), WidgetRole::Window, Some("App".to_owned()))
}

fn button_node(id: u64) -> A11yNode {
    A11yNode::simple(NodeId(id), WidgetRole::Button, Some("OK".to_owned()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// `A11yTree::build` converts a single root node to a `TreeUpdate` with one
/// node entry.
#[test]
fn a11y_tree_build_single_node() {
    let root = simple_root(1);
    let update = A11yTree::build(&root);
    assert_eq!(update.nodes.len(), 1, "expected 1 node in TreeUpdate");
}

/// `A11yTree::build` includes children in the `TreeUpdate`.
#[test]
fn a11y_tree_build_with_children() {
    let mut root = simple_root(1);
    root.children = vec![button_node(2), button_node(3)];
    let update = A11yTree::build(&root);
    // Root + 2 children = 3 nodes.
    assert_eq!(update.nodes.len(), 3);
}

/// Root node id is present in the `TreeUpdate.nodes` list.
#[test]
fn a11y_tree_build_root_id_present() {
    let root = simple_root(42);
    let update = A11yTree::build(&root);
    let ids: Vec<NodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&NodeId(42)), "root NodeId(42) not in update");
}

/// `A11yTree::build_and_store` returns the same update as `build`.
#[test]
fn a11y_tree_build_and_store_consistent() {
    let root = simple_root(1);
    let mut tree = A11yTree::default();
    let stored_update = tree.build_and_store(&root);
    let fresh_update = A11yTree::build(&root);
    assert_eq!(
        stored_update.nodes.len(),
        fresh_update.nodes.len(),
        "build_and_store and build must produce the same node count"
    );
}

/// `A11yTree::diff` between identical trees produces no changed nodes.
#[test]
fn a11y_tree_diff_identical_trees_no_changes() {
    let root = simple_root(1);
    let mut old_tree = A11yTree::default();
    old_tree.build_and_store(&root);
    let mut new_tree = A11yTree::default();
    new_tree.build_and_store(&root);

    let diff = A11yTree::diff(&old_tree, &new_tree);
    // Identical trees: no changed nodes.
    assert!(
        diff.nodes.is_empty(),
        "diff of identical trees should have no changed nodes; got {}",
        diff.nodes.len()
    );
}

/// `A11yTree::diff` between trees where the new tree has an extra child reports
/// the changed nodes.
#[test]
fn a11y_tree_diff_added_child() {
    let root_no_child = simple_root(1);
    let mut root_with_child = simple_root(1);
    root_with_child.children = vec![button_node(2)];

    let mut old_tree = A11yTree::default();
    old_tree.build_and_store(&root_no_child);
    let mut new_tree = A11yTree::default();
    new_tree.build_and_store(&root_with_child);

    let diff = A11yTree::diff(&old_tree, &new_tree);
    // At minimum, the root changed (children list changed) + new child is added.
    assert!(
        !diff.nodes.is_empty(),
        "diff with added child should have changed nodes"
    );
}

/// `A11yTree::set_focus` sets the focus and `focus()` accessor returns it.
#[test]
fn a11y_tree_set_and_get_focus() {
    let mut tree = A11yTree::default();
    assert_eq!(tree.focus(), None);
    tree.set_focus(Some(NodeId(5)));
    assert_eq!(tree.focus(), Some(NodeId(5)));
    tree.set_focus(None);
    assert_eq!(tree.focus(), None);
}

/// `A11yTree::focus_update` produces a `TreeUpdate` with the focus set.
#[test]
fn a11y_tree_focus_update_reflects_focus() {
    let mut tree = A11yTree::default();
    tree.set_focus(Some(NodeId(7)));
    let update = tree.focus_update();
    assert_eq!(update.focus, NodeId(7));
}

/// `A11yTree::announce` creates a live-region node and returns a NodeId.
#[test]
fn a11y_tree_announce_returns_node_id() {
    let mut tree = A11yTree::default();
    let id = tree.announce(
        "Alert: action completed",
        oxiui_accessibility::props::LiveSetting::Assertive,
    );
    // The id must be non-zero (valid).
    assert_ne!(
        id,
        NodeId(0),
        "announce must return a valid non-zero NodeId"
    );
}

/// `oxiui_accessibility::build_a11y_tree` builds a tree from an `A11yWidgetNode` reference.
#[test]
fn build_a11y_tree_from_widget() {
    use oxiui_accessibility::widget_bridge::A11yWidgetNode;
    use oxiui_accessibility::{build_a11y_tree, NodeIdAllocator};
    use oxiui_core::{A11yRole, UiCtx, Widget};

    struct DummyButton;
    impl Widget for DummyButton {
        fn render(&mut self, _ui: &mut dyn UiCtx) {}
        fn a11y_role(&self) -> A11yRole {
            A11yRole::Button
        }
        fn a11y_label(&self) -> Option<String> {
            Some("Submit".to_owned())
        }
    }
    // `A11yWidgetNode` is automatically implemented for `Widget` types (default impl).
    impl A11yWidgetNode for DummyButton {}

    let mut allocator = NodeIdAllocator::default();
    let widget = DummyButton;
    let node = build_a11y_tree(&widget, &mut allocator);
    // The node's id must be a valid NodeId.
    assert_ne!(node.id, NodeId(0));
}

/// `oxiui_egui::a11y` bridge: `oxiui_tree_to_accesskit` wraps `A11yTree::build`.
#[cfg(feature = "a11y")]
#[test]
fn a11y_bridge_oxiui_tree_to_accesskit() {
    use oxiui_egui::a11y::oxiui_tree_to_accesskit;
    let root = simple_root(1);
    let update = oxiui_tree_to_accesskit(&root);
    assert_eq!(update.nodes.len(), 1);
}

/// `oxiui_egui::a11y` bridge: `diff_a11y_trees` between identical trees is empty.
#[cfg(feature = "a11y")]
#[test]
fn a11y_bridge_diff_identical() {
    use oxiui_egui::a11y::diff_a11y_trees;
    let root = simple_root(1);
    let mut old_t = A11yTree::default();
    old_t.build_and_store(&root);
    let mut new_t = A11yTree::default();
    new_t.build_and_store(&root);
    let diff = diff_a11y_trees(&old_t, &new_t);
    assert!(diff.nodes.is_empty());
}

/// `A11yEguiBridge::update` on the first frame produces a full tree.
#[cfg(feature = "a11y")]
#[test]
fn a11y_bridge_first_frame_full_update() {
    use oxiui_egui::a11y::A11yEguiBridge;
    let mut bridge = A11yEguiBridge::new();
    let root = simple_root(1);
    let update = bridge.update(&root);
    assert_eq!(update.nodes.len(), 1, "first frame must be a full update");
}

/// `A11yEguiBridge::update` on the second identical frame produces an empty diff.
#[cfg(feature = "a11y")]
#[test]
fn a11y_bridge_second_frame_empty_diff() {
    use oxiui_egui::a11y::A11yEguiBridge;
    let mut bridge = A11yEguiBridge::new();
    let root = simple_root(1);
    // First frame: full.
    let _ = bridge.update(&root);
    // Second frame: same root, diff should be empty.
    let diff = bridge.update(&root);
    assert!(
        diff.nodes.is_empty(),
        "second identical frame should produce empty diff; got {} nodes",
        diff.nodes.len()
    );
}

/// `A11yEguiBridge::set_focus` returns a focus-only `TreeUpdate`.
#[cfg(feature = "a11y")]
#[test]
fn a11y_bridge_set_focus_update() {
    use oxiui_egui::a11y::A11yEguiBridge;
    let mut bridge = A11yEguiBridge::new();
    let update = bridge.set_focus(Some(NodeId(3)));
    assert_eq!(update.focus, NodeId(3));
    assert!(
        update.nodes.is_empty(),
        "focus-only update should have no nodes"
    );
}
