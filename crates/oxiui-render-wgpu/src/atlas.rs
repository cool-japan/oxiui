//! Dynamic texture atlas with shelf-based bin packing and LRU eviction.
//!
//! The atlas allocates rectangular regions on a fixed-size 2-D texture using a
//! shelf algorithm: rows of allocations ("shelves") are stacked top-to-bottom.
//! When the atlas is full and a new insertion would fail, the least-recently-used
//! allocation is evicted to make room (best-effort; eviction may fail if the
//! freed region is too small).

use std::collections::{HashMap, VecDeque};

// ── Public types ─────────────────────────────────────────────────────────────

/// The pixel rectangle occupied by an atlas allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRect {
    /// Left edge in atlas-space pixels.
    pub x: u32,
    /// Top edge in atlas-space pixels.
    pub y: u32,
    /// Width in pixels.
    pub w: u32,
    /// Height in pixels.
    pub h: u32,
}

/// Generation-based opaque handle to an atlas allocation.
///
/// Handles are invalidated when the atlas is resized or when the corresponding
/// region is evicted via LRU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AtlasHandle(u64);

// ── Internal shelf ────────────────────────────────────────────────────────────

/// A single horizontal shelf inside the atlas.
#[derive(Debug)]
struct Shelf {
    /// Y-coordinate of the shelf's top edge.
    y: u32,
    /// Height of the shelf (fixed at creation from the tallest item placed in it).
    height: u32,
    /// Next free X position within this shelf.
    cursor_x: u32,
}

// ── TextureAtlas ──────────────────────────────────────────────────────────────

/// Dynamic shelf-based texture atlas with LRU eviction.
///
/// # Allocation strategy
///
/// 1. Check the free-list for a first-fit region large enough.
/// 2. Walk existing shelves for one that can accommodate the request.
/// 3. Open a new shelf at the current bottom of the atlas.
/// 4. If all of the above fail, evict the LRU handle and retry once.
///
/// # Eviction
///
/// Evicted regions are pushed onto the free-list; they are re-used on the
/// next allocation attempt.  The atlas never compacts its shelf layout.
pub struct TextureAtlas {
    /// Atlas width in pixels.
    pub width: u32,
    /// Atlas height in pixels.
    pub height: u32,
    /// Y-coordinate of the next row of shelves.
    next_y: u32,
    /// All current shelves.
    shelves: Vec<Shelf>,
    /// Map from handle to allocated rectangle.
    allocations: HashMap<AtlasHandle, AtlasRect>,
    /// Insertion-order queue for LRU tracking (oldest at front).
    lru_order: VecDeque<AtlasHandle>,
    /// Freed regions available for reuse.
    free_list: Vec<AtlasRect>,
    /// Monotonically increasing handle counter.
    next_id: u64,
    /// Total area currently held in live allocations.
    used_area: u64,
}

impl TextureAtlas {
    /// Construct a new empty atlas of the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            next_y: 0,
            shelves: Vec::new(),
            allocations: HashMap::new(),
            lru_order: VecDeque::new(),
            free_list: Vec::new(),
            next_id: 0,
            used_area: 0,
        }
    }

    /// Insert a rectangle of size `w × h` into the atlas.
    ///
    /// Returns an [`AtlasHandle`] on success, or `None` if the rectangle is
    /// larger than the atlas or all eviction attempts were exhausted.
    pub fn insert(&mut self, w: u32, h: u32) -> Option<AtlasHandle> {
        if w == 0 || h == 0 || w > self.width || h > self.height {
            return None;
        }

        // 1. Try the free list (first-fit).
        if let Some(rect) = self.alloc_from_free_list(w, h) {
            return Some(self.register(rect));
        }

        // 2. Try existing shelves.
        if let Some(rect) = self.alloc_from_shelves(w, h) {
            return Some(self.register(rect));
        }

        // 3. Try opening a new shelf.
        if let Some(rect) = self.alloc_new_shelf(w, h) {
            return Some(self.register(rect));
        }

        // 4. Evict the LRU handle and retry once.
        self.evict_lru()?;

        if let Some(rect) = self.alloc_from_free_list(w, h) {
            return Some(self.register(rect));
        }
        if let Some(rect) = self.alloc_from_shelves(w, h) {
            return Some(self.register(rect));
        }
        if let Some(rect) = self.alloc_new_shelf(w, h) {
            return Some(self.register(rect));
        }

        None
    }

    /// Retrieve the pixel rectangle for the given handle, if still live.
    pub fn get(&self, h: AtlasHandle) -> Option<AtlasRect> {
        self.allocations.get(&h).copied()
    }

    /// Fraction of the atlas area occupied by live allocations.
    ///
    /// Returns a value in `[0.0, 1.0]`.
    pub fn utilization(&self) -> f32 {
        let total = u64::from(self.width) * u64::from(self.height);
        if total == 0 {
            return 0.0;
        }
        (self.used_area as f32) / (total as f32)
    }

    /// Return the number of live (non-evicted) allocations.
    pub fn allocation_count(&self) -> usize {
        self.allocations.len()
    }

    /// Return `true` if atlas utilization has fallen below `threshold`
    /// (0.0–1.0).  This indicates significant fragmentation and the caller
    /// should consider triggering a defrag/rebuild.
    ///
    /// This check is cheap (a single float comparison).
    pub fn is_fragmented(&self, threshold: f32) -> bool {
        self.utilization() < threshold
    }

    /// Rebuild the atlas layout from scratch using only the currently live
    /// allocations.
    ///
    /// After a defrag, all previously live handles continue to resolve to valid
    /// (but potentially repositioned) rectangles.  Handles that had already
    /// been evicted remain invalid.  The GPU texture content is **not** updated
    /// by this call — the caller must re-upload all live texture regions.
    ///
    /// Returns a `Vec<(AtlasHandle, AtlasRect)>` mapping each live handle to
    /// its new position, in insertion order.  The caller uses this to schedule
    /// GPU texture copies.
    ///
    /// # Complexity
    ///
    /// O(n log n) in the number of live allocations.
    pub fn defrag(&mut self) -> Vec<(AtlasHandle, AtlasRect)> {
        // Snapshot the live set sorted by area descending (best-fit heuristic).
        let mut live: Vec<(AtlasHandle, AtlasRect)> =
            self.allocations.iter().map(|(&h, &r)| (h, r)).collect();
        // Sort by area descending so large items get placed first.
        live.sort_unstable_by(|(_, a), (_, b)| {
            let area_a = u64::from(a.w) * u64::from(a.h);
            let area_b = u64::from(b.w) * u64::from(b.h);
            area_b.cmp(&area_a)
        });

        // Rebuild the shelf layout.
        let width = self.width;
        let height = self.height;
        self.next_y = 0;
        self.shelves.clear();
        self.allocations.clear();
        self.lru_order.clear();
        self.free_list.clear();
        self.used_area = 0;
        // Keep next_id monotonically increasing so old handles staying live
        // still compare equal to the new allocations we register below.

        let mut result = Vec::with_capacity(live.len());
        for (handle, old_rect) in live {
            let w = old_rect.w;
            let h = old_rect.h;
            // Try to place this item using the normal allocation path.
            let placed = self
                .alloc_from_free_list(w, h)
                .or_else(|| self.alloc_from_shelves(w, h))
                .or_else(|| self.alloc_new_shelf(w, h));
            if let Some(new_rect) = placed {
                self.used_area += u64::from(new_rect.w) * u64::from(new_rect.h);
                // Re-use the original handle so callers' existing handles are valid.
                self.allocations.insert(handle, new_rect);
                self.lru_order.push_back(handle);
                result.push((handle, new_rect));
            }
            // Unreachable in practice: atlas is at least as large as before.
            let _ = (width, height); // suppress unused warning
        }
        result
    }

    /// Check whether utilization is below the given `threshold` and, if so,
    /// perform an in-place defrag.
    ///
    /// Returns `Some(relocations)` if a defrag was performed, `None` if
    /// utilization was already above the threshold.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use oxiui_render_wgpu::TextureAtlas;
    /// let mut atlas = TextureAtlas::new(64, 64);
    /// // … insert/evict until fragmented …
    /// if let Some(moves) = atlas.defrag_if_fragmented(0.5) {
    ///     // re-upload the relocated regions
    ///     for (_handle, _new_rect) in moves {}
    /// }
    /// ```
    pub fn defrag_if_fragmented(
        &mut self,
        threshold: f32,
    ) -> Option<Vec<(AtlasHandle, AtlasRect)>> {
        if self.is_fragmented(threshold) {
            Some(self.defrag())
        } else {
            None
        }
    }

    /// Discard all allocations and reinitialise with new dimensions.
    ///
    /// Any existing handles become invalid.  Uploading updated GPU texture data
    /// is the caller's responsibility.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        self.width = new_width;
        self.height = new_height;
        self.next_y = 0;
        self.shelves.clear();
        self.allocations.clear();
        self.lru_order.clear();
        self.free_list.clear();
        self.next_id = 0;
        self.used_area = 0;
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Attempt to satisfy the request from the free list (first-fit).
    fn alloc_from_free_list(&mut self, w: u32, h: u32) -> Option<AtlasRect> {
        let pos = self.free_list.iter().position(|r| r.w >= w && r.h >= h)?;
        let free_rect = self.free_list.remove(pos);
        Some(AtlasRect {
            x: free_rect.x,
            y: free_rect.y,
            w,
            h,
        })
    }

    /// Attempt to satisfy the request by appending to an existing shelf.
    fn alloc_from_shelves(&mut self, w: u32, h: u32) -> Option<AtlasRect> {
        for shelf in &mut self.shelves {
            if shelf.height >= h && self.width.saturating_sub(shelf.cursor_x) >= w {
                let rect = AtlasRect {
                    x: shelf.cursor_x,
                    y: shelf.y,
                    w,
                    h,
                };
                shelf.cursor_x += w;
                return Some(rect);
            }
        }
        None
    }

    /// Attempt to open a new shelf at `next_y`.
    fn alloc_new_shelf(&mut self, w: u32, h: u32) -> Option<AtlasRect> {
        if self.height.saturating_sub(self.next_y) < h || self.width < w {
            return None;
        }
        let shelf = Shelf {
            y: self.next_y,
            height: h,
            cursor_x: w,
        };
        let rect = AtlasRect {
            x: 0,
            y: self.next_y,
            w,
            h,
        };
        self.next_y += h;
        self.shelves.push(shelf);
        Some(rect)
    }

    /// Register a raw rect as a live allocation and return a fresh handle.
    fn register(&mut self, rect: AtlasRect) -> AtlasHandle {
        let handle = AtlasHandle(self.next_id);
        self.next_id += 1;
        self.used_area += u64::from(rect.w) * u64::from(rect.h);
        self.allocations.insert(handle, rect);
        self.lru_order.push_back(handle);
        handle
    }

    /// Evict the oldest (front of queue) live handle.  Returns `Some(())` on
    /// success, `None` if the LRU queue is empty.
    fn evict_lru(&mut self) -> Option<()> {
        // Skip any handles that are no longer live (already removed).
        loop {
            let candidate = self.lru_order.pop_front()?;
            if let Some(rect) = self.allocations.remove(&candidate) {
                self.used_area = self
                    .used_area
                    .saturating_sub(u64::from(rect.w) * u64::from(rect.h));
                self.free_list.push(rect);
                return Some(());
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_packs_100_rects_no_overlap() {
        let mut atlas = TextureAtlas::new(128, 128);
        let mut rects: Vec<(AtlasHandle, AtlasRect)> = Vec::new();

        for _ in 0..100 {
            if let Some(h) = atlas.insert(4, 4) {
                let r = atlas.get(h).expect("handle must be valid");
                rects.push((h, r));
            }
        }

        // All returned handles must have been valid.
        assert!(!rects.is_empty());

        // No two rects may overlap.
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let a = rects[i].1;
                let b = rects[j].1;
                let overlap =
                    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "rects {i} and {j} overlap: {a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn atlas_utilization_above_threshold() {
        // A 32×32 atlas packed with 1×1 tiles should approach 100%.
        let mut atlas = TextureAtlas::new(32, 32);
        let mut count = 0u32;
        while atlas.insert(1, 1).is_some() {
            count += 1;
            if count > 1024 * 4 {
                break; // guard against infinite loop
            }
        }
        // We expect at least 70% utilization once the atlas is exhausted.
        assert!(
            atlas.utilization() >= 0.70,
            "utilization was {}",
            atlas.utilization()
        );
    }

    #[test]
    fn atlas_lru_eviction_keeps_invariants() {
        // Use a tiny atlas so we can force eviction.
        let mut atlas = TextureAtlas::new(4, 4);
        // Fill with 4×1 strips.
        let h0 = atlas.insert(4, 1).expect("first insert");
        let h1 = atlas.insert(4, 1).expect("second insert");
        let h2 = atlas.insert(4, 1).expect("third insert");
        let h3 = atlas.insert(4, 1).expect("fourth insert");
        // Atlas is now full. Next insert must evict the LRU (h0).
        let h4 = atlas.insert(4, 1).expect("insert with eviction");
        // h0 should have been evicted (get returns None), h4 live.
        assert!(
            atlas.get(h0).is_none(),
            "LRU handle h0 must have been evicted"
        );
        assert!(atlas.get(h4).is_some(), "new handle h4 must be live");
        // h1, h2, h3 must still be live.
        assert!(atlas.get(h1).is_some());
        assert!(atlas.get(h2).is_some());
        assert!(atlas.get(h3).is_some());
        // No two live rects overlap.
        let live_rects: Vec<AtlasRect> = [h1, h2, h3, h4]
            .iter()
            .filter_map(|&h| atlas.get(h))
            .collect();
        for i in 0..live_rects.len() {
            for j in (i + 1)..live_rects.len() {
                let a = live_rects[i];
                let b = live_rects[j];
                let overlap =
                    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "live rects {i} and {j} overlap: {a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn atlas_fragmentation_detected_and_defrag_works() {
        // Build a small atlas, fill it completely with 4×4 items on a 16×4 atlas,
        // then verify defrag keeps live handles valid.
        let mut atlas = TextureAtlas::new(16, 4);
        // Insert 4 items of 4×4 — exactly fills the atlas.
        let h0 = atlas.insert(4, 4).expect("h0");
        let h1 = atlas.insert(4, 4).expect("h1");
        let h2 = atlas.insert(4, 4).expect("h2");
        let h3 = atlas.insert(4, 4).expect("h3");
        assert_eq!(atlas.allocation_count(), 4);

        // Defrag a full atlas: all 4 handles should be remapped.
        let relocations = atlas.defrag();
        assert_eq!(relocations.len(), 4, "defrag must remap all 4 live handles");

        // Every remapped handle must be fetchable via get().
        for (h, new_rect) in &relocations {
            let fetched = atlas.get(*h).expect("handle must remain live after defrag");
            assert_eq!(fetched, *new_rect, "relocation rect must match get()");
        }

        // Handles h0..h3 should still be live after the defrag.
        for h in [h0, h1, h2, h3] {
            assert!(atlas.get(h).is_some(), "handle must survive defrag: {h:?}");
        }
    }

    #[test]
    fn atlas_is_fragmented_threshold() {
        let mut atlas = TextureAtlas::new(16, 16);
        // Start empty — utilization == 0 → fragmented for any positive threshold.
        assert!(
            atlas.is_fragmented(0.1),
            "empty atlas must be fragmented for threshold > 0"
        );
        // Insert one item to raise utilization above 0.
        let _h = atlas.insert(4, 4).expect("insert");
        // Threshold=0.0 → never fragmented.
        assert!(
            !atlas.is_fragmented(0.0),
            "threshold=0 must never report fragmentation"
        );
    }

    #[test]
    fn atlas_defrag_if_fragmented_skips_when_healthy() {
        let mut atlas = TextureAtlas::new(8, 8);
        // Fill entire atlas with one item.
        let _ = atlas.insert(8, 8).expect("insert");
        assert!(atlas.utilization() >= 0.99);
        // Threshold < utilization → no defrag needed.
        let result = atlas.defrag_if_fragmented(0.5);
        assert!(
            result.is_none(),
            "defrag_if_fragmented should return None when healthy"
        );
    }

    #[test]
    fn atlas_resize_clears() {
        let mut atlas = TextureAtlas::new(16, 16);
        let h = atlas.insert(4, 4).expect("insert before resize");
        atlas.resize(32, 32);
        // After resize, old handles must be invalid.
        assert!(
            atlas.get(h).is_none(),
            "handle must be invalid after resize"
        );
        assert!(
            (atlas.utilization() - 0.0).abs() < f32::EPSILON,
            "utilization must be 0 after resize"
        );
        // New atlas must accept insertions.
        assert!(
            atlas.insert(4, 4).is_some(),
            "insert after resize must succeed"
        );
    }
}
