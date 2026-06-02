//! Prefix-sum cumulative height cache with O(log n) binary search for row-by-offset lookup.
//! Row caching: bounded LRU cache for last-N materialized rows.

use crate::Cell;
use std::collections::VecDeque;

/// Prefix-sum cumulative height cache.
///
/// Stores per-row heights and lazily rebuilds a prefix-sum array whenever
/// the heights are mutated.  All lookup operations are O(log n) via binary search.
pub struct CumulativeHeightCache {
    row_heights: Vec<f32>,
    /// `prefix_sums[0]` = 0; `prefix_sums[i]` = sum of `row_heights[0..i]`.
    prefix_sums: Vec<f32>,
    dirty: bool,
}

impl CumulativeHeightCache {
    /// Create an empty, dirty cache.
    pub fn new() -> Self {
        Self {
            row_heights: Vec::new(),
            prefix_sums: Vec::new(),
            dirty: true,
        }
    }

    /// Replace all row heights with `heights`. Marks the cache dirty.
    pub fn set_heights(&mut self, heights: Vec<f32>) {
        self.row_heights = heights;
        self.dirty = true;
    }

    /// Set `row_count` rows all to the same `height`. Marks the cache dirty.
    pub fn set_uniform_height(&mut self, row_count: usize, height: f32) {
        self.row_heights = vec![height; row_count];
        self.dirty = true;
    }

    /// Mark the cache dirty so the prefix-sum array is rebuilt on the next access.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn rebuild(&mut self) {
        let n = self.row_heights.len();
        self.prefix_sums = Vec::with_capacity(n + 1);
        self.prefix_sums.push(0.0_f32);
        let mut acc = 0.0_f32;
        for &h in &self.row_heights {
            acc += h;
            self.prefix_sums.push(acc);
        }
        self.dirty = false;
    }

    fn ensure_built(&mut self) {
        if self.dirty {
            self.rebuild();
        }
    }

    /// Find the row index whose vertical span contains offset `y`. O(log n).
    ///
    /// Returns `0` when the cache is empty or `y < 0`.
    pub fn row_at_offset(&mut self, y: f32) -> usize {
        self.ensure_built();
        if self.prefix_sums.is_empty() {
            return 0;
        }
        // `partition_point` returns the first index where `prefix_sums[idx] > y`.
        // Subtracting one gives the row whose top edge is at or before `y`.
        let idx = self.prefix_sums.partition_point(|&ps| ps <= y);
        let max_row = self.row_heights.len().saturating_sub(1);
        idx.saturating_sub(1).min(max_row)
    }

    /// Return the `(top, bottom)` Y-coordinate range for `row`.
    pub fn row_y_range(&mut self, row: usize) -> (f32, f32) {
        self.ensure_built();
        let start = self.prefix_sums.get(row).copied().unwrap_or(0.0);
        let end = self.prefix_sums.get(row + 1).copied().unwrap_or(start);
        (start, end)
    }

    /// Return the range of row indices at least partially visible in the viewport
    /// `[viewport_y, viewport_y + viewport_height)`.
    pub fn visible_range(
        &mut self,
        viewport_y: f32,
        viewport_height: f32,
    ) -> std::ops::Range<usize> {
        self.ensure_built();
        let start = self.row_at_offset(viewport_y);
        let end_y = viewport_y + viewport_height;
        let end_row = self.row_at_offset(end_y);
        // Include any partially-visible row at the bottom edge.
        let end = (end_row + 1).min(self.row_heights.len());
        start..end
    }

    /// Total height of all rows in logical pixels.
    pub fn total_height(&mut self) -> f32 {
        self.ensure_built();
        self.prefix_sums.last().copied().unwrap_or(0.0)
    }

    /// Number of rows in the cache.
    pub fn row_count(&self) -> usize {
        self.row_heights.len()
    }
}

impl Default for CumulativeHeightCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── RowCache ─────────────────────────────────────────────────────────────────

/// Bounded LRU row cache: stores the last `max` materialized rows, keyed by
/// row index, to avoid re-fetching identical rows from a `RowSource`.
///
/// The eviction policy is LRU-approximate: the oldest-inserted entry is
/// evicted when the cache is full.  Entries are also bumped to the back of
/// the deque on re-insertion (cache-hit update).
pub struct RowCache {
    data: VecDeque<(usize, Vec<Cell>)>,
    max: usize,
}

impl RowCache {
    /// Create a new cache that holds at most `max` rows.
    ///
    /// Setting `max` to `0` effectively disables caching (every lookup misses).
    pub fn new(max: usize) -> Self {
        Self {
            data: VecDeque::new(),
            max,
        }
    }

    /// Look up a row by index. Returns `None` on a cache miss.
    pub fn get(&self, index: usize) -> Option<&Vec<Cell>> {
        self.data
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, cells)| cells)
    }

    /// Insert or update a row in the cache.
    ///
    /// If an entry for `index` already exists it is removed first so the
    /// refreshed entry goes to the back (most-recently-used position).  When
    /// the cache is at capacity the front (least-recently-used) entry is
    /// evicted before insertion.
    pub fn insert(&mut self, index: usize, cells: Vec<Cell>) {
        if self.max == 0 {
            return;
        }
        // Remove any stale entry for this row index.
        self.data.retain(|(i, _)| *i != index);
        // Evict the oldest entry if we are at capacity.
        if self.data.len() >= self.max {
            self.data.pop_front();
        }
        self.data.push_back((index, cells));
    }

    /// Invalidate the entire cache. Call when the underlying `RowSource` changes.
    pub fn invalidate(&mut self) {
        self.data.clear();
    }

    /// Number of currently cached rows.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` when the cache holds no rows.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}
