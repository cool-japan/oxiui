//! Widget-tree diffing: turn an old tree into a new one with a minimal op set.
//!
//! Given two [`WidgetTree`]s whose nodes carry stable [`WidgetId`]s, [`diff`]
//! produces an ordered [`Vec<DiffOp>`] of `Insert` / `Remove` / `Update` /
//! `Move` operations that, applied in order, transform the old tree's structure
//! into the new one. This is the reconciliation step a retained backend runs to
//! avoid rebuilding unchanged subtrees.
//!
//! ## Algorithm
//!
//! Diffing is per-node: for each id present in both trees we compare the child
//! id lists and compute a **longest common subsequence (LCS)**. Children in the
//! LCS keep their relative order for free; children present in both lists but
//! *not* in the LCS are emitted as `Move`s; children only in the new list are
//! `Insert`s; children only in the old list are `Remove`s. A node whose own
//! paint-relevant fields changed is emitted as an `Update`. Using the LCS keeps
//! the move set minimal (you never move a node that didn't need moving), which
//! is the property that makes keyed list diffing efficient and stable.

use crate::tree::{WidgetId, WidgetNode, WidgetTree};
use std::collections::HashSet;

/// A single reconciliation operation produced by [`diff`].
#[derive(Clone, Debug, PartialEq)]
pub enum DiffOp {
    /// A new node `id` was added as a child of `parent` at child-list index
    /// `index`.
    Insert {
        /// The node being inserted.
        id: WidgetId,
        /// The parent it is inserted under.
        parent: WidgetId,
        /// The position within `parent`'s child list.
        index: usize,
    },
    /// Node `id` (and its subtree) was removed.
    Remove {
        /// The node being removed.
        id: WidgetId,
    },
    /// Node `id` persists but one or more of its paint-relevant fields changed.
    Update {
        /// The node whose fields changed.
        id: WidgetId,
    },
    /// Node `id` kept its parent but moved to a new index within the child list.
    Move {
        /// The node being moved.
        id: WidgetId,
        /// Its parent (unchanged).
        parent: WidgetId,
        /// The new position within `parent`'s child list.
        index: usize,
    },
}

/// Compute the ordered diff that turns `old` into `new`.
///
/// Both trees are assumed to share an id space (ids are stable across frames).
/// Removals are emitted before inserts/moves so a backend can free first.
pub fn diff(old: &WidgetTree, new: &WidgetTree) -> Vec<DiffOp> {
    let mut ops = Vec::new();

    let old_ids: HashSet<WidgetId> = collect_ids(old);
    let new_ids: HashSet<WidgetId> = collect_ids(new);

    // ── Removals: ids in old but not in new (top-most only; a removed subtree
    // is a single Remove of its root). ──────────────────────────────────────
    for &id in old_ids.iter() {
        if !new_ids.contains(&id) {
            // Skip if an ancestor is also being removed (avoid redundant ops).
            let ancestor_removed = ancestor_chain(old, id)
                .into_iter()
                .any(|anc| anc != id && !new_ids.contains(&anc));
            if !ancestor_removed {
                ops.push(DiffOp::Remove { id });
            }
        }
    }

    // ── Per-shared-node: child reconciliation + field updates. ───────────────
    // Visit in DFS order of `new` so inserts happen parent-before-child.
    let mut visit_order = Vec::new();
    new.walk_dfs(|node, _| visit_order.push(node.id));

    for id in visit_order {
        let new_node = match new.get(id) {
            Some(n) => n,
            None => continue,
        };

        match old.get(id) {
            // Node exists in both: reconcile children + maybe update fields.
            Some(old_node) => {
                if node_changed(old_node, new_node) {
                    ops.push(DiffOp::Update { id });
                }
                reconcile_children(
                    id,
                    &old_node.children,
                    &new_node.children,
                    &old_ids,
                    &mut ops,
                );
            }
            // Node is new but its *parent* already existed (root-level new
            // subtrees are handled by their parent's reconcile). If the parent
            // is also new, the parent's reconcile emits this insert; skip here.
            None => {
                // Inserts are emitted by the parent's reconcile pass; nothing to
                // do at the node itself.
            }
        }
    }

    ops
}

/// Reconcile one node's child list using an LCS, appending ops.
fn reconcile_children(
    parent: WidgetId,
    old_children: &[WidgetId],
    new_children: &[WidgetId],
    old_ids: &HashSet<WidgetId>,
    ops: &mut Vec<DiffOp>,
) {
    // The set of new children that already existed somewhere in the old tree.
    // Children that are brand-new ids are pure inserts; the rest are candidates
    // for "kept in place" (LCS) or "moved".
    let lcs = longest_common_subsequence(old_children, new_children);
    let kept: HashSet<WidgetId> = lcs.iter().copied().collect();

    for (index, &child) in new_children.iter().enumerate() {
        if kept.contains(&child) {
            // In the LCS: stays put, no op.
            continue;
        }
        if old_ids.contains(&child) {
            // Existed before but not part of the stable subsequence → moved.
            ops.push(DiffOp::Move {
                id: child,
                parent,
                index,
            });
        } else {
            // Never seen before → inserted.
            ops.push(DiffOp::Insert {
                id: child,
                parent,
                index,
            });
        }
    }
}

/// Whether two versions of the same node differ in any paint-relevant field.
/// Child-list changes are handled separately by [`reconcile_children`], so they
/// are deliberately *not* compared here.
fn node_changed(old: &WidgetNode, new: &WidgetNode) -> bool {
    old.rect != new.rect
        || old.z_index != new.z_index
        || old.hit_testable != new.hit_testable
        || old.focusable != new.focusable
        || old.clip_rect != new.clip_rect
        || old.label != new.label
}

/// Collect every id in a tree.
fn collect_ids(tree: &WidgetTree) -> HashSet<WidgetId> {
    let mut ids = HashSet::new();
    tree.walk_dfs(|node, _| {
        ids.insert(node.id);
    });
    ids
}

/// The chain of ids from `id` up to (and including) the root.
fn ancestor_chain(tree: &WidgetTree, id: WidgetId) -> Vec<WidgetId> {
    let mut chain = Vec::new();
    let mut cur = tree.get(id);
    while let Some(node) = cur {
        chain.push(node.id);
        cur = node.parent.and_then(|p| tree.get(p));
    }
    chain
}

/// Compute the longest common subsequence of two id sequences.
///
/// Standard O(n·m) dynamic-programming LCS over `WidgetId`. Returns the
/// subsequence itself (in order). Used to identify children that can stay in
/// place without a move.
fn longest_common_subsequence(a: &[WidgetId], b: &[WidgetId]) -> Vec<WidgetId> {
    let n = a.len();
    let m = b.len();
    if n == 0 || m == 0 {
        return Vec::new();
    }
    // dp[i][j] = LCS length of a[i..] and b[j..]; (n+1)*(m+1) table.
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    // Reconstruct.
    let mut result = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            result.push(a[i]);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn r(x: f32) -> Rect {
        Rect::new(x, 0.0, 10.0, 10.0)
    }

    #[test]
    fn lcs_basic() {
        let a = [WidgetId(1), WidgetId(2), WidgetId(3), WidgetId(4)];
        let b = [WidgetId(2), WidgetId(4), WidgetId(1), WidgetId(3)];
        let lcs = longest_common_subsequence(&a, &b);
        // A valid LCS of these is [1,3] or [2,4] — length 2 either way.
        assert_eq!(lcs.len(), 2);
    }

    #[test]
    fn identical_trees_yield_no_ops() {
        let mut old = WidgetTree::new(r(0.0));
        let a = old.insert(WidgetId::ROOT, r(1.0)).expect("root");
        let _b = old.insert(WidgetId::ROOT, r(2.0)).expect("root");
        let _c = old.insert(a, r(3.0)).expect("a");

        // Build an identical tree (same ids because allocation order matches).
        let mut new = WidgetTree::new(r(0.0));
        let a2 = new.insert(WidgetId::ROOT, r(1.0)).expect("root");
        let _b2 = new.insert(WidgetId::ROOT, r(2.0)).expect("root");
        let _c2 = new.insert(a2, r(3.0)).expect("a");

        let ops = diff(&old, &new);
        assert!(
            ops.is_empty(),
            "identical trees should diff to nothing: {ops:?}"
        );
    }

    #[test]
    fn detects_field_update() {
        let mut old = WidgetTree::new(r(0.0));
        let a = old.insert(WidgetId::ROOT, r(1.0)).expect("root");

        let mut new = WidgetTree::new(r(0.0));
        let a2 = new.insert(WidgetId::ROOT, r(1.0)).expect("root");
        if let Some(n) = new.get_mut(a2) {
            n.rect = r(99.0); // moved rect → Update
        }
        assert_eq!(a, a2);

        let ops = diff(&old, &new);
        assert!(
            ops.contains(&DiffOp::Update { id: a }),
            "expected Update, got {ops:?}"
        );
    }

    #[test]
    fn detects_insert_and_remove() {
        // old: root → [a]
        let mut old = WidgetTree::new(r(0.0));
        let a = old.insert(WidgetId::ROOT, r(1.0)).expect("root");

        // new: root → [a, b]  (b is id 2, which never existed in old)
        let mut new = WidgetTree::new(r(0.0));
        let a2 = new.insert(WidgetId::ROOT, r(1.0)).expect("root");
        let b = new.insert(WidgetId::ROOT, r(2.0)).expect("root");
        assert_eq!(a, a2);

        let ops = diff(&old, &new);
        assert!(
            ops.iter()
                .any(|o| matches!(o, DiffOp::Insert { id, parent, index }
                if *id == b && *parent == WidgetId::ROOT && *index == 1)),
            "expected Insert of b at index 1, got {ops:?}"
        );

        // Reverse direction: removing b.
        let ops_rev = diff(&new, &old);
        assert!(
            ops_rev.contains(&DiffOp::Remove { id: b }),
            "expected Remove of b, got {ops_rev:?}"
        );
    }

    #[test]
    fn removed_subtree_emits_single_remove() {
        // old: root → a → a1 ; remove a (and implicitly a1).
        let mut old = WidgetTree::new(r(0.0));
        let a = old.insert(WidgetId::ROOT, r(1.0)).expect("root");
        let a1 = old.insert(a, r(2.0)).expect("a");

        let new = WidgetTree::new(r(0.0)); // empty but root

        let ops = diff(&old, &new);
        // Only the subtree root `a` is removed; `a1` is not separately removed.
        assert!(ops.contains(&DiffOp::Remove { id: a }), "got {ops:?}");
        assert!(
            !ops.contains(&DiffOp::Remove { id: a1 }),
            "child should not get its own Remove: {ops:?}"
        );
    }

    #[test]
    fn reorder_emits_minimal_moves() {
        // Build old with children [c1, c2, c3] under root.
        let mut old = WidgetTree::new(r(0.0));
        let c1 = old.insert(WidgetId::ROOT, r(1.0)).expect("root");
        let c2 = old.insert(WidgetId::ROOT, r(2.0)).expect("root");
        let c3 = old.insert(WidgetId::ROOT, r(3.0)).expect("root");

        // New tree with the SAME ids but reordered to [c3, c1, c2]. Construct by
        // inserting then reordering the root child vector directly.
        let mut new = WidgetTree::new(r(0.0));
        let _ = new.insert(WidgetId::ROOT, r(1.0)).expect("root"); // c1
        let _ = new.insert(WidgetId::ROOT, r(2.0)).expect("root"); // c2
        let _ = new.insert(WidgetId::ROOT, r(3.0)).expect("root"); // c3
        if let Some(root) = new.get_mut(WidgetId::ROOT) {
            root.children = vec![c3, c1, c2];
        }

        let ops = diff(&old, &new);
        // LCS of [c1,c2,c3] and [c3,c1,c2] is [c1,c2] (len 2); only c3 moves.
        let moves: Vec<_> = ops
            .iter()
            .filter(|o| matches!(o, DiffOp::Move { .. }))
            .collect();
        assert_eq!(moves.len(), 1, "exactly one move expected, got {ops:?}");
        assert!(
            matches!(moves[0], DiffOp::Move { id, index, .. } if *id == c3 && *index == 0),
            "c3 should move to index 0, got {:?}",
            moves[0]
        );
    }
}
