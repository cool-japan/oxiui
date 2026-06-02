//! Prefix-sum cache for variable-height row scroll lookup.
//!
//! When a [`RowSource`] overrides [`crate::RowSource::row_height`]
//! to return per-row heights, [`CumulativeHeights`] pre-computes a prefix-sum array
//! so that scroll-offset-to-row-index translation is O(log n) instead of O(n).

use crate::RowSource;

/// Prefix-sum cache for variable-height row scroll lookups.
///
/// Build once per frame (or whenever the row set changes) with
/// [`CumulativeHeights::build`], then call [`row_at_offset`](Self::row_at_offset)
/// and [`visible_range`](Self::visible_range) as needed during rendering.
pub struct CumulativeHeights {
    /// `cumulative[i]` = sum of `row_height(0..i)`.
    /// Length is `row_count + 1`; `cumulative[0]` is always `0.0`.
    cumulative: Vec<f32>,
}

impl CumulativeHeights {
    /// Build the prefix-sum array from a [`RowSource`].
    ///
    /// Calls [`RowSource::row_height`] once per row and accumulates the results.
    pub fn build<S: RowSource + ?Sized>(source: &S) -> Self {
        let n = source.row_count();
        let mut cumulative = vec![0.0f32; n + 1];
        for i in 0..n {
            cumulative[i + 1] = cumulative[i] + source.row_height(i);
        }
        Self { cumulative }
    }

    /// Total height of all rows in logical pixels.
    pub fn total_height(&self) -> f32 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    /// Return the index of the first row whose top edge is at or before
    /// `scroll_offset`.
    ///
    /// Uses a binary search for O(log n) lookup.
    pub fn row_at_offset(&self, scroll_offset: f32) -> usize {
        match self.cumulative.partition_point(|&c| c <= scroll_offset) {
            0 => 0,
            n => (n - 1).min(self.cumulative.len().saturating_sub(2)),
        }
    }

    /// Return the range of row indices that are (at least partially) visible
    /// in a viewport starting at `scroll_offset` with height `viewport_height`.
    pub fn visible_range(
        &self,
        scroll_offset: f32,
        viewport_height: f32,
    ) -> std::ops::Range<usize> {
        let start = self.row_at_offset(scroll_offset);
        let end_offset = scroll_offset + viewport_height;
        let end = self
            .cumulative
            .partition_point(|&c| c < end_offset)
            .min(self.cumulative.len().saturating_sub(1));
        start..end
    }
}
