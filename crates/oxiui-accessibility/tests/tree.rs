//! Headless integration tests for the oxiui-accessibility tree builder.

use accesskit::{Action, ActionRequest, NodeId, Role, TreeId};
use oxiui_accessibility::dirty::DirtyTracker;
use oxiui_accessibility::dirty::Lazy;
use oxiui_accessibility::nav::TabOrder;
use oxiui_accessibility::pool::NodePool;
use oxiui_accessibility::tree::{A11yNode, A11yTree, WidgetRole};
use oxiui_accessibility::{tab_next, tab_prev, ActionDispatcher, WindowA11yId};

// Helper: construct a NodeId from a plain u64 (accesskit NodeId wraps u64).
fn node_id(n: u64) -> NodeId {
    NodeId(n)
}

// Helper: construct a simple A11yNode with just id, role, label, no children, no extra props.
fn simple_node(id: NodeId, role: WidgetRole, label: Option<&str>) -> A11yNode {
    A11yNode::simple(id, role, label.map(str::to_owned))
}

// ── Basic tree shape ─────────────────────────────────────────────────────────

#[test]
fn tree_update_non_empty_for_sample_ui() {
    let mut root = simple_node(node_id(1), WidgetRole::Window, Some("My App"));
    root.children.push(simple_node(
        node_id(2),
        WidgetRole::Button,
        Some("Click me"),
    ));
    root.children.push(simple_node(
        node_id(3),
        WidgetRole::Label,
        Some("Hello World"),
    ));

    let update = A11yTree::build(&root);
    assert!(!update.nodes.is_empty(), "tree update must have nodes");
    assert_eq!(update.nodes.len(), 3, "root + 2 children");
}

// ── Role mapping ─────────────────────────────────────────────────────────────

#[test]
fn button_role_maps_correctly() {
    let node = simple_node(node_id(10), WidgetRole::Button, Some("Submit"));
    let update = A11yTree::build(&node);
    assert_eq!(update.nodes.len(), 1);
    let (_, ref button_node) = update.nodes[0];
    assert_eq!(button_node.role(), Role::Button);
}

#[test]
fn table_row_role_maps_correctly() {
    let node = simple_node(node_id(20), WidgetRole::TableRow, None);
    let update = A11yTree::build(&node);
    assert_eq!(update.nodes.len(), 1);
    let (_, ref row_node) = update.nodes[0];
    assert_eq!(row_node.role(), Role::Row);
}

#[test]
fn table_cell_role_maps_correctly() {
    let node = simple_node(node_id(21), WidgetRole::TableCell, None);
    let update = A11yTree::build(&node);
    let (_, ref cell_node) = update.nodes[0];
    assert_eq!(cell_node.role(), Role::Cell);
}

#[test]
fn window_role_maps_correctly() {
    let node = simple_node(node_id(30), WidgetRole::Window, Some("Root Window"));
    let update = A11yTree::build(&node);
    let (_, ref win_node) = update.nodes[0];
    assert_eq!(win_node.role(), Role::Window);
}

#[test]
fn label_role_maps_correctly() {
    let node = simple_node(node_id(40), WidgetRole::Label, Some("Status: OK"));
    let update = A11yTree::build(&node);
    let (_, ref lbl_node) = update.nodes[0];
    assert_eq!(lbl_node.role(), Role::Label);
}

#[test]
fn scroll_view_role_maps_correctly() {
    let node = simple_node(node_id(50), WidgetRole::ScrollView, None);
    let update = A11yTree::build(&node);
    let (_, ref sv_node) = update.nodes[0];
    assert_eq!(sv_node.role(), Role::ScrollView);
}

// ── Label propagation ────────────────────────────────────────────────────────

#[test]
fn label_is_set_on_node_when_provided() {
    let node = simple_node(node_id(60), WidgetRole::Button, Some("Save"));
    let update = A11yTree::build(&node);
    let (_, ref ak_node) = update.nodes[0];
    assert_eq!(ak_node.label(), Some("Save"));
}

#[test]
fn label_is_absent_when_none() {
    let node = simple_node(node_id(61), WidgetRole::Group, None);
    let update = A11yTree::build(&node);
    let (_, ref ak_node) = update.nodes[0];
    assert_eq!(ak_node.label(), None);
}

// ── Tree structure ────────────────────────────────────────────────────────────

#[test]
fn root_id_and_focus_match() {
    let node = simple_node(node_id(100), WidgetRole::Window, None);
    let update = A11yTree::build(&node);
    assert_eq!(update.focus, node_id(100));
    assert_eq!(update.tree.as_ref().map(|t| t.root), Some(node_id(100)));
}

#[test]
fn deep_tree_has_correct_node_count() {
    let leaf_a = simple_node(node_id(202), WidgetRole::Button, Some("A"));
    let leaf_b = simple_node(node_id(203), WidgetRole::Label, Some("B"));

    let mut group = simple_node(node_id(201), WidgetRole::Group, None);
    group.children.push(leaf_a);
    group.children.push(leaf_b);

    let mut root = simple_node(node_id(200), WidgetRole::Window, None);
    root.children.push(group);

    let update = A11yTree::build(&root);
    // root + group + 2 leaves = 4
    assert_eq!(update.nodes.len(), 4);
}

#[test]
fn parent_children_field_references_child_ids() {
    let child_id = node_id(302);
    let child = simple_node(child_id, WidgetRole::Button, None);

    let mut root = simple_node(node_id(301), WidgetRole::Group, None);
    root.children.push(child);

    let update = A11yTree::build(&root);
    // First node is the parent; its children list should contain child_id
    let (_, ref parent_node) = update.nodes[0];
    assert!(parent_node.children().contains(&child_id));
}

// ── Hash-based dirty-flag diff tests ─────────────────────────────────────────

/// A changed label causes the modified node to appear in the delta.
/// The unchanged sibling must NOT appear.
/// The parent (root) must NOT appear because its children-ID list is unchanged
/// — only a child's *content* changed, not the set of child IDs.
#[test]
fn changed_prop_node_appears_in_delta() {
    let mut tree_a = A11yTree::default();
    let mut root_a = simple_node(node_id(400), WidgetRole::Window, None);
    root_a
        .children
        .push(simple_node(node_id(401), WidgetRole::Button, Some("OK")));
    root_a.children.push(simple_node(
        node_id(402),
        WidgetRole::Label,
        Some("Sibling"),
    ));
    tree_a.build_and_store(&root_a);

    let mut tree_b = A11yTree::default();
    let mut root_b = simple_node(node_id(400), WidgetRole::Window, None);
    // Change the label of node 401; same children-ID set as before.
    root_b.children.push(simple_node(
        node_id(401),
        WidgetRole::Button,
        Some("Cancel"),
    ));
    // Sibling is identical
    root_b.children.push(simple_node(
        node_id(402),
        WidgetRole::Label,
        Some("Sibling"),
    ));
    tree_b.build_and_store(&root_b);

    let delta = A11yTree::diff(&tree_a, &tree_b);

    let ids: Vec<NodeId> = delta.nodes.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&node_id(401)), "changed node must be in delta");
    assert!(
        !ids.contains(&node_id(402)),
        "unchanged sibling must NOT be in delta"
    );
    // The root's children-ID list is unchanged (401 and 402 are still the
    // children, only 401's content changed), so the root must NOT appear.
    assert!(
        !ids.contains(&node_id(400)),
        "root must NOT be in delta when only a child's content changed"
    );
}

/// Adding a child produces a delta containing the new child node.
#[test]
fn add_child_delta_contains_new_node() {
    let mut tree_a = A11yTree::default();
    let root_a = simple_node(node_id(500), WidgetRole::Window, None);
    tree_a.build_and_store(&root_a);

    let mut tree_b = A11yTree::default();
    let mut root_b = simple_node(node_id(500), WidgetRole::Window, None);
    root_b
        .children
        .push(simple_node(node_id(501), WidgetRole::Button, Some("New")));
    tree_b.build_and_store(&root_b);

    let delta = A11yTree::diff(&tree_a, &tree_b);

    let ids: Vec<NodeId> = delta.nodes.iter().map(|(id, _)| *id).collect();
    assert!(
        ids.contains(&node_id(501)),
        "new child must appear in delta"
    );
    // Root also changed (child list grew).
    assert!(
        ids.contains(&node_id(500)),
        "parent must appear in delta when child added"
    );
}

/// Removing a child causes the parent to appear in the delta (its child-hash changed).
#[test]
fn remove_child_delta_reflects_parent_update() {
    let mut tree_a = A11yTree::default();
    let mut root_a = simple_node(node_id(600), WidgetRole::Window, None);
    root_a
        .children
        .push(simple_node(node_id(601), WidgetRole::Button, Some("Gone")));
    tree_a.build_and_store(&root_a);

    let mut tree_b = A11yTree::default();
    // root_b has no children — the button was removed.
    let root_b = simple_node(node_id(600), WidgetRole::Window, None);
    tree_b.build_and_store(&root_b);

    let delta = A11yTree::diff(&tree_a, &tree_b);

    let ids: Vec<NodeId> = delta.nodes.iter().map(|(id, _)| *id).collect();
    // The parent must appear because its children list changed.
    assert!(
        ids.contains(&node_id(600)),
        "parent must be in delta when child removed"
    );
    // The removed node itself should NOT appear in the new delta (it's gone).
    assert!(
        !ids.contains(&node_id(601)),
        "removed node must NOT appear in delta"
    );
}

/// Diffing two identical trees must produce an empty delta.
#[test]
fn no_change_produces_empty_delta() {
    let mut tree_a = A11yTree::default();
    let mut root = simple_node(node_id(700), WidgetRole::Window, Some("App"));
    root.children
        .push(simple_node(node_id(701), WidgetRole::Button, Some("Click")));
    tree_a.build_and_store(&root);

    // Rebuild identical tree in tree_b.
    let mut tree_b = A11yTree::default();
    let mut root2 = simple_node(node_id(700), WidgetRole::Window, Some("App"));
    root2
        .children
        .push(simple_node(node_id(701), WidgetRole::Button, Some("Click")));
    tree_b.build_and_store(&root2);

    let delta = A11yTree::diff(&tree_a, &tree_b);
    assert!(
        delta.nodes.is_empty(),
        "identical trees must produce an empty delta, got {:?} nodes",
        delta.nodes.len()
    );
}

// ── NodePool tests ────────────────────────────────────────────────────────────

#[test]
fn pool_active_count_tracks_allocs() {
    let mut pool = NodePool::new();
    assert_eq!(pool.active_count(), 0);
    assert_eq!(pool.free_count(), 0);

    pool.alloc(
        node_id(1),
        simple_node(node_id(1), WidgetRole::Button, None),
    );
    assert_eq!(pool.active_count(), 1);
    assert_eq!(pool.free_count(), 0);

    pool.recycle();
    assert_eq!(pool.active_count(), 0);
    assert_eq!(pool.free_count(), 1);
}

#[test]
fn pool_recycle_moves_to_free_list() {
    let mut pool = NodePool::new();
    for i in 1u64..=5 {
        pool.alloc(node_id(i), simple_node(node_id(i), WidgetRole::Label, None));
    }
    assert_eq!(pool.active_count(), 5);

    pool.recycle();
    assert_eq!(pool.active_count(), 0);
    assert_eq!(pool.free_count(), 5);
}

#[test]
fn pool_clear_resets_everything() {
    let mut pool = NodePool::new();
    pool.alloc(
        node_id(10),
        simple_node(node_id(10), WidgetRole::Group, None),
    );
    pool.recycle();
    assert_eq!(pool.free_count(), 1);

    pool.clear();
    assert_eq!(pool.active_count(), 0);
    assert_eq!(pool.free_count(), 0);
}

#[test]
fn pool_get_returns_active_node() {
    let mut pool = NodePool::new();
    pool.alloc(
        node_id(20),
        simple_node(node_id(20), WidgetRole::Dialog, Some("About")),
    );

    let node = pool.get(&node_id(20));
    assert!(node.is_some());
    assert_eq!(node.and_then(|n| n.label.as_deref()), Some("About"));

    assert!(
        pool.get(&node_id(99)).is_none(),
        "non-existent id returns None"
    );
}

#[test]
fn pool_alloc_recycled_reuses_memory() {
    let mut pool = NodePool::new();
    // Alloc, recycle to free list, then alloc_recycled.
    pool.alloc(
        node_id(30),
        simple_node(node_id(30), WidgetRole::Button, Some("Old")),
    );
    pool.recycle();
    assert_eq!(pool.free_count(), 1);

    pool.alloc_recycled(node_id(31), WidgetRole::Label, Some("New".to_string()));
    // The free list should be drained (one node reused).
    assert_eq!(pool.free_count(), 0);
    assert_eq!(pool.active_count(), 1);

    let node = pool.get(&node_id(31));
    assert!(node.is_some());
    assert_eq!(node.and_then(|n| n.label.as_deref()), Some("New"));
}

// ── Lazy / dirty-flag tests ───────────────────────────────────────────────────

#[test]
fn lazy_computes_once() {
    let mut counter = 0usize;
    let mut lazy: Lazy<usize> = Lazy::new();
    assert!(lazy.is_dirty());

    let v1 = *lazy.get_or_compute(|| {
        counter += 1;
        42
    });
    assert_eq!(v1, 42);
    assert!(!lazy.is_dirty());

    // Second access — closure must NOT run.
    let v2 = *lazy.get_or_compute(|| {
        counter += 1;
        99
    });
    assert_eq!(v2, 42);
    assert_eq!(counter, 1, "compute closure ran more than once");
}

#[test]
fn lazy_recomputes_after_invalidate() {
    let mut counter = 0usize;
    let mut lazy: Lazy<usize> = Lazy::new();

    let _ = lazy.get_or_compute(|| {
        counter += 1;
        42
    });
    lazy.invalidate();
    assert!(lazy.is_dirty());

    let v = *lazy.get_or_compute(|| {
        counter += 1;
        99
    });
    assert_eq!(v, 99);
    assert_eq!(counter, 2);
}

#[test]
fn lazy_set_skips_compute() {
    let mut lazy: Lazy<String> = Lazy::new();
    lazy.set("preset".to_string());
    assert!(!lazy.is_dirty());

    let val = lazy.get_or_compute(|| "should-not-run".to_string());
    assert_eq!(val, "preset");
}

#[test]
fn lazy_get_if_clean_returns_none_when_dirty() {
    let mut lazy: Lazy<i32> = Lazy::new();
    assert!(lazy.get_if_clean().is_none());

    lazy.set(7);
    assert_eq!(lazy.get_if_clean(), Some(&7));

    lazy.invalidate();
    assert!(lazy.get_if_clean().is_none());
}

// ── Content hash stability tests ──────────────────────────────────────────────

#[test]
fn content_hash_same_for_identical_nodes() {
    let a = simple_node(node_id(800), WidgetRole::Button, Some("Hello"));
    let b = simple_node(node_id(800), WidgetRole::Button, Some("Hello"));
    assert_eq!(
        a.content_hash(),
        b.content_hash(),
        "identical nodes must have the same content hash"
    );
}

#[test]
fn content_hash_differs_after_label_change() {
    let a = simple_node(node_id(900), WidgetRole::Button, Some("Before"));
    let b = simple_node(node_id(900), WidgetRole::Button, Some("After"));
    assert_ne!(
        a.content_hash(),
        b.content_hash(),
        "different labels must produce different content hashes"
    );
}

#[test]
fn content_hash_differs_for_different_roles() {
    let a = simple_node(node_id(1000), WidgetRole::Button, Some("X"));
    let b = simple_node(node_id(1000), WidgetRole::Label, Some("X"));
    assert_ne!(
        a.content_hash(),
        b.content_hash(),
        "different roles must produce different content hashes"
    );
}

#[test]
fn content_hash_differs_when_child_added() {
    let mut a = simple_node(node_id(1100), WidgetRole::Group, None);
    let mut b = simple_node(node_id(1100), WidgetRole::Group, None);
    b.children
        .push(simple_node(node_id(1101), WidgetRole::Button, None));
    assert_ne!(
        a.content_hash(),
        b.content_hash(),
        "adding a child must change the parent's content hash"
    );
    // Now add the same child to a — hashes should be equal again.
    a.children
        .push(simple_node(node_id(1101), WidgetRole::Button, None));
    assert_eq!(a.content_hash(), b.content_hash());
}

// ── DirtyTracker tests ────────────────────────────────────────────────────────

#[test]
fn test_dirty_tracker_mark_and_clear() {
    let mut tracker = DirtyTracker::new();
    let win_a = WindowA11yId(1);
    let win_b = WindowA11yId(2);

    tracker.mark_dirty(win_a);
    tracker.mark_dirty(win_b);

    assert!(
        tracker.is_dirty(win_a),
        "win_a must be dirty after mark_dirty"
    );
    assert!(
        tracker.is_dirty(win_b),
        "win_b must be dirty after mark_dirty"
    );

    tracker.clear(win_a);
    assert!(!tracker.is_dirty(win_a), "win_a must be clean after clear");
    assert!(tracker.is_dirty(win_b), "win_b must still be dirty");
}

#[test]
fn test_dirty_tracker_generation() {
    let mut tracker = DirtyTracker::new();
    let gen0 = tracker.generation();
    assert_eq!(gen0, 0, "initial generation must be 0");

    tracker.mark_dirty(WindowA11yId(10));
    assert_eq!(
        tracker.generation(),
        1,
        "generation must increment on mark_dirty"
    );

    tracker.mark_dirty(WindowA11yId(20));
    assert_eq!(
        tracker.generation(),
        2,
        "each mark_dirty bumps the generation"
    );

    // clear() must NOT change the generation.
    tracker.clear(WindowA11yId(10));
    assert_eq!(
        tracker.generation(),
        2,
        "clear must not change the generation"
    );
}

// ── ActionDispatcher tests ────────────────────────────────────────────────────

#[test]
fn test_action_dispatcher_dispatch() {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    let call_count = Arc::new(AtomicU32::new(0));
    let call_count2 = Arc::clone(&call_count);

    let mut dispatcher = ActionDispatcher::new();
    dispatcher.on_action(move |_req| {
        call_count2.fetch_add(1, Ordering::SeqCst);
    });

    let req = ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: NodeId(1),
        data: None,
    };
    dispatcher.dispatch(&req);

    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "handler must have been called exactly once"
    );

    // Dispatch again — handler is called again.
    dispatcher.dispatch(&req);
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        2,
        "handler must be called for each dispatch"
    );
}

// ── Tab-order navigation tests ────────────────────────────────────────────────

fn make_tab_order_3() -> TabOrder {
    let mut root = A11yNode::simple(NodeId(0), WidgetRole::Window, None);
    root.children.push(A11yNode::simple(
        NodeId(1),
        WidgetRole::Button,
        Some("B1".to_string()),
    ));
    root.children.push(A11yNode::simple(
        NodeId(2),
        WidgetRole::Button,
        Some("B2".to_string()),
    ));
    root.children.push(A11yNode::simple(
        NodeId(3),
        WidgetRole::Button,
        Some("B3".to_string()),
    ));
    TabOrder::compute(&root)
}

#[test]
fn test_tab_next_wraps() {
    let order = make_tab_order_3();
    // Next from the last node must wrap to the first.
    let next = tab_next(&order, Some(NodeId(3)));
    assert_eq!(
        next,
        Some(NodeId(1)),
        "tab_next from last must wrap to first"
    );
}

#[test]
fn test_tab_prev_wraps() {
    let order = make_tab_order_3();
    // Prev from the first node must wrap to the last.
    let prev = tab_prev(&order, Some(NodeId(1)));
    assert_eq!(
        prev,
        Some(NodeId(3)),
        "tab_prev from first must wrap to last"
    );
}

#[test]
fn test_tab_next_from_none() {
    let order = make_tab_order_3();
    // tab_next with no current focus returns the first node.
    let next = tab_next(&order, None);
    assert_eq!(
        next,
        Some(NodeId(1)),
        "tab_next from None must return first node"
    );
}
