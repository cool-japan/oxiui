//! Lazy / dirty-flag value wrapper.
//!
//! [`Lazy<T>`] wraps a computed value that is expensive to recompute every
//! frame.  The inner value is computed on demand via a closure and cached until
//! explicitly invalidated.
//!
//! # Typical use
//!
//! ```rust
//! use oxiui_accessibility::dirty::Lazy;
//!
//! let mut effective_label: Lazy<String> = Lazy::new();
//!
//! // First access — the closure runs.
//! let label = effective_label.get_or_compute(|| "Hello".to_string());
//! assert_eq!(label, "Hello");
//!
//! // Second access — the closure is NOT run again.
//! let label2 = effective_label.get_or_compute(|| "Ignored".to_string());
//! assert_eq!(label2, "Hello");
//!
//! // Invalidate and recompute.
//! effective_label.invalidate();
//! let label3 = effective_label.get_or_compute(|| "World".to_string());
//! assert_eq!(label3, "World");
//! ```

// ── Lazy<T> ──────────────────────────────────────────────────────────────────

/// A dirty-flag wrapper for a lazily computed value.
///
/// The inner value is computed on the first call to [`get_or_compute`] and
/// cached thereafter.  Call [`invalidate`] to mark the cache stale; the next
/// [`get_or_compute`] call will recompute.
///
/// [`get_or_compute`]: Lazy::get_or_compute
/// [`invalidate`]: Lazy::invalidate
#[derive(Debug, Clone)]
pub struct Lazy<T: Clone> {
    value: Option<T>,
    dirty: bool,
}

impl<T: Clone> Default for Lazy<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Lazy<T> {
    /// Create a new, uncomputed lazy value (dirty by construction).
    pub fn new() -> Self {
        Self {
            value: None,
            dirty: true,
        }
    }

    /// Create a pre-populated lazy value (not dirty).
    pub fn with_value(value: T) -> Self {
        Self {
            value: Some(value),
            dirty: false,
        }
    }

    /// Returns `true` if the cached value is absent or stale.
    pub fn is_dirty(&self) -> bool {
        self.dirty || self.value.is_none()
    }

    /// Return a reference to the cached value, running `compute` if necessary.
    ///
    /// After this call the value is clean until the next [`invalidate`] call.
    ///
    /// [`invalidate`]: Lazy::invalidate
    pub fn get_or_compute<F: FnOnce() -> T>(&mut self, compute: F) -> &T {
        if self.is_dirty() {
            self.value = Some(compute());
            self.dirty = false;
        }
        // Safe: we just ensured `self.value` is `Some`.
        match self.value.as_ref() {
            Some(v) => v,
            None => unreachable!("value is Some after get_or_compute"),
        }
    }

    /// Mark the cached value as stale, forcing recomputation on next access.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Overwrite the cached value directly without running a closure.
    ///
    /// Useful when the value is known ahead of time (e.g. precomputed on a
    /// background thread).
    pub fn set(&mut self, value: T) {
        self.value = Some(value);
        self.dirty = false;
    }

    /// Return the cached value if it is clean, or `None` if dirty/absent.
    pub fn get_if_clean(&self) -> Option<&T> {
        if self.is_dirty() {
            None
        } else {
            self.value.as_ref()
        }
    }

    /// Consume the wrapper and return the inner value, if present.
    pub fn into_inner(self) -> Option<T> {
        self.value
    }
}

// ── DirtyTracker ─────────────────────────────────────────────────────────────

/// Tracks which a11y tree nodes are dirty and need to be rebuilt.
///
/// Each window's a11y tree is identified by a [`crate::WindowA11yId`].  When
/// something in that window changes (widget property update, focus change, etc.)
/// the caller marks the window dirty via [`DirtyTracker::mark_dirty`]; the next
/// frame's a11y pass can then call [`DirtyTracker::is_dirty`] before deciding
/// whether to run an expensive [`crate::A11yTree::build_and_store`] / diff
/// cycle.
///
/// A monotonically increasing `generation` counter is bumped on every
/// `mark_dirty` call and can be used as a cheap change-detection token by
/// callers that keep their own generation snapshot.
#[derive(Default)]
pub struct DirtyTracker {
    dirty_ids: std::collections::HashSet<crate::WindowA11yId>,
    generation: u64,
}

impl DirtyTracker {
    /// Create a new, empty tracker with generation 0.
    pub fn new() -> Self {
        Self {
            dirty_ids: std::collections::HashSet::new(),
            generation: 0,
        }
    }

    /// Mark a window's a11y tree as needing rebuild.
    ///
    /// Subsequent calls to [`is_dirty`] for this `id` will return `true` until
    /// [`clear`] is called.  The internal generation counter is bumped on every
    /// call, even if the id was already dirty.
    ///
    /// [`is_dirty`]: DirtyTracker::is_dirty
    /// [`clear`]: DirtyTracker::clear
    pub fn mark_dirty(&mut self, id: crate::WindowA11yId) {
        self.dirty_ids.insert(id);
        self.generation += 1;
    }

    /// Check whether `id`'s a11y tree is dirty and needs rebuilding.
    pub fn is_dirty(&self, id: crate::WindowA11yId) -> bool {
        self.dirty_ids.contains(&id)
    }

    /// Clear the dirty flag for `id` after its tree has been rebuilt.
    ///
    /// Has no effect if `id` was not dirty.
    pub fn clear(&mut self, id: crate::WindowA11yId) {
        self.dirty_ids.remove(&id);
    }

    /// The current generation counter.
    ///
    /// Incremented on every [`mark_dirty`] call.  Callers can store a snapshot
    /// of this value and compare on the next frame to detect any change without
    /// iterating all known window ids.
    ///
    /// [`mark_dirty`]: DirtyTracker::mark_dirty
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
