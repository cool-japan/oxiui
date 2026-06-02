//! Pagination state: page-size, current page, and row-range computation.

/// Tracks pagination state for a table: which page we're on and how large each
/// page is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaginationState {
    /// Number of rows per page.
    pub page_size: usize,
    /// Current page index, 0-indexed.
    pub current_page: usize,
    /// Total number of rows in the (filtered+sorted) data set.
    pub total_rows: usize,
}

impl PaginationState {
    /// Create a new [`PaginationState`] and immediately clamp `current_page`.
    pub fn new(total_rows: usize, page_size: usize) -> Self {
        let mut s = Self {
            page_size: page_size.max(1),
            current_page: 0,
            total_rows,
        };
        s.clamp();
        s
    }

    /// Total number of pages (ceiling division).
    pub fn total_pages(&self) -> usize {
        self.total_rows.div_ceil(self.page_size.max(1))
    }

    /// Clamp `current_page` to the valid range `0..total_pages().saturating_sub(1)`.
    pub fn clamp(&mut self) {
        let max = self.total_pages().saturating_sub(1);
        self.current_page = self.current_page.min(max);
    }

    /// Navigate to an arbitrary page, clamped to the valid range.
    pub fn go_to(&mut self, page: usize) {
        self.current_page = page;
        self.clamp();
    }

    /// Navigate to the next page (no-op if already on the last page).
    pub fn next(&mut self) {
        let max = self.total_pages().saturating_sub(1);
        if self.current_page < max {
            self.current_page += 1;
        }
    }

    /// Navigate to the previous page (no-op if already on page 0).
    pub fn prev(&mut self) {
        self.current_page = self.current_page.saturating_sub(1);
    }

    /// Navigate to the first page.
    pub fn first(&mut self) {
        self.current_page = 0;
    }

    /// Navigate to the last page.
    pub fn last(&mut self) {
        self.current_page = self.total_pages().saturating_sub(1);
    }

    /// The half-open row range `[start, end)` for the current page.
    ///
    /// Both `start` and `end` are clamped to `total_rows`.
    pub fn row_range(&self) -> std::ops::Range<usize> {
        let start = (self.current_page * self.page_size).min(self.total_rows);
        let end = (start + self.page_size).min(self.total_rows);
        start..end
    }

    /// Slice the subset of `sorted_indices` that belongs to the current page.
    ///
    /// Returns a sub-slice (not a copy) of the incoming index array.
    pub fn apply<'a>(&self, sorted_indices: &'a [usize]) -> &'a [usize] {
        let range = self.row_range();
        let start = range.start.min(sorted_indices.len());
        let end = range.end.min(sorted_indices.len());
        &sorted_indices[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_total_pages() {
        let p = PaginationState::new(100, 25);
        assert_eq!(p.total_pages(), 4);
    }

    #[test]
    fn pagination_page3_rows() {
        let p = PaginationState {
            page_size: 25,
            current_page: 3,
            total_rows: 100,
        };
        assert_eq!(p.row_range(), 75..100);
    }

    #[test]
    fn pagination_clamp_overflow() {
        let mut p = PaginationState::new(100, 25);
        p.go_to(999);
        assert_eq!(p.current_page, 3);
    }

    #[test]
    fn pagination_apply_visible() {
        let mut p = PaginationState::new(100, 25);
        p.go_to(2);
        let all: Vec<usize> = (0..100).collect();
        let visible = p.apply(&all);
        assert_eq!(visible, &(50..75).collect::<Vec<usize>>()[..]);
    }

    #[test]
    fn pagination_next_prev() {
        let mut p = PaginationState::new(100, 25);
        p.next();
        assert_eq!(p.current_page, 1);
        p.prev();
        assert_eq!(p.current_page, 0);
        // Prev at 0 stays at 0.
        p.prev();
        assert_eq!(p.current_page, 0);
        // Next to last page (page 3) and then no-op.
        p.go_to(3);
        p.next();
        assert_eq!(p.current_page, 3);
    }
}
