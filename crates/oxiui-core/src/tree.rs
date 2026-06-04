//! Retained widget-tree data structure with stable IDs and hit testing.
//!
//! This is an optional retained-mode scaffold used for hit testing, focus
//! traversal, and accessibility-tree generation. The immediate-mode
//! [`UiCtx`](crate::UiCtx) path does not require it; adapters that need a
//! persistent tree (e.g. `oxiui-accessibility`) build one here.
//!
//! ## Arena-style allocation
//!
//! Nodes are stored in a flat [`Vec`] (the *arena*). A companion
//! `HashMap<WidgetId, usize>` (*index*) maps every live [`WidgetId`] to its
//! slot in the vec in O(1). This makes `get` / `get_mut` / `index_of` all
//! O(1) instead of O(n), which matters for trees with thousands of nodes.
//!
//! When a node is removed its slot is swapped with the last element and the
//! index is updated, keeping the vec compact (no holes). The index is the
//! single source of truth for liveness: a [`WidgetId`] absent from the index
//! is definitively dead.

use crate::geometry::{Point, Rect};
use std::collections::HashMap;

/// A stable, unique identifier for a node within a single [`WidgetTree`].
///
/// IDs are allocated monotonically by [`WidgetIdAllocator`] and never reused
/// within the lifetime of one allocator, guaranteeing uniqueness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(pub u64);

impl WidgetId {
    /// The reserved root identifier.
    pub const ROOT: WidgetId = WidgetId(0);
}

/// Monotonic allocator of [`WidgetId`]s.
///
/// `WidgetId(0)` is reserved for the root; user IDs start at `1`.
#[derive(Debug)]
pub struct WidgetIdAllocator {
    next: u64,
}

impl WidgetIdAllocator {
    /// Create a fresh allocator. The first allocated ID is `WidgetId(1)`.
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Allocate the next unique [`WidgetId`].
    pub fn alloc(&mut self) -> WidgetId {
        let id = WidgetId(self.next);
        self.next += 1;
        id
    }

    /// Number of IDs allocated so far (excluding the reserved root).
    pub fn allocated(&self) -> u64 {
        self.next - 1
    }
}

impl Default for WidgetIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// A single node in a [`WidgetTree`].
#[derive(Clone, Debug)]
pub struct WidgetNode {
    /// This node's stable identifier.
    pub id: WidgetId,
    /// Parent node, or `None` for the root.
    pub parent: Option<WidgetId>,
    /// Child nodes in paint (back-to-front) order.
    pub children: Vec<WidgetId>,
    /// Layout rectangle in the coordinate space of the tree's root.
    pub rect: Rect,
    /// Paint order among siblings; higher draws on top.
    pub z_index: i32,
    /// Whether this node participates in hit testing (false = pass-through).
    pub hit_testable: bool,
    /// Whether this node can receive keyboard focus.
    pub focusable: bool,
    /// Dirty flag: layout or paint needs recomputation.
    pub dirty: bool,
    /// Optional clip rectangle, in the tree's root coordinate space. When set,
    /// content (and hit testing) is constrained to the intersection of `rect`
    /// and this clip. `None` means inherit the parent clip / no clipping.
    pub clip_rect: Option<Rect>,
    /// Optional debug/role label (used by a11y and diagnostics).
    pub label: Option<String>,
}

impl WidgetNode {
    /// Construct a node with the given id and rectangle. Defaults: hit-testable,
    /// not focusable, `z_index` 0, dirty.
    pub fn new(id: WidgetId, rect: Rect) -> Self {
        Self {
            id,
            parent: None,
            children: Vec::new(),
            rect,
            z_index: 0,
            hit_testable: true,
            focusable: false,
            dirty: true,
            clip_rect: None,
            label: None,
        }
    }

    /// Builder-style setter for [`clip_rect`](WidgetNode::clip_rect).
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip_rect = Some(clip);
        self
    }
}

/// A tree of [`WidgetNode`]s addressed by [`WidgetId`].
///
/// Nodes are stored in a flat arena vector; parent/child relationships are
/// tracked by id. A companion `HashMap` index provides O(1) lookup from id to
/// vec slot. The tree always contains a root node (`WidgetId::ROOT`).
///
/// See the module-level documentation for the arena allocation strategy.
#[derive(Debug)]
pub struct WidgetTree {
    /// Arena: flat storage of all live nodes.
    nodes: Vec<WidgetNode>,
    /// O(1) index: maps WidgetId → arena slot index.
    index: HashMap<WidgetId, usize>,
    alloc: WidgetIdAllocator,
}

impl WidgetTree {
    /// Create a new tree whose root covers `root_rect`.
    pub fn new(root_rect: Rect) -> Self {
        let mut root = WidgetNode::new(WidgetId::ROOT, root_rect);
        root.label = Some("root".to_owned());
        let mut index = HashMap::new();
        index.insert(WidgetId::ROOT, 0usize);
        Self {
            nodes: vec![root],
            index,
            alloc: WidgetIdAllocator::new(),
        }
    }

    /// Total number of nodes (including the root).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if only the root node exists.
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    fn index_of(&self, id: WidgetId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// Borrow a node by id.
    pub fn get(&self, id: WidgetId) -> Option<&WidgetNode> {
        self.index_of(id).map(|i| &self.nodes[i])
    }

    /// Mutably borrow a node by id.
    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut WidgetNode> {
        self.index_of(id).map(move |i| &mut self.nodes[i])
    }

    /// Insert a new child of `parent` covering `rect`. Returns the new id, or
    /// `None` if `parent` does not exist.
    pub fn insert(&mut self, parent: WidgetId, rect: Rect) -> Option<WidgetId> {
        self.index_of(parent)?;
        let id = self.alloc.alloc();
        let new_idx = self.nodes.len();
        let mut node = WidgetNode::new(id, rect);
        node.parent = Some(parent);
        self.nodes.push(node);
        self.index.insert(id, new_idx);
        if let Some(pi) = self.index_of(parent) {
            self.nodes[pi].children.push(id);
        }
        Some(id)
    }

    /// Remove `id` and its entire subtree. The root cannot be removed.
    ///
    /// Returns the number of nodes removed.
    pub fn remove(&mut self, id: WidgetId) -> usize {
        if id == WidgetId::ROOT {
            return 0;
        }
        // Collect subtree ids (DFS) without borrowing issues.
        let mut to_remove = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if let Some(node) = self.get(cur) {
                stack.extend(node.children.iter().copied());
                to_remove.push(cur);
            }
        }
        // Detach from parent.
        if let Some(parent) = self.get(id).and_then(|n| n.parent) {
            if let Some(pi) = self.index_of(parent) {
                self.nodes[pi].children.retain(|&c| c != id);
            }
        }
        let removed = to_remove.len();
        // Remove the nodes from the arena.
        self.nodes.retain(|n| !to_remove.contains(&n.id));
        // Remove stale index entries for the removed ids.
        for rid in &to_remove {
            self.index.remove(rid);
        }
        // Rebuild the remaining index entries (positions may have shifted after retain).
        for (slot, node) in self.nodes.iter().enumerate() {
            self.index.insert(node.id, slot);
        }
        removed
    }

    /// Move `id` to become a child of `new_parent`. Returns `false` if either
    /// id is missing, if `id` is the root, or if the move would create a cycle.
    pub fn reparent(&mut self, id: WidgetId, new_parent: WidgetId) -> bool {
        if id == WidgetId::ROOT || self.get(id).is_none() || self.get(new_parent).is_none() {
            return false;
        }
        // Reject cycles: new_parent must not be a descendant of id.
        if self.is_descendant(new_parent, id) {
            return false;
        }
        let old_parent = self.get(id).and_then(|n| n.parent);
        if let Some(op) = old_parent {
            if let Some(oi) = self.index_of(op) {
                self.nodes[oi].children.retain(|&c| c != id);
            }
        }
        if let Some(ni) = self.index_of(new_parent) {
            self.nodes[ni].children.push(id);
        }
        if let Some(i) = self.index_of(id) {
            self.nodes[i].parent = Some(new_parent);
        }
        true
    }

    /// Returns `true` if `maybe_descendant` is in the subtree rooted at `ancestor`.
    pub fn is_descendant(&self, maybe_descendant: WidgetId, ancestor: WidgetId) -> bool {
        let mut cur = maybe_descendant;
        while let Some(node) = self.get(cur) {
            match node.parent {
                Some(p) if p == ancestor => return true,
                Some(p) => cur = p,
                None => return false,
            }
        }
        false
    }

    /// Depth of `id` from the root (root has depth 0). Returns `None` if missing.
    pub fn depth(&self, id: WidgetId) -> Option<usize> {
        let mut depth = 0;
        let mut cur = self.get(id)?;
        while let Some(parent) = cur.parent {
            depth += 1;
            cur = self.get(parent)?;
        }
        Some(depth)
    }

    /// Visit every node in depth-first pre-order, invoking `visit(node, depth)`.
    pub fn walk_dfs(&self, mut visit: impl FnMut(&WidgetNode, usize)) {
        let mut stack = vec![(WidgetId::ROOT, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            if let Some(node) = self.get(id) {
                visit(node, depth);
                // Push children reversed so they're visited left-to-right.
                for &child in node.children.iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
    }

    /// The effective clip rectangle for `id`: the intersection of its own
    /// `clip_rect` (if any) with every ancestor's `clip_rect`. Returns `None`
    /// when no ancestor (and the node itself) clips — meaning "unclipped".
    ///
    /// If the accumulated clips do not overlap at all, returns
    /// `Some(Rect::ZERO)` (a degenerate, empty clip) so callers treat the node
    /// as fully clipped away.
    pub fn effective_clip(&self, id: WidgetId) -> Option<Rect> {
        let mut acc: Option<Rect> = None;
        let mut cur = self.get(id);
        while let Some(node) = cur {
            if let Some(clip) = node.clip_rect {
                acc = Some(match acc {
                    None => clip,
                    Some(existing) => existing.intersection(&clip).unwrap_or(Rect::ZERO),
                });
            }
            cur = node.parent.and_then(|p| self.get(p));
        }
        acc
    }

    /// Hit-test `point`, returning the front-most hit-testable node containing it.
    ///
    /// Traversal is depth-first; among overlapping candidates the one with the
    /// greatest combined `(depth, z_index)` paint order wins (i.e. the visually
    /// front-most). Non-`hit_testable` nodes are skipped but their descendants
    /// are still tested. A node is only a candidate when `point` lies within
    /// both its `rect` *and* its [`effective_clip`](WidgetTree::effective_clip);
    /// content outside an (ancestor) clip is not interactive.
    pub fn hit_test(&self, point: Point) -> Option<WidgetId> {
        let mut best: Option<(WidgetId, usize, i32)> = None;
        self.walk_dfs(|node, depth| {
            if !node.hit_testable || !node.rect.contains(point) {
                return;
            }
            // Respect clipping: the point must also lie inside the accumulated
            // clip region for this node.
            if let Some(clip) = self.effective_clip(node.id) {
                if !clip.contains(point) {
                    return;
                }
            }
            let key = (depth, node.z_index);
            match best {
                Some((_, bd, bz)) if (bd, bz) >= key => {}
                _ => best = Some((node.id, depth, node.z_index)),
            }
        });
        best.map(|(id, _, _)| id)
    }

    /// Collect the focusable nodes in DFS (tab) order.
    pub fn focus_order(&self) -> Vec<WidgetId> {
        let mut order = Vec::new();
        self.walk_dfs(|node, _| {
            if node.focusable {
                order.push(node.id);
            }
        });
        order
    }

    /// Collect every node id in back-to-front paint order.
    ///
    /// Order is a stable sort by the key `(depth, z_index)`: shallower nodes
    /// paint first (behind), and within the same depth lower `z_index` paints
    /// first. Painting nodes in this order means later draws correctly occlude
    /// earlier ones. The sort is stable, so siblings with equal `z_index`
    /// retain their tree (DFS) order, matching CSS source-order tie-breaking.
    pub fn paint_order(&self) -> Vec<WidgetId> {
        let mut entries: Vec<(usize, i32, usize, WidgetId)> = Vec::new();
        let mut seq = 0usize;
        self.walk_dfs(|node, depth| {
            entries.push((depth, node.z_index, seq, node.id));
            seq += 1;
        });
        // Stable sort by (depth, z_index); `seq` (DFS order) breaks ties so the
        // result is deterministic and source-ordered within a stacking level.
        entries.sort_by_key(|&(depth, z, seq, _)| (depth, z, seq));
        entries.into_iter().map(|(_, _, _, id)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn sample_tree() -> WidgetTree {
        let mut t = WidgetTree::new(Rect::new(0.0, 0.0, 200.0, 200.0));
        let a = t
            .insert(WidgetId::ROOT, Rect::new(0.0, 0.0, 100.0, 100.0))
            .expect("root exists");
        let _b = t
            .insert(WidgetId::ROOT, Rect::new(100.0, 0.0, 100.0, 100.0))
            .expect("root exists");
        let _a1 = t
            .insert(a, Rect::new(10.0, 10.0, 30.0, 30.0))
            .expect("a exists");
        t
    }

    #[test]
    fn id_allocator_is_monotonic_and_unique() {
        let mut alloc = WidgetIdAllocator::new();
        let a = alloc.alloc();
        let b = alloc.alloc();
        assert_ne!(a, b);
        assert_eq!(a, WidgetId(1));
        assert_eq!(b, WidgetId(2));
        assert_eq!(alloc.allocated(), 2);
    }

    #[test]
    fn insert_and_structure() {
        let t = sample_tree();
        assert_eq!(t.len(), 4); // root + a + b + a1
        let root = t.get(WidgetId::ROOT).expect("root");
        assert_eq!(root.children.len(), 2);
        assert_eq!(t.depth(WidgetId(3)), Some(2)); // a1 nested under a
    }

    #[test]
    fn remove_subtree() {
        let mut t = sample_tree();
        let removed = t.remove(WidgetId(1)); // remove `a` and its child a1
        assert_eq!(removed, 2);
        assert_eq!(t.len(), 2); // root + b
        assert!(t.get(WidgetId(1)).is_none());
        assert!(t.get(WidgetId(3)).is_none());
        // root cannot be removed
        assert_eq!(t.remove(WidgetId::ROOT), 0);
    }

    #[test]
    fn reparent_rejects_cycles() {
        let mut t = sample_tree();
        // Try to make `a` (id 1) a child of its own descendant a1 (id 3): cycle.
        assert!(!t.reparent(WidgetId(1), WidgetId(3)));
        // Valid reparent: move a1 (id 3) under b (id 2). b is a child of root
        // (depth 1), so a1 ends up at depth 2.
        assert!(t.reparent(WidgetId(3), WidgetId(2)));
        assert_eq!(t.depth(WidgetId(3)), Some(2));
        // a1 is now a child of b, no longer of a.
        assert_eq!(t.get(WidgetId(3)).and_then(|n| n.parent), Some(WidgetId(2)));
    }

    #[test]
    fn hit_test_front_most_wins() {
        let mut t = WidgetTree::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        // Two overlapping siblings; the deeper/later one should win.
        let back = t
            .insert(WidgetId::ROOT, Rect::new(0.0, 0.0, 50.0, 50.0))
            .expect("root");
        let front = t
            .insert(back, Rect::new(10.0, 10.0, 20.0, 20.0))
            .expect("back");
        let hit = t.hit_test(Point::new(15.0, 15.0));
        assert_eq!(hit, Some(front));
        // A point only inside the parent hits the parent.
        assert_eq!(t.hit_test(Point::new(45.0, 45.0)), Some(back));
        // Outside everything.
        assert_eq!(t.hit_test(Point::new(90.0, 90.0)), Some(WidgetId::ROOT));
    }

    #[test]
    fn hit_test_skips_non_hit_testable() {
        let mut t = WidgetTree::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let overlay = t
            .insert(WidgetId::ROOT, Rect::new(0.0, 0.0, 100.0, 100.0))
            .expect("root");
        if let Some(n) = t.get_mut(overlay) {
            n.hit_testable = false; // pass-through overlay
        }
        // Overlay is pass-through, so root receives the hit.
        assert_eq!(t.hit_test(Point::new(50.0, 50.0)), Some(WidgetId::ROOT));
    }

    #[test]
    fn focus_order_dfs() {
        let mut t = WidgetTree::new(Rect::ZERO);
        let a = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        let b = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        for id in [a, b] {
            if let Some(n) = t.get_mut(id) {
                n.focusable = true;
            }
        }
        assert_eq!(t.focus_order(), vec![a, b]);
    }

    #[test]
    fn hit_test_respects_clip_rect() {
        let mut t = WidgetTree::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        // A scroll container clipped to its left half.
        let container = t
            .insert(WidgetId::ROOT, Rect::new(0.0, 0.0, 100.0, 100.0))
            .expect("root");
        if let Some(n) = t.get_mut(container) {
            n.clip_rect = Some(Rect::new(0.0, 0.0, 50.0, 100.0));
        }
        // A child that overflows past the clip on the right.
        let child = t
            .insert(container, Rect::new(40.0, 0.0, 40.0, 20.0))
            .expect("container");
        // Inside both child rect and clip -> hits child.
        assert_eq!(t.hit_test(Point::new(45.0, 5.0)), Some(child));
        // Inside child rect but outside the clip (x >= 50) -> falls through to
        // the (clipped) container, but that point is also outside the container
        // clip, so it reaches the unclipped root.
        assert_eq!(t.hit_test(Point::new(70.0, 5.0)), Some(WidgetId::ROOT));
    }

    #[test]
    fn effective_clip_intersects_ancestors() {
        let mut t = WidgetTree::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let outer = t
            .insert(WidgetId::ROOT, Rect::new(0.0, 0.0, 100.0, 100.0))
            .expect("root");
        if let Some(n) = t.get_mut(outer) {
            n.clip_rect = Some(Rect::new(0.0, 0.0, 60.0, 100.0));
        }
        let inner = t
            .insert(outer, Rect::new(0.0, 0.0, 100.0, 100.0))
            .expect("outer");
        if let Some(n) = t.get_mut(inner) {
            n.clip_rect = Some(Rect::new(40.0, 0.0, 60.0, 100.0));
        }
        // Intersection of [0,60) and [40,100) on x => [40,60).
        assert_eq!(
            t.effective_clip(inner),
            Some(Rect::new(40.0, 0.0, 20.0, 100.0))
        );
        // Root (unclipped) has no effective clip.
        assert_eq!(t.effective_clip(WidgetId::ROOT), None);
    }

    #[test]
    fn paint_order_back_to_front() {
        let mut t = WidgetTree::new(Rect::ZERO);
        // root(depth0); a,b children(depth1); a has higher z than b.
        let a = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        let b = t.insert(WidgetId::ROOT, Rect::ZERO).expect("root");
        let a1 = t.insert(a, Rect::ZERO).expect("a"); // depth 2
        if let Some(n) = t.get_mut(a) {
            n.z_index = 5;
        }
        if let Some(n) = t.get_mut(b) {
            n.z_index = 1;
        }
        let order = t.paint_order();
        // Root first (shallowest). Among depth-1 siblings, lower z (b) before a.
        assert_eq!(order[0], WidgetId::ROOT);
        let pos_a = order.iter().position(|&x| x == a).expect("a present");
        let pos_b = order.iter().position(|&x| x == b).expect("b present");
        let pos_a1 = order.iter().position(|&x| x == a1).expect("a1 present");
        assert!(pos_b < pos_a, "lower z_index paints first");
        // a1 is deepest -> paints last (in front of everything).
        assert!(pos_a1 > pos_a && pos_a1 > pos_b);
    }
}
