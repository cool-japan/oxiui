//! Reactive primitives: [`Signal<T>`] and [`Computed<T>`] with automatic
//! dependency tracking, topological dirty propagation, and cycle detection.
//!
//! All types are `Send + Sync` — the runtime stores node state behind an
//! `Arc<RwLock<RuntimeInner>>` and runs computed thunks **without** holding the
//! lock (critical deadlock prevention).
//!
//! # Design
//!
//! - **[`ReactiveRuntime`]** — the shared graph owner.  Clone freely; it is
//!   `Arc`-backed.
//! - **[`Signal<T>`]** — a settable cell.  `set()` marks transitive dependents
//!   dirty via BFS over the dep graph.
//! - **[`Computed<T>`]** — lazily evaluated, cached derived value.  On `get()`
//!   the thunk is run **outside** any lock; dependency edges are registered by
//!   reads that occur during the thunk.
//! - **Thread-local stack** — `COMPUTATION_STACK` tracks which computed node
//!   is currently evaluating.  Any `get()` call on a signal or computed while
//!   the stack is non-empty registers a dep edge (source → caller).
//! - **Cycle detection** — two layers: (1) runtime: stack contains self →
//!   `Err(Cycle)`; (2) graph: DFS before inserting an edge →
//!   `Err(DependencyCycle)`.
//!
//! # Example
//! ```no_run
//! use oxiui_core::reactive::ReactiveRuntime;
//!
//! let rt = ReactiveRuntime::new();
//! let count = rt.signal(0i32);
//! let doubled = rt.computed({
//!     let c = count.clone();
//!     move || c.get() * 2
//! }).expect("no cycle");
//!
//! count.set(5);
//! assert_eq!(doubled.get(), Ok(10));
//! ```

use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    marker::PhantomData,
    sync::{Arc, RwLock},
};

// ─── Type aliases ────────────────────────────────────────────────────────────

/// A boxed type-erased value that is `Send + Sync`.
type AnyValue = Box<dyn Any + Send + Sync>;

/// A cloneable computed thunk: no arguments, returns a type-erased value.
///
/// We use `Arc` so the thunk can be cloned out of the `RwLock` guard before
/// being called (holding the lock across user code would deadlock).
type ArcThunk = Arc<dyn Fn() -> AnyValue + Send + Sync>;

// ─── Thread-local computation stack ─────────────────────────────────────────

thread_local! {
    /// The stack of [`NodeId`]s currently being evaluated.
    ///
    /// The top of the stack is the computed node whose thunk is executing now.
    /// Any `get()` call on a node while the stack is non-empty registers the
    /// reading node (stack top) as a dependent of the node being read.
    static COMPUTATION_STACK: RefCell<Vec<NodeId>> = const { RefCell::new(Vec::new()) };
}

/// Push `id` onto the thread-local computation stack.
fn stack_push(id: NodeId) {
    COMPUTATION_STACK.with(|s| s.borrow_mut().push(id));
}

/// Pop the top entry from the thread-local computation stack.
fn stack_pop() {
    COMPUTATION_STACK.with(|s| {
        s.borrow_mut().pop();
    });
}

/// Return the node currently being evaluated (the top of the stack), if any.
fn stack_top() -> Option<NodeId> {
    COMPUTATION_STACK.with(|s| s.borrow().last().copied())
}

/// Return `true` if `id` is anywhere on the thread-local computation stack.
fn stack_contains(id: NodeId) -> bool {
    COMPUTATION_STACK.with(|s| s.borrow().contains(&id))
}

// ─── Error ───────────────────────────────────────────────────────────────────

/// Errors produced by the reactive runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum ReactiveError {
    /// A computation would read its own result — runtime self-reference cycle
    /// detected via the thread-local evaluation stack.
    Cycle,
    /// Inserting a dependency edge would introduce a cycle in the dep graph —
    /// detected via DFS before the edge is committed.
    DependencyCycle,
    /// The stored value's concrete type does not match the requested `T`.
    TypeMismatch,
}

impl std::fmt::Display for ReactiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReactiveError::Cycle => {
                write!(f, "reactive cycle: a computed node reads its own value")
            }
            ReactiveError::DependencyCycle => {
                write!(f, "reactive dependency cycle detected on edge insertion")
            }
            ReactiveError::TypeMismatch => write!(
                f,
                "reactive type mismatch: stored type does not match requested type"
            ),
        }
    }
}

impl std::error::Error for ReactiveError {}

// ─── NodeId ──────────────────────────────────────────────────────────────────

/// A stable, opaque identifier for a reactive node (signal or computed).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(u64);

// ─── NodeKind ────────────────────────────────────────────────────────────────

/// Internal storage for a reactive graph node.
enum NodeKind {
    /// A settable cell holding a type-erased value.
    Signal {
        /// The current value.
        value: AnyValue,
    },
    /// A lazily evaluated, cached derived value.
    Computed {
        /// The function that computes the value.
        ///
        /// Stored behind `Arc` so it can be cloned out of the write lock before
        /// being called — holding the lock across user code would deadlock.
        thunk: ArcThunk,
        /// The most recently computed value, or `None` if never evaluated.
        cached: Option<AnyValue>,
        /// Whether the cached value is stale and must be recomputed.
        dirty: bool,
    },
}

// ─── RuntimeInner ────────────────────────────────────────────────────────────

/// The mutable interior of [`ReactiveRuntime`].
struct RuntimeInner {
    /// All nodes indexed by [`NodeId`].
    nodes: HashMap<NodeId, NodeKind>,
    /// `deps[N]` = nodes that *read* `N` (dependents of `N`).
    ///
    /// When `N` changes, every node in `deps[N]` must be marked dirty.
    deps: HashMap<NodeId, Vec<NodeId>>,
    /// Monotonically increasing counter used to generate [`NodeId`]s.
    next_id: u64,
}

impl RuntimeInner {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            deps: HashMap::new(),
            next_id: 0,
        }
    }

    /// Allocate and return a fresh [`NodeId`].
    fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Return `true` if there is a path from `start` to `target` through the
    /// existing `deps` graph (BFS).
    ///
    /// Used to detect whether adding `deps[source].push(caller)` would create
    /// a cycle: we check if there is a path from `caller` → `source` (since
    /// the new edge goes `source → caller`, a cycle exists iff `caller` is
    /// already reachable from `source` via dep edges).
    fn reachable(&self, start: NodeId, target: NodeId) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some(current) = queue.pop_front() {
            if current == target {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }
            if let Some(dependents) = self.deps.get(&current) {
                for &dep in dependents {
                    queue.push_back(dep);
                }
            }
        }
        false
    }

    /// Register that `caller` depends on `source` (i.e. `caller` reads `source`).
    ///
    /// Adds `caller` to `deps[source]`, avoiding duplicates.  Returns
    /// `Err(DependencyCycle)` if the new edge would form a cycle.
    fn try_add_dependency(&mut self, source: NodeId, caller: NodeId) -> Result<(), ReactiveError> {
        // A cycle exists if `source` is already reachable from `caller`.
        if self.reachable(caller, source) {
            return Err(ReactiveError::DependencyCycle);
        }
        let dependents = self.deps.entry(source).or_default();
        if !dependents.contains(&caller) {
            dependents.push(caller);
        }
        Ok(())
    }

    /// Mark all transitive dependents of `id` as dirty (BFS).
    fn mark_dirty_transitive(&mut self, id: NodeId) {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        if let Some(dependents) = self.deps.get(&id) {
            for &dep in dependents {
                queue.push_back(dep);
            }
        }
        while let Some(current) = queue.pop_front() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(NodeKind::Computed { dirty, .. }) = self.nodes.get_mut(&current) {
                *dirty = true;
            }
            if let Some(dependents) = self.deps.get(&current) {
                for &dep in dependents {
                    queue.push_back(dep);
                }
            }
        }
    }
}

// ─── ReactiveRuntime ─────────────────────────────────────────────────────────

/// A shared reactive graph that owns [`Signal`]s and [`Computed`]s.
///
/// The runtime is `Clone` (and `Send + Sync`) because it is backed by
/// `Arc<RwLock<_>>`.  All handles (`Signal`, `Computed`) clone the same `Arc`.
#[derive(Clone)]
pub struct ReactiveRuntime {
    inner: Arc<RwLock<RuntimeInner>>,
}

impl Default for ReactiveRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactiveRuntime {
    /// Create a new, empty reactive runtime.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RuntimeInner::new())),
        }
    }

    /// Create a new [`Signal`] holding `initial`.
    ///
    /// The returned handle is lightweight; clone it freely.
    pub fn signal<T: Send + Sync + Clone + 'static>(&self, initial: T) -> Signal<T> {
        let mut inner = self
            .inner
            .write()
            .expect("ReactiveRuntime::signal: RwLock poisoned");
        let id = inner.alloc_id();
        inner.nodes.insert(
            id,
            NodeKind::Signal {
                value: Box::new(initial),
            },
        );
        drop(inner);
        Signal {
            runtime: Arc::clone(&self.inner),
            id,
            _phantom: PhantomData,
        }
    }

    /// Create a new [`Computed`] whose value is derived by calling `f`.
    ///
    /// The thunk `f` is called lazily on the first `get()` and whenever the
    /// cached value is stale.  Dependency edges are registered automatically
    /// when `f` calls `.get()` on signals or other computeds.
    ///
    /// Returns `Err(ReactiveError::DependencyCycle)` if a cycle is detected on
    /// the first evaluation (not possible via the safe public API).
    pub fn computed<T: Send + Sync + Clone + 'static>(
        &self,
        f: impl Fn() -> T + Send + Sync + 'static,
    ) -> Result<Computed<T>, ReactiveError> {
        let thunk: ArcThunk = Arc::new(move || Box::new(f()) as AnyValue);
        let mut inner = self
            .inner
            .write()
            .expect("ReactiveRuntime::computed: RwLock poisoned");
        let id = inner.alloc_id();
        inner.nodes.insert(
            id,
            NodeKind::Computed {
                thunk,
                cached: None,
                dirty: true,
            },
        );
        drop(inner);
        Ok(Computed {
            runtime: Arc::clone(&self.inner),
            id,
            _phantom: PhantomData,
        })
    }
}

// ─── Signal<T> ───────────────────────────────────────────────────────────────

/// A settable reactive value of type `T`.
///
/// `Signal<T>` is a lightweight handle; clone it to share access to the same
/// reactive cell.  All clones observe the same value and propagation.
pub struct Signal<T: Send + Sync + Clone + 'static> {
    runtime: Arc<RwLock<RuntimeInner>>,
    /// The node's identifier within the shared runtime.
    id: NodeId,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone + 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime),
            id: self.id,
            _phantom: PhantomData,
        }
    }
}

impl<T: Send + Sync + Clone + 'static> Signal<T> {
    /// Read the current value.
    ///
    /// If called inside a [`Computed`] thunk, registers the reading computed as
    /// a dependent so future `set()` calls propagate correctly.
    ///
    /// # Panics
    /// Panics only if the `RwLock` is poisoned or the stored type violates `T`
    /// (neither is possible via the public API).
    pub fn get(&self) -> T {
        // (1) Acquire read lock, clone the value, release.
        let value = {
            let inner = self.runtime.read().expect("Signal::get: RwLock poisoned");
            match inner.nodes.get(&self.id) {
                Some(NodeKind::Signal { value }) => value
                    .downcast_ref::<T>()
                    .expect("Signal<T> type invariant: stored type must match T")
                    .clone(),
                _ => panic!("Signal<T> node not found or wrong kind"),
            }
        }; // read lock released here

        // (2) Register dependency edge if inside a computed evaluation.
        if let Some(caller) = stack_top() {
            let mut inner = self
                .runtime
                .write()
                .expect("Signal::get dep-reg: RwLock poisoned");
            // Ignore errors: DependencyCycle is surfaced by Computed::get().
            let _ = inner.try_add_dependency(self.id, caller);
        }

        value
    }

    /// Update the value and mark all transitive dependents dirty.
    pub fn set(&self, value: T) {
        let mut inner = self.runtime.write().expect("Signal::set: RwLock poisoned");
        match inner.nodes.get_mut(&self.id) {
            Some(NodeKind::Signal { value: stored }) => {
                *stored = Box::new(value);
            }
            _ => panic!("Signal<T> node not found or wrong kind"),
        }
        inner.mark_dirty_transitive(self.id);
        // write lock released here
    }
}

// ─── Computed<T> ─────────────────────────────────────────────────────────────

/// A lazily evaluated reactive value derived from signals or other computeds.
///
/// `Computed<T>` is a lightweight handle; clone it to share access to the same
/// derived node.
pub struct Computed<T: Send + Sync + Clone + 'static> {
    runtime: Arc<RwLock<RuntimeInner>>,
    /// The node's identifier within the shared runtime.
    id: NodeId,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + Clone + 'static> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime),
            id: self.id,
            _phantom: PhantomData,
        }
    }
}

impl<T: Send + Sync + Clone + 'static> Computed<T> {
    /// Read the current value, recomputing if stale.
    ///
    /// If called inside another computed thunk, registers the outer computed as
    /// a dependent of this one.
    ///
    /// # Errors
    /// - [`ReactiveError::Cycle`] — this node is already on the evaluation
    ///   stack (self-referential cycle).
    /// - [`ReactiveError::DependencyCycle`] — registering a dep edge would
    ///   introduce a graph cycle.
    /// - [`ReactiveError::TypeMismatch`] — the cached value cannot be downcast
    ///   to `T` (unreachable via the public API).
    pub fn get(&self) -> Result<T, ReactiveError> {
        // ── Layer 1 cycle check: self already on the evaluation stack? ────────
        if stack_contains(self.id) {
            return Err(ReactiveError::Cycle);
        }

        // ── Register dependency edge (even for a clean/cached read) ───────────
        // This ensures that a computed reading another computed still gets the
        // edge, so later dirty propagation reaches all transitive dependents.
        if let Some(caller) = stack_top() {
            let mut inner = self
                .runtime
                .write()
                .expect("Computed::get dep-reg: RwLock poisoned");
            inner.try_add_dependency(self.id, caller)?;
        } // write lock released

        // ── Check dirty flag ──────────────────────────────────────────────────
        let is_dirty = {
            let inner = self
                .runtime
                .read()
                .expect("Computed::get dirty-check: RwLock poisoned");
            match inner.nodes.get(&self.id) {
                Some(NodeKind::Computed { dirty, .. }) => *dirty,
                _ => return Err(ReactiveError::TypeMismatch),
            }
        }; // read lock released

        if !is_dirty {
            // Return the cached value without recomputing.
            let inner = self
                .runtime
                .read()
                .expect("Computed::get cached-read: RwLock poisoned");
            return match inner.nodes.get(&self.id) {
                Some(NodeKind::Computed {
                    cached: Some(v), ..
                }) => v
                    .downcast_ref::<T>()
                    .cloned()
                    .ok_or(ReactiveError::TypeMismatch),
                _ => Err(ReactiveError::TypeMismatch),
            };
        } // read lock released

        // ── Recompute path ────────────────────────────────────────────────────
        // Step 1: Clone the Arc-thunk out of the RwLock guard.
        //         We MUST release the lock before calling the thunk, because
        //         the thunk calls signal.get() which acquires the write lock
        //         for dep registration — holding both would deadlock.
        let thunk: ArcThunk = {
            let inner = self
                .runtime
                .read()
                .expect("Computed::get thunk-clone: RwLock poisoned");
            match inner.nodes.get(&self.id) {
                Some(NodeKind::Computed { thunk, .. }) => Arc::clone(thunk),
                _ => return Err(ReactiveError::TypeMismatch),
            }
        }; // read lock released — thunk is now owned by this stack frame

        // Step 2: Push self onto the computation stack and run the thunk.
        //         No lock is held at this point.
        stack_push(self.id);
        let new_value: AnyValue = thunk();
        stack_pop();

        // Step 3: Re-acquire write lock, store the new value, clear dirty flag.
        {
            let mut inner = self
                .runtime
                .write()
                .expect("Computed::get store: RwLock poisoned");
            match inner.nodes.get_mut(&self.id) {
                Some(NodeKind::Computed { cached, dirty, .. }) => {
                    *cached = Some(new_value);
                    *dirty = false;
                }
                _ => return Err(ReactiveError::TypeMismatch),
            }
        } // write lock released

        // Step 4: Read the freshly stored value back and return it.
        let inner = self
            .runtime
            .read()
            .expect("Computed::get final-read: RwLock poisoned");
        match inner.nodes.get(&self.id) {
            Some(NodeKind::Computed {
                cached: Some(v), ..
            }) => v
                .downcast_ref::<T>()
                .cloned()
                .ok_or(ReactiveError::TypeMismatch),
            _ => Err(ReactiveError::TypeMismatch),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Basic signal read and write.
    #[test]
    fn test_signal_get_set() {
        let rt = ReactiveRuntime::new();
        let s = rt.signal(42i32);
        assert_eq!(s.get(), 42);
        s.set(99);
        assert_eq!(s.get(), 99);
    }

    /// A computed node derives its value from a signal and updates when the
    /// signal changes.
    #[test]
    fn test_computed_derives_from_signal() {
        let rt = ReactiveRuntime::new();
        let s = rt.signal(10i32);
        let sc = s.clone();
        let c = rt.computed(move || sc.get() * 2).expect("no cycle");
        assert_eq!(c.get(), Ok(20));
        s.set(5);
        assert_eq!(c.get(), Ok(10));
    }

    /// Dirty propagation through a chain a → b → c.
    #[test]
    fn test_chain_propagation() {
        let rt = ReactiveRuntime::new();
        let a = rt.signal(1i32);
        let ac = a.clone();
        let b = rt.computed(move || ac.get() * 2).expect("b ok");
        let bc = b.clone();
        let c = rt.computed(move || bc.get().expect("b") + 1).expect("c ok");

        // Initial: a=1, b=2, c=3
        assert_eq!(c.get(), Ok(3));

        // After set: a=10, b=20, c=21
        a.set(10);
        assert_eq!(c.get(), Ok(21));
    }

    /// DFS-based cycle detection via `try_add_dependency` — the diamond pattern
    /// must NOT produce a false-positive cycle error.
    #[test]
    fn test_cycle_detection_no_false_positive_diamond() {
        // Diamond: a → b, a → c, (b, c) → d
        let rt = ReactiveRuntime::new();
        let a = rt.signal(2i32);
        let ac1 = a.clone();
        let ac2 = a.clone();
        let b = rt.computed(move || ac1.get() * 3).expect("b ok");
        let c = rt.computed(move || ac2.get() + 10).expect("c ok");
        let bc = b.clone();
        let cc = c.clone();
        let d = rt
            .computed(move || bc.get().expect("b") + cc.get().expect("c"))
            .expect("diamond: no cycle");

        // a=2: b=6, c=12, d=18
        assert_eq!(d.get(), Ok(18));
        a.set(5);
        // a=5: b=15, c=15, d=30
        assert_eq!(d.get(), Ok(30));
    }

    /// The DFS guard in `try_add_dependency` correctly rejects an edge that
    /// would form a cycle in the dep graph.
    #[test]
    fn test_cycle_detection_dep_graph_dfs() {
        let rt = ReactiveRuntime::new();
        let a = rt.signal(1i32);
        let ac = a.clone();
        let b = rt.computed(move || ac.get() + 1).expect("b ok");

        // Trigger b.get() so the dep edge a→b is registered in the graph.
        let _ = b.get();

        // Attempting to add the reverse edge b→a (meaning a "reads" b) would
        // create a cycle: a→b already exists.
        let result = {
            let mut inner = rt.inner.write().unwrap();
            // try_add_dependency(source=b, caller=a): deps[b].push(a)
            // But there is already a path caller=a → source=b, so this cycles.
            inner.try_add_dependency(b.id, a.id)
        };
        assert_eq!(result, Err(ReactiveError::DependencyCycle));
    }

    /// Diamond recomputes to correct values after multiple set() calls.
    #[test]
    fn test_diamond_recomputes_correctly() {
        let rt = ReactiveRuntime::new();
        let a = rt.signal(1i32);
        let ac1 = a.clone();
        let ac2 = a.clone();
        let b = rt.computed(move || ac1.get() * 2).expect("b");
        let c = rt.computed(move || ac2.get() + 5).expect("c");
        let bc = b.clone();
        let cc = c.clone();
        let d = rt
            .computed(move || bc.get().expect("b") + cc.get().expect("c"))
            .expect("d");

        // a=1: b=2, c=6, d=8
        assert_eq!(d.get(), Ok(8));
        a.set(3);
        // a=3: b=6, c=8, d=14
        assert_eq!(d.get(), Ok(14));
        a.set(0);
        // a=0: b=0, c=5, d=5
        assert_eq!(d.get(), Ok(5));
    }

    /// Compile-time verification that all public reactive types implement
    /// `Send + Sync`.
    #[test]
    fn test_send_sync_bounds() {
        fn _assert_send<T: Send>() {}
        fn _assert_sync<T: Sync>() {}
        fn _check(rt: ReactiveRuntime) {
            let _: &dyn Send = &rt;
            let _: &dyn Sync = &rt;
        }
        _assert_send::<ReactiveRuntime>();
        _assert_sync::<ReactiveRuntime>();
        _assert_send::<Signal<i32>>();
        _assert_sync::<Signal<i32>>();
        _assert_send::<Computed<i32>>();
        _assert_sync::<Computed<i32>>();
    }

    /// Nested computed (B reads A which reads signal x).  Must complete without
    /// deadlock within a tight deadline.
    #[test]
    fn test_no_deadlock_nested_computed() {
        use std::time::{Duration, Instant};

        let rt = ReactiveRuntime::new();
        let x = rt.signal(7i32);
        let xc = x.clone();
        let comp_a = rt.computed(move || xc.get() * 3).expect("a ok");
        let ac = comp_a.clone();
        let comp_b = rt.computed(move || ac.get().expect("a") + 1).expect("b ok");

        let start = Instant::now();
        let result = comp_b.get();
        let elapsed = start.elapsed();

        // 7 * 3 + 1 = 22
        assert_eq!(result, Ok(22));
        assert!(
            elapsed < Duration::from_secs(1),
            "get() should not deadlock (elapsed: {elapsed:?})",
        );
    }
}
