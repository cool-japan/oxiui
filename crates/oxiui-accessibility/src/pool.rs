//! Node pool for allocation-friendly accessibility tree construction.
//!
//! [`NodePool`] maintains a set of active [`A11yNode`] allocations keyed by
//! [`accesskit::NodeId`] and a free list of previously-used nodes whose memory
//! can be reused in the next frame.
//!
//! # Usage pattern
//!
//! Each UI frame:
//! 1. Call [`NodePool::recycle`] to move all active nodes back to the free list.
//! 2. For each widget, call [`NodePool::alloc`] (or [`NodePool::alloc_recycled`])
//!    to place a fresh / reused [`A11yNode`] into the active map.
//! 3. Pass the active nodes to the tree builder as usual.
//!
//! The pool does **not** interact with [`crate::tree::A11yTree`] directly; it is a helper
//! for callers that build the `A11yNode` graph themselves.

use std::collections::HashMap;

use accesskit::NodeId;

use crate::tree::{A11yNode, WidgetRole};

// ── NodePool ─────────────────────────────────────────────────────────────────

/// A reusable pool of [`A11yNode`] allocations, keyed by [`NodeId`].
///
/// Reduces per-frame heap allocations when the accessibility tree is rebuilt
/// on every frame: rather than dropping and re-allocating every node, the pool
/// keeps previously-allocated structs (and their internal `Vec` allocations)
/// on a free list.
///
/// # Invariants
///
/// * A node is either *active* (in `active`) or *free* (in `free_list`), never
///   both simultaneously.
/// * After [`recycle`](NodePool::recycle), `active` is empty and `free_list`
///   holds all previously-active nodes.
/// * After [`clear`](NodePool::clear), both maps are empty.
#[derive(Debug, Default)]
pub struct NodePool {
    /// Nodes currently in use, indexed by their [`NodeId`].
    active: HashMap<NodeId, A11yNode>,
    /// Nodes available for reuse from a previous frame.
    free_list: Vec<A11yNode>,
}

impl NodePool {
    /// Create a new, empty pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// Place `node` into the active map under `id`.
    ///
    /// If a node with the same `id` already exists it is silently replaced and
    /// the old node is dropped (not returned to the free list, to avoid a
    /// same-id duplicate).
    pub fn alloc(&mut self, id: NodeId, node: A11yNode) {
        self.active.insert(id, node);
    }

    /// Allocate a node slot, optionally reusing memory from the free list.
    ///
    /// If a free node is available its struct is taken from the free list and
    /// reset to the supplied `id`, `role`, and `label` before being placed in
    /// the active map.  Otherwise a fresh node is created.
    ///
    /// This is the preferred allocation path for hot paths that care about
    /// minimising heap allocation churn.
    pub fn alloc_recycled(
        &mut self,
        id: NodeId,
        role: WidgetRole,
        label: Option<String>,
    ) -> &mut A11yNode {
        let mut node = match self.free_list.pop() {
            Some(mut recycled) => {
                // Reset to a clean state, reusing the heap-allocated `children` Vec.
                recycled.id = id;
                recycled.role = role;
                recycled.label = label;
                recycled.children.clear();
                recycled.props = crate::props::A11yNodeProps::default();
                recycled.text_content = None;
                recycled
            }
            None => A11yNode::simple(id, role, label),
        };
        // Ensure the id is correct even if the recycled node carried a different one.
        node.id = id;
        self.active.insert(id, node);
        // Safety: we just inserted the value; the entry must exist.
        self.active
            .get_mut(&id)
            .unwrap_or_else(|| unreachable!("just inserted"))
    }

    /// Retrieve an active node by its [`NodeId`].
    pub fn get(&self, id: &NodeId) -> Option<&A11yNode> {
        self.active.get(id)
    }

    /// Retrieve a mutable reference to an active node.
    pub fn get_mut(&mut self, id: &NodeId) -> Option<&mut A11yNode> {
        self.active.get_mut(id)
    }

    /// Move all active nodes back to the free list for reuse next frame.
    ///
    /// After this call `active_count() == 0` and `free_count()` reflects the
    /// total number of recycled nodes.
    pub fn recycle(&mut self) {
        for (_, node) in self.active.drain() {
            self.free_list.push(node);
        }
    }

    /// Number of currently-active nodes.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Number of nodes available for reuse.
    pub fn free_count(&self) -> usize {
        self.free_list.len()
    }

    /// Discard all nodes (active and free).
    pub fn clear(&mut self) {
        self.active.clear();
        self.free_list.clear();
    }

    /// Iterate over all active nodes.
    pub fn iter_active(&self) -> impl Iterator<Item = (&NodeId, &A11yNode)> {
        self.active.iter()
    }
}
