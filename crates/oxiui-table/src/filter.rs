//! Row filtering: predicate-based and per-column text-substring filtering.

use crate::{Cell, RowSource};

/// Compute the indices of rows in `source` for which `predicate` returns `true`.
///
/// The predicate receives the materialized row (`&[Cell]`). Returns matching
/// indices in ascending order.
pub fn filter_indices<S, F>(source: &S, mut predicate: F) -> Vec<usize>
where
    S: RowSource,
    F: FnMut(&[Cell]) -> bool,
{
    (0..source.row_count())
        .filter(|&i| {
            let row = source.row(i);
            predicate(&row)
        })
        .collect()
}

/// A case-insensitive substring filter applied to a single column.
#[derive(Clone, Debug)]
pub struct ColumnFilter {
    /// Index of the column to test.
    pub column: usize,
    /// The (lower-cased) substring to match against the cell's display string.
    pattern: String,
}

impl ColumnFilter {
    /// Construct a filter matching rows whose cell in `column` contains
    /// `pattern` (case-insensitively).
    pub fn new(column: usize, pattern: impl Into<String>) -> Self {
        Self {
            column,
            pattern: pattern.into().to_lowercase(),
        }
    }

    /// Returns `true` if the pattern is empty (i.e. matches everything).
    pub fn is_inactive(&self) -> bool {
        self.pattern.is_empty()
    }

    /// Test a single materialized row against this filter.
    pub fn matches(&self, row: &[Cell]) -> bool {
        if self.is_inactive() {
            return true;
        }
        match row.get(self.column) {
            Some(cell) => cell.to_string().to_lowercase().contains(&self.pattern),
            None => false,
        }
    }

    /// Apply this filter to `source`, returning matching row indices.
    pub fn apply<S: RowSource>(&self, source: &S) -> Vec<usize> {
        filter_indices(source, |row| self.matches(row))
    }
}

/// Combine multiple [`ColumnFilter`]s with AND semantics, returning matching
/// row indices. A row matches only if it satisfies every active filter.
pub fn apply_all<S: RowSource>(source: &S, filters: &[ColumnFilter]) -> Vec<usize> {
    filter_indices(source, |row| filters.iter().all(|f| f.matches(row)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnDef;

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
                vec![Cell::from("Alice"), Cell::Int(30)],
                vec![Cell::from("Bob"), Cell::Int(25)],
                vec![Cell::from("alfred"), Cell::Int(40)],
            ],
            cols: vec![],
        }
    }

    #[test]
    fn predicate_filter() {
        let d = data();
        let young = filter_indices(&d, |row| matches!(row[1], Cell::Int(n) if n < 35));
        assert_eq!(young, vec![0, 1]);
    }

    #[test]
    fn column_substring_case_insensitive() {
        let d = data();
        let f = ColumnFilter::new(0, "AL");
        // "Alice" and "alfred" both contain "al" case-insensitively.
        assert_eq!(f.apply(&d), vec![0, 2]);
    }

    #[test]
    fn empty_pattern_matches_all() {
        let d = data();
        let f = ColumnFilter::new(0, "");
        assert!(f.is_inactive());
        assert_eq!(f.apply(&d), vec![0, 1, 2]);
    }

    #[test]
    fn combined_and_filter() {
        let d = data();
        let filters = vec![ColumnFilter::new(0, "al"), ColumnFilter::new(1, "4")];
        // "alfred" (row 2) matches name "al" AND age containing "4".
        assert_eq!(apply_all(&d, &filters), vec![2]);
    }
}
