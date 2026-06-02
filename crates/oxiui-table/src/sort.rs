//! Column sorting: sort state and index-based sort over a [`RowSource`].
//!
//! Sorting produces a permutation of row indices rather than mutating the
//! data source, so the original row order is always recoverable and the source
//! stays immutable.

use crate::RowSource;

/// The sort direction for a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending order (A→Z, 0→9).
    Ascending,
    /// Descending order (Z→A, 9→0).
    Descending,
    /// No sort applied (original order).
    None,
}

impl SortDirection {
    /// Cycle through the three states: `None → Ascending → Descending → None`.
    ///
    /// Useful for click-to-sort headers.
    pub fn next(self) -> SortDirection {
        match self {
            SortDirection::None => SortDirection::Ascending,
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::None,
        }
    }
}

/// The active sort: which column and in which direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortState {
    /// Index of the column being sorted.
    pub column: usize,
    /// Direction of the sort.
    pub direction: SortDirection,
}

impl SortState {
    /// Construct a sort state.
    pub fn new(column: usize, direction: SortDirection) -> Self {
        Self { column, direction }
    }
}

/// Compute the row-index permutation that sorts `source` by a single column.
///
/// Returns indices `0..row_count`. When `direction` is [`SortDirection::None`]
/// the identity order is returned. The sort is stable, so equal keys preserve
/// their relative order (enabling multi-pass / multi-column sorting by sorting
/// least-significant column first).
pub fn sort_indices<S: RowSource + ?Sized>(
    source: &S,
    column: usize,
    direction: SortDirection,
) -> Vec<usize> {
    let n = source.row_count();
    let mut indices: Vec<usize> = (0..n).collect();
    if direction == SortDirection::None {
        return indices;
    }
    // Materialize the sort key for each row once (the cell in `column`).
    let keys: Vec<_> = (0..n)
        .map(|i| {
            let row = source.row(i);
            row.into_iter().nth(column)
        })
        .collect();
    indices.sort_by(|&a, &b| {
        let ord = match (&keys[a], &keys[b]) {
            (Some(ca), Some(cb)) => ca.compare(cb),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        };
        match direction {
            SortDirection::Descending => ord.reverse(),
            _ => ord,
        }
    });
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cell, ColumnDef};

    struct Data {
        rows: Vec<Vec<Cell>>,
        cols: Vec<ColumnDef>,
    }
    impl RowSource for Data {
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

    fn data() -> Data {
        Data {
            rows: vec![
                vec![Cell::Int(3), Cell::from("c")],
                vec![Cell::Int(1), Cell::from("a")],
                vec![Cell::Int(2), Cell::from("b")],
            ],
            cols: vec![],
        }
    }

    #[test]
    fn ascending_by_int_column() {
        let d = data();
        let order = sort_indices(&d, 0, SortDirection::Ascending);
        assert_eq!(order, vec![1, 2, 0]); // rows with Int 1,2,3
    }

    #[test]
    fn descending_by_int_column() {
        let d = data();
        let order = sort_indices(&d, 0, SortDirection::Descending);
        assert_eq!(order, vec![0, 2, 1]); // 3,2,1
    }

    #[test]
    fn none_is_identity() {
        let d = data();
        let order = sort_indices(&d, 0, SortDirection::None);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn text_column_sort() {
        let d = data();
        let order = sort_indices(&d, 1, SortDirection::Ascending);
        // text a,b,c => rows 1,2,0
        assert_eq!(order, vec![1, 2, 0]);
    }

    #[test]
    fn stable_multi_column() {
        // Sort by secondary (col 1) first, then primary (col 0) — stable sort
        // yields a primary-major, secondary-minor ordering on ties.
        let d = Data {
            rows: vec![
                vec![Cell::Int(1), Cell::from("y")],
                vec![Cell::Int(1), Cell::from("x")],
                vec![Cell::Int(0), Cell::from("z")],
            ],
            cols: vec![],
        };
        let by_secondary = sort_indices(&d, 1, SortDirection::Ascending);
        // Reorder a temp view, then sort by primary stably.
        // Build a source-like permutation check: apply secondary then primary.
        let mut idx = by_secondary;
        // Stable re-sort by primary key using the same comparator.
        let keys: Vec<_> = (0..d.row_count()).map(|i| d.row(i)[0].clone()).collect();
        idx.sort_by(|&a, &b| keys[a].compare(&keys[b]));
        // Expect primary 0 first (row 2), then primary 1 rows in secondary order
        // (x before y => row 1 before row 0).
        assert_eq!(idx, vec![2, 1, 0]);
    }

    #[test]
    fn direction_cycles() {
        assert_eq!(SortDirection::None.next(), SortDirection::Ascending);
        assert_eq!(SortDirection::Ascending.next(), SortDirection::Descending);
        assert_eq!(SortDirection::Descending.next(), SortDirection::None);
    }
}
