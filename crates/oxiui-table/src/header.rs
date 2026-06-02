//! Header-level UI state: sort indicator, column reordering, and memoised
//! index computation.
//!
//! This module is intentionally decoupled from the renderer backends so that
//! both egui and iced can share the same state types.

use crate::{ColumnFilter, RowSource, SortDirection, SortState};

// ── HeaderSortState ──────────────────────────────────────────────────────────

/// UI sort state tracked per column-header click.
///
/// Unlike the lower-level [`SortState`] (which records which column and an
/// enum direction), `HeaderSortState` uses a simple `ascending: bool` toggle
/// and an `Option<usize>` for the active column, matching the UI interaction
/// model (click to sort ascending, click again to reverse).
///
/// Use [`HeaderSortState::as_sort_state`] to convert into the algorithm-level
/// [`SortState`] accepted by `sort_indices`.
#[derive(Clone, Debug, Default)]
pub struct HeaderSortState {
    /// The column currently sorted, or `None` when unsorted.
    pub column: Option<usize>,
    /// `true` for ascending, `false` for descending.
    pub ascending: bool,
}

impl HeaderSortState {
    /// Create a new, unsorted [`HeaderSortState`].
    pub fn new() -> Self {
        Self {
            column: None,
            ascending: true,
        }
    }

    /// Toggle the sort for `col`:
    /// - If `col` is not currently sorted, sort it ascending.
    /// - If `col` is currently sorted ascending, switch to descending.
    /// - If `col` is currently sorted descending, clear the sort.
    pub fn toggle(&mut self, col: usize) {
        match self.column {
            Some(c) if c == col => {
                if self.ascending {
                    self.ascending = false;
                } else {
                    // Descending → clear
                    self.column = None;
                    self.ascending = true;
                }
            }
            _ => {
                self.column = Some(col);
                self.ascending = true;
            }
        }
    }

    /// Return the indicator symbol for `col`:
    /// - `"▲"` if this column is sorted ascending
    /// - `"▼"` if this column is sorted descending
    /// - `""` otherwise
    pub fn indicator(&self, col: usize) -> &'static str {
        match self.column {
            Some(c) if c == col => {
                if self.ascending {
                    "▲"
                } else {
                    "▼"
                }
            }
            _ => "",
        }
    }

    /// Convert to the algorithm-level [`SortState`] for use with `sort_indices`.
    /// Returns `None` when unsorted.
    pub fn as_sort_state(&self) -> Option<SortState> {
        self.column.map(|col| {
            let dir = if self.ascending {
                SortDirection::Ascending
            } else {
                SortDirection::Descending
            };
            SortState::new(col, dir)
        })
    }
}

// ── Column reordering ────────────────────────────────────────────────────────

/// Apply a column move: move the column at index `from` in `order` to position
/// `to`. Other columns shift to fill the gap.
///
/// Out-of-bounds indices are silently ignored.
pub fn move_column(order: &mut Vec<usize>, from: usize, to: usize) {
    if from >= order.len() || to >= order.len() {
        return;
    }
    let col = order.remove(from);
    order.insert(to, col);
}

// ── TableIndex ───────────────────────────────────────────────────────────────

/// Memoised sort-and-filter index to avoid recomputing expensive permutations
/// on every frame.
///
/// The dirty flags track whether the cached index is still valid. Call
/// [`TableIndex::invalidate_sort`] after a sort-order change and
/// [`TableIndex::invalidate_filter`] after a filter change (sorting a different
/// column also implicitly invalidates the filter index).
#[derive(Clone, Debug)]
pub struct TableIndex {
    sort_index: Vec<usize>,
    filter_index: Vec<usize>,
    sort_dirty: bool,
    filter_dirty: bool,
}

impl TableIndex {
    /// Create a new, empty (dirty) [`TableIndex`].
    pub fn new() -> Self {
        Self {
            sort_index: Vec::new(),
            filter_index: Vec::new(),
            sort_dirty: true,
            filter_dirty: true,
        }
    }

    /// Mark the sort index (and consequently the filter index) as stale.
    pub fn invalidate_sort(&mut self) {
        self.sort_dirty = true;
        self.filter_dirty = true;
    }

    /// Mark only the filter index as stale (sort order is unchanged).
    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    /// Returns `true` if the sort index needs to be recomputed.
    pub fn is_sort_dirty(&self) -> bool {
        self.sort_dirty
    }

    /// Returns `true` if the filter index needs to be recomputed.
    pub fn is_filter_dirty(&self) -> bool {
        self.filter_dirty
    }

    /// Return the current sort index, recomputing it if dirty.
    ///
    /// The returned slice holds row indices in sorted order.
    pub fn sort_index(&mut self, rows: &dyn RowSource, sort: &HeaderSortState) -> &[usize] {
        if self.sort_dirty {
            self.sort_index = match sort.as_sort_state() {
                Some(st) => crate::sort_indices(rows, st.column, st.direction),
                None => (0..rows.row_count()).collect(),
            };
            self.sort_dirty = false;
        }
        &self.sort_index
    }

    /// Return the current filter index, recomputing it if dirty.
    ///
    /// The returned slice holds a subset of the sort index that passes every
    /// active column filter.
    pub fn filter_index(
        &mut self,
        rows: &dyn RowSource,
        sort: &HeaderSortState,
        filters: &[ColumnFilter],
    ) -> &[usize] {
        // Ensure sort is up-to-date first (filter depends on it).
        self.sort_index(rows, sort);
        if self.filter_dirty {
            let active: Vec<&ColumnFilter> = filters.iter().filter(|f| !f.is_inactive()).collect();
            if active.is_empty() {
                self.filter_index = self.sort_index.clone();
            } else {
                self.filter_index = self
                    .sort_index
                    .iter()
                    .copied()
                    .filter(|&i| {
                        let row = rows.row(i);
                        active.iter().all(|f| f.matches(&row))
                    })
                    .collect();
            }
            self.filter_dirty = false;
        }
        &self.filter_index
    }
}

impl Default for TableIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ── Row selection helper ─────────────────────────────────────────────────────

use crate::SelectionModel;

/// Handle a row-click event, delegating to the appropriate [`SelectionModel`]
/// method.
///
/// - `ctrl`: ctrl/cmd held (toggle).
/// - `shift`: shift held (range-select from last clicked).
/// - `last_clicked`: updated with `row` after every call.
pub fn handle_row_click(
    selection: &mut SelectionModel,
    row: usize,
    ctrl: bool,
    shift: bool,
    last_clicked: &mut Option<usize>,
) {
    if shift {
        selection.shift_click(row);
    } else if ctrl {
        selection.ctrl_click(row);
    } else {
        selection.click(row);
    }
    *last_clicked = Some(row);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, ColumnDef, SelectionMode};

    // ── Sort state tests ─────────────────────────────────────────────────────

    #[test]
    fn sort_toggle_new_column() {
        let mut s = HeaderSortState::new();
        s.toggle(0);
        assert_eq!(s.column, Some(0));
        assert!(s.ascending);
    }

    #[test]
    fn sort_toggle_same_column() {
        let mut s = HeaderSortState::new();
        s.toggle(0); // ascending
        s.toggle(0); // descending
        assert_eq!(s.column, Some(0));
        assert!(!s.ascending);
        s.toggle(0); // clear
        assert_eq!(s.column, None);
    }

    #[test]
    fn sort_indicator_active() {
        let mut s = HeaderSortState::new();
        s.toggle(0);
        assert_eq!(s.indicator(0), "▲");
        s.toggle(0);
        assert_eq!(s.indicator(0), "▼");
    }

    #[test]
    fn sort_indicator_inactive() {
        let mut s = HeaderSortState::new();
        s.toggle(0);
        assert_eq!(s.indicator(1), "");
    }

    // ── Column reorder tests ─────────────────────────────────────────────────

    #[test]
    fn move_column_forward() {
        let mut order = vec![0usize, 1, 2];
        move_column(&mut order, 0, 2);
        assert_eq!(order, vec![1, 2, 0]);
    }

    #[test]
    fn move_column_backward() {
        let mut order = vec![0usize, 1, 2];
        move_column(&mut order, 2, 0);
        assert_eq!(order, vec![2, 0, 1]);
    }

    #[test]
    fn move_column_no_op() {
        let mut order = vec![0usize, 1, 2];
        move_column(&mut order, 1, 1);
        assert_eq!(order, vec![0, 1, 2]);
    }

    // ── TableIndex tests ─────────────────────────────────────────────────────

    struct SimpleData {
        rows: Vec<Vec<Cell>>,
        cols: Vec<ColumnDef>,
    }
    impl RowSource for SimpleData {
        fn row_count(&self) -> usize {
            self.rows.len()
        }
        fn row(&self, i: usize) -> Vec<Cell> {
            self.rows[i].clone()
        }
        fn column_defs(&self) -> &[ColumnDef] {
            &self.cols
        }
    }

    fn make_data() -> SimpleData {
        SimpleData {
            rows: vec![vec![Cell::Int(3)], vec![Cell::Int(1)], vec![Cell::Int(2)]],
            cols: vec![],
        }
    }

    #[test]
    fn sort_index_invalidate() {
        let mut idx = TableIndex::new();
        // Force computation.
        let data = make_data();
        let sort = HeaderSortState::new();
        let _ = idx.sort_index(&data, &sort);
        assert!(!idx.is_sort_dirty());
        idx.invalidate_sort();
        assert!(idx.is_sort_dirty());
    }

    #[test]
    fn filter_index_invalidate() {
        let mut idx = TableIndex::new();
        let data = make_data();
        let sort = HeaderSortState::new();
        let _ = idx.filter_index(&data, &sort, &[]);
        assert!(!idx.is_filter_dirty());
        idx.invalidate_filter();
        assert!(idx.is_filter_dirty());
    }

    // ── Row selection via handle_row_click ───────────────────────────────────

    #[test]
    fn handle_click_selects_row() {
        let mut sel = SelectionModel::new(SelectionMode::Multi);
        let mut last = None;
        handle_row_click(&mut sel, 3, false, false, &mut last);
        assert!(sel.is_selected(3));
        assert_eq!(last, Some(3));
    }

    #[test]
    fn handle_ctrl_click_toggles() {
        let mut sel = SelectionModel::new(SelectionMode::Multi);
        let mut last = None;
        handle_row_click(&mut sel, 3, true, false, &mut last);
        assert!(sel.is_selected(3));
        // Second ctrl+click deselects.
        handle_row_click(&mut sel, 3, true, false, &mut last);
        assert!(!sel.is_selected(3));
    }

    #[test]
    fn handle_shift_click_range() {
        let mut sel = SelectionModel::new(SelectionMode::Multi);
        let mut last = None;
        handle_row_click(&mut sel, 2, false, false, &mut last);
        handle_row_click(&mut sel, 5, false, true, &mut last);
        assert_eq!(sel.selected_sorted(), vec![2, 3, 4, 5]);
    }
}
