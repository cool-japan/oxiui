//! Layout memoization keyed by `(WidgetId, input Size, content hash)`.
//!
//! Re-laying out an unchanged subtree every frame is wasteful. [`LayoutCache`]
//! stores the computed [`Rect`] for a node keyed by the inputs that determine
//! it: the node's [`WidgetId`], the available [`Size`] it was laid out into, and
//! a `u64` *content hash* the caller supplies (a digest of whatever else affects
//! the result — text, font, child sizes, …). A lookup hits only when all three
//! match, so any change in available space or content misses and recomputes.
//!
//! Entries can also be invalidated explicitly via a per-node **dirty** flag,
//! which a tree mutation (resize, child add/remove) sets. The cache tracks hit
//! and miss counts so callers can measure effectiveness.
//!
//! `f32` sizes are not `Hash`/`Eq`, so the size is quantised to its raw bits
//! (`to_bits`) for the key; this treats `0.0` and `-0.0` as equal and is exact
//! for finite values, which is what we want for cache identity.

use crate::geometry::{Rect, Size};
use crate::tree::WidgetId;
use std::collections::{HashMap, HashSet};

/// The composite key identifying a cached layout result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LayoutKey {
    id: WidgetId,
    width_bits: u32,
    height_bits: u32,
    content_hash: u64,
}

impl LayoutKey {
    fn new(id: WidgetId, avail: Size, content_hash: u64) -> Self {
        Self {
            id,
            width_bits: avail.width.to_bits(),
            height_bits: avail.height.to_bits(),
            content_hash,
        }
    }
}

/// A memoizing cache of computed layout rectangles.
#[derive(Debug, Default)]
pub struct LayoutCache {
    entries: HashMap<LayoutKey, Rect>,
    /// Nodes explicitly marked dirty; their entries are ignored until refreshed.
    dirty: HashSet<WidgetId>,
    hits: u64,
    misses: u64,
}

impl LayoutCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache hit count since creation (or last [`reset_stats`](Self::reset_stats)).
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache miss count since creation (or last [`reset_stats`](Self::reset_stats)).
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Hit rate in `[0, 1]`; `0` when there have been no accesses.
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }

    /// Reset the hit/miss counters (leaves entries intact).
    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }

    /// Look up the cached rect for `(id, avail, content_hash)`.
    ///
    /// Returns `None` (a miss) when there is no matching entry **or** when `id`
    /// is currently marked dirty. Updates the hit/miss counters.
    pub fn get(&mut self, id: WidgetId, avail: Size, content_hash: u64) -> Option<Rect> {
        if self.dirty.contains(&id) {
            self.misses += 1;
            return None;
        }
        let key = LayoutKey::new(id, avail, content_hash);
        match self.entries.get(&key) {
            Some(rect) => {
                self.hits += 1;
                Some(*rect)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Store the computed `rect` for `(id, avail, content_hash)`, clearing any
    /// dirty flag on `id` (it is now freshly computed).
    pub fn put(&mut self, id: WidgetId, avail: Size, content_hash: u64, rect: Rect) {
        self.dirty.remove(&id);
        self.entries
            .insert(LayoutKey::new(id, avail, content_hash), rect);
    }

    /// Mark `id` dirty so its next [`get`](Self::get) misses regardless of key.
    /// The stale entries are dropped immediately to bound memory.
    pub fn invalidate(&mut self, id: WidgetId) {
        self.dirty.insert(id);
        self.entries.retain(|k, _| k.id != id);
    }

    /// Mark several nodes dirty at once (e.g. an invalidated subtree).
    pub fn invalidate_many(&mut self, ids: impl IntoIterator<Item = WidgetId>) {
        for id in ids {
            self.invalidate(id);
        }
    }

    /// Whether `id` is currently flagged dirty.
    pub fn is_dirty(&self, id: WidgetId) -> bool {
        self.dirty.contains(&id)
    }

    /// Drop every entry and dirty flag (a full layout invalidation).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.dirty.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sz(w: f32, h: f32) -> Size {
        Size::new(w, h)
    }

    #[test]
    fn hit_after_put() {
        let mut c = LayoutCache::new();
        let id = WidgetId(1);
        let rect = Rect::new(0.0, 0.0, 10.0, 20.0);
        // First access misses.
        assert!(c.get(id, sz(100.0, 100.0), 7).is_none());
        c.put(id, sz(100.0, 100.0), 7, rect);
        // Now it hits with identical inputs.
        assert_eq!(c.get(id, sz(100.0, 100.0), 7), Some(rect));
        assert_eq!(c.hits(), 1);
        assert_eq!(c.misses(), 1);
    }

    #[test]
    fn miss_on_different_size_or_content() {
        let mut c = LayoutCache::new();
        let id = WidgetId(1);
        c.put(id, sz(100.0, 100.0), 7, Rect::ZERO);
        // Different available size -> miss.
        assert!(c.get(id, sz(120.0, 100.0), 7).is_none());
        // Different content hash -> miss.
        assert!(c.get(id, sz(100.0, 100.0), 8).is_none());
        // Original still hits.
        assert_eq!(c.get(id, sz(100.0, 100.0), 7), Some(Rect::ZERO));
    }

    #[test]
    fn dirty_flag_forces_miss_until_refreshed() {
        let mut c = LayoutCache::new();
        let id = WidgetId(2);
        c.put(id, sz(50.0, 50.0), 1, Rect::new(0.0, 0.0, 5.0, 5.0));
        assert!(c.get(id, sz(50.0, 50.0), 1).is_some());
        // Invalidate -> next get misses even with identical key.
        c.invalidate(id);
        assert!(c.is_dirty(id));
        assert!(c.get(id, sz(50.0, 50.0), 1).is_none());
        // Re-putting clears dirty and restores hits.
        c.put(id, sz(50.0, 50.0), 1, Rect::new(0.0, 0.0, 5.0, 5.0));
        assert!(!c.is_dirty(id));
        assert!(c.get(id, sz(50.0, 50.0), 1).is_some());
    }

    #[test]
    fn invalidate_drops_stale_entries() {
        let mut c = LayoutCache::new();
        let id = WidgetId(3);
        // Two entries for the same node under different sizes.
        c.put(id, sz(10.0, 10.0), 0, Rect::ZERO);
        c.put(id, sz(20.0, 20.0), 0, Rect::ZERO);
        assert_eq!(c.len(), 2);
        c.invalidate(id);
        // Both gone (bounded memory), node marked dirty.
        assert_eq!(c.len(), 0);
        assert!(c.is_dirty(id));
    }

    #[test]
    fn hit_rate_and_reset() {
        let mut c = LayoutCache::new();
        let id = WidgetId(4);
        c.put(id, sz(1.0, 1.0), 0, Rect::ZERO);
        let _ = c.get(id, sz(1.0, 1.0), 0); // hit
        let _ = c.get(id, sz(2.0, 2.0), 0); // miss
        assert!((c.hit_rate() - 0.5).abs() < 1e-6);
        c.reset_stats();
        assert_eq!(c.hits(), 0);
        assert_eq!(c.misses(), 0);
        assert_eq!(c.hit_rate(), 0.0);
    }

    #[test]
    fn invalidate_many_and_clear() {
        let mut c = LayoutCache::new();
        c.put(WidgetId(1), sz(1.0, 1.0), 0, Rect::ZERO);
        c.put(WidgetId(2), sz(1.0, 1.0), 0, Rect::ZERO);
        c.invalidate_many([WidgetId(1), WidgetId(2)]);
        assert!(c.is_dirty(WidgetId(1)) && c.is_dirty(WidgetId(2)));
        c.clear();
        assert!(c.is_empty());
        assert!(!c.is_dirty(WidgetId(1)));
    }
}
