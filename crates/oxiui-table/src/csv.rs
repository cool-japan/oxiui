//! CSV export of table data, implemented as a pure-Rust string builder.
//!
//! Follows RFC-4180-style quoting: fields containing the delimiter, a double
//! quote, or a newline are wrapped in double quotes, and embedded quotes are
//! doubled. No external CSV crate is used (COOLJAPAN Pure-Rust policy).

use crate::RowSource;

/// Escape a single field for CSV output with the given `delimiter`.
///
/// Crate-internal entry point used by `Table::csv_from_indices`.
pub(crate) fn escape_field_pub(field: &str, delimiter: char) -> String {
    escape_field(field, delimiter)
}

fn escape_field(field: &str, delimiter: char) -> String {
    let needs_quotes = field.contains(delimiter)
        || field.contains('"')
        || field.contains('\n')
        || field.contains('\r');
    if needs_quotes {
        let escaped = field.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        field.to_owned()
    }
}

/// Serialize `source` to a CSV string.
///
/// A header row is emitted from the column names (if `column_defs` is
/// non-empty), followed by one line per row. Cells are rendered via their
/// [`Display`](std::fmt::Display) impl. `delimiter` is typically `','` or
/// `'\t'`. Lines are separated by `\n`.
pub fn to_csv<S: RowSource>(source: &S, delimiter: char) -> String {
    let mut out = String::new();
    let cols = source.column_defs();

    // Header (only when column definitions are present).
    if !cols.is_empty() {
        let header: Vec<String> = cols
            .iter()
            .map(|c| escape_field(&c.name, delimiter))
            .collect();
        out.push_str(&header.join(&delimiter.to_string()));
        out.push('\n');
    }

    for i in 0..source.row_count() {
        let row = source.row(i);
        let fields: Vec<String> = row
            .iter()
            .map(|cell| escape_field(&cell.to_string(), delimiter))
            .collect();
        out.push_str(&fields.join(&delimiter.to_string()));
        out.push('\n');
    }
    out
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

    #[test]
    fn header_and_rows() {
        let d = Data {
            rows: vec![
                vec![Cell::Int(1), Cell::from("Alice")],
                vec![Cell::Int(2), Cell::from("Bob")],
            ],
            cols: vec![
                ColumnDef {
                    name: "ID".into(),
                    width: 60.0,
                    ..ColumnDef::default()
                },
                ColumnDef {
                    name: "Name".into(),
                    width: 120.0,
                    ..ColumnDef::default()
                },
            ],
        };
        let csv = to_csv(&d, ',');
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
        assert_eq!(lines[0], "ID,Name");
        assert_eq!(lines[1], "1,Alice");
        assert_eq!(lines[2], "2,Bob");
    }

    #[test]
    fn quoting_special_chars() {
        let d = Data {
            rows: vec![vec![
                Cell::from("a,b"),
                Cell::from("quote\"here"),
                Cell::from("line\nbreak"),
            ]],
            cols: vec![],
        };
        let csv = to_csv(&d, ',');
        // No header (cols empty); one data line.
        assert_eq!(csv, "\"a,b\",\"quote\"\"here\",\"line\nbreak\"\n");
    }

    #[test]
    fn tab_delimiter() {
        let d = Data {
            rows: vec![vec![Cell::from("x"), Cell::Int(5)]],
            cols: vec![],
        };
        let csv = to_csv(&d, '\t');
        assert_eq!(csv, "x\t5\n");
    }

    #[test]
    fn empty_source() {
        let d = Data {
            rows: vec![],
            cols: vec![],
        };
        assert_eq!(to_csv(&d, ','), "");
    }
}
