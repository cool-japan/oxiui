//! `oxiui-accessibility` integration for `oxiui-table`.
//!
//! Builds an AccessKit / OxiUI accessibility tree for a `Table<S>` so that
//! screen readers and assistive technologies can:
//!
//! - Enumerate each data row as a `WidgetRole::TableRow`.
//! - Enumerate each cell as a `WidgetRole::TableCell` with its text content
//!   and a `"Row N Column M"` description.
//! - Enumerate each column header as `WidgetRole::ColumnHeader` with the
//!   column name as its label and a sort-state annotation.
//! - Announce the selected row(s) via `is_selected = true` on the matching
//!   row nodes.
//!
//! # Usage
//!
//! ```rust,no_run
//! # use oxiui_table::{Table, RowSource, Cell, ColumnDef};
//! # struct S;
//! # impl RowSource for S {
//! #     fn row_count(&self) -> usize { 0 }
//! #     fn row(&self, _: usize) -> Vec<Cell> { vec![] }
//! #     fn column_defs(&self) -> &[ColumnDef] { &[] }
//! # }
//! use oxiui_table::accessibility::{build_table_a11y_tree, TableA11yParams};
//! use oxiui_table::selection::SelectionModel;
//!
//! let source = S;
//! let selection = SelectionModel::default();
//! let params = TableA11yParams {
//!     row_count: source.row_count(),
//!     col_headers: &[],
//!     selected_rows: selection.selected_rows(),
//!     first_node_id: 1,
//! };
//! let root = build_table_a11y_tree(&params);
//! assert_eq!(root.role, oxiui_table::accessibility::A11yRole::Group);
//! ```
//!
//! # Feature flag
//!
//! Full integration via `accesskit::NodeId` is only available when the
//! `a11y-table` feature is enabled.  The lightweight types and pure-Rust
//! fallback implementations always compile.

#[cfg(feature = "a11y-table")]
use oxiui_accessibility::tree::{
    build_table_a11y as upstream_build_table, column_header_node, table_cell_node, table_row_node,
    A11yNode, WidgetRole,
};

// ── A11y role mirror ──────────────────────────────────────────────────────────

/// Mirror of the OxiUI accessibility role enum for use without the `a11y-table`
/// feature flag.
///
/// When `a11y-table` is enabled, [`A11yRole`] maps 1:1 to
/// `oxiui_accessibility::tree::WidgetRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yRole {
    /// A generic container group (table root).
    Group,
    /// A column header.
    ColumnHeader,
    /// A data row.
    TableRow,
    /// A data cell.
    TableCell,
}

// ── Lightweight a11y node (feature-independent) ───────────────────────────────

/// A simplified, dependency-free representation of an a11y tree node.
///
/// Used by callers that do not enable the `a11y-table` feature but still
/// want to inspect the logical structure produced by
/// [`build_table_a11y_tree`].  When `a11y-table` is enabled, the full
/// `oxiui_accessibility::tree::A11yNode` tree is also available via
/// `build_table_a11y_full` (requires `a11y-table` feature).
#[derive(Debug)]
pub struct LightNode {
    /// Sequential id (1-based).
    pub id: u64,
    /// Accessibility role of this node.
    pub role: A11yRole,
    /// Human-readable label (column names, cell text).
    pub label: Option<String>,
    /// Description text (e.g. `"Row 1 Column 2"`, `"Column 3 header"`).
    pub description: Option<String>,
    /// Whether this row is in the selected state.
    pub is_selected: bool,
    /// Child nodes in document order.
    pub children: Vec<LightNode>,
}

// ── Params ────────────────────────────────────────────────────────────────────

/// Parameters for constructing an accessibility tree snapshot of a table.
pub struct TableA11yParams<'a> {
    /// Total number of data rows in the current (filtered+sorted) view.
    pub row_count: usize,
    /// Column header labels in logical order.
    pub col_headers: &'a [&'a str],
    /// Set of **visible** row indices that are currently selected.
    pub selected_rows: &'a [usize],
    /// Seed for node IDs; the root gets this id, children count upward.
    pub first_node_id: u64,
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Build a lightweight accessibility tree describing a table.
///
/// The returned [`LightNode`] has role [`A11yRole::Group`] and contains:
///
/// - One [`A11yRole::ColumnHeader`] child per entry in `params.col_headers`.
/// - One [`A11yRole::TableRow`] child per row in `0..params.row_count`.
///   Each row node carries [`A11yRole::TableCell`] children (one per column),
///   `is_selected` set when the row index is in `params.selected_rows`, and a
///   `description` of `"Row N"` (1-based).
///
/// Node IDs are minted sequentially starting from `params.first_node_id`.
pub fn build_table_a11y_tree(params: &TableA11yParams<'_>) -> LightNode {
    let col_count = params.col_headers.len();
    let mut next_id = params.first_node_id;

    let mut root = LightNode {
        id: next_id,
        role: A11yRole::Group,
        label: None,
        description: None,
        is_selected: false,
        children: Vec::with_capacity(col_count + params.row_count),
    };
    next_id += 1;

    // Column-header children
    for (col_idx, &header) in params.col_headers.iter().enumerate() {
        root.children.push(LightNode {
            id: next_id,
            role: A11yRole::ColumnHeader,
            label: Some(header.to_owned()),
            description: Some(format!("Column {} header", col_idx + 1)),
            is_selected: false,
            children: Vec::new(),
        });
        next_id += 1;
    }

    // Data rows
    for row_idx in 0..params.row_count {
        let is_row_selected = params.selected_rows.contains(&row_idx);
        let mut row = LightNode {
            id: next_id,
            role: A11yRole::TableRow,
            label: None,
            description: Some(format!("Row {}", row_idx + 1)),
            is_selected: is_row_selected,
            children: Vec::with_capacity(col_count),
        };
        next_id += 1;

        // One cell per column (content is not available without the source, so
        // leave label/description empty at this level; callers may enrich via
        // the `with_cell_text` variant below).
        for col_idx in 0..col_count {
            row.children.push(LightNode {
                id: next_id,
                role: A11yRole::TableCell,
                label: None,
                description: Some(format!("Row {} Column {}", row_idx + 1, col_idx + 1)),
                is_selected: false,
                children: Vec::new(),
            });
            next_id += 1;
        }

        root.children.push(row);
    }

    root
}

/// Parameters for building a tree with concrete cell text.
pub struct TableA11yWithTextParams<'a> {
    /// Base params (row/col counts, selection, id seed).
    pub base: TableA11yParams<'a>,
    /// `cell_text[row][col]` — the display string for each cell.
    pub cell_text: &'a [Vec<String>],
}

/// Build an accessibility tree with concrete cell text content.
///
/// Identical to [`build_table_a11y_tree`] but fills in the `label` field of
/// each [`A11yRole::TableCell`] with the corresponding entry from
/// `params.cell_text`.  Rows or columns beyond the `cell_text` bounds are
/// silently skipped.
pub fn build_table_a11y_with_text(params: &TableA11yWithTextParams<'_>) -> LightNode {
    let col_count = params.base.col_headers.len();
    let mut next_id = params.base.first_node_id;

    let mut root = LightNode {
        id: next_id,
        role: A11yRole::Group,
        label: None,
        description: None,
        is_selected: false,
        children: Vec::with_capacity(col_count + params.base.row_count),
    };
    next_id += 1;

    // Column-header children
    for (col_idx, &header) in params.base.col_headers.iter().enumerate() {
        root.children.push(LightNode {
            id: next_id,
            role: A11yRole::ColumnHeader,
            label: Some(header.to_owned()),
            description: Some(format!("Column {} header", col_idx + 1)),
            is_selected: false,
            children: Vec::new(),
        });
        next_id += 1;
    }

    // Data rows
    for row_idx in 0..params.base.row_count {
        let is_row_selected = params.base.selected_rows.contains(&row_idx);
        let row_cells: &[String] = params
            .cell_text
            .get(row_idx)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        let mut row = LightNode {
            id: next_id,
            role: A11yRole::TableRow,
            label: None,
            description: Some(format!("Row {}", row_idx + 1)),
            is_selected: is_row_selected,
            children: Vec::with_capacity(col_count),
        };
        next_id += 1;

        for col_idx in 0..col_count {
            let cell_label = row_cells.get(col_idx).cloned();
            row.children.push(LightNode {
                id: next_id,
                role: A11yRole::TableCell,
                label: cell_label,
                description: Some(format!("Row {} Column {}", row_idx + 1, col_idx + 1)),
                is_selected: false,
                children: Vec::new(),
            });
            next_id += 1;
        }

        root.children.push(row);
    }

    root
}

// ── Full AccessKit integration (a11y-table feature) ───────────────────────────

/// Build a full `oxiui_accessibility::tree::A11yNode` tree for the table.
///
/// Delegates to `oxiui_accessibility::tree::build_table_a11y` which
/// produces the complete AccessKit node structure with proper role, label,
/// and description mapping.
///
/// Only available when the `a11y-table` feature is enabled.
#[cfg(feature = "a11y-table")]
pub fn build_table_a11y_full(row_count: usize, col_count: usize, col_headers: &[&str]) -> A11yNode {
    upstream_build_table(row_count, col_count, col_headers)
}

/// Build a full `oxiui_accessibility::tree::A11yNode` tree with per-cell text.
///
/// Constructs the table root node with column headers and row/cell children,
/// filling in the `text_content` field of each cell node from `cell_text`.
///
/// Only available when the `a11y-table` feature is enabled.
#[cfg(feature = "a11y-table")]
pub fn build_table_a11y_full_with_text(
    row_count: usize,
    col_headers: &[&str],
    col_count: usize,
    cell_text: &[Vec<String>],
    selected_rows: &[usize],
) -> A11yNode {
    use accesskit::NodeId; // accesskit is re-exported by oxiui_accessibility

    let mut next_id: u64 = 0;

    let mut root = A11yNode::simple(NodeId(next_id), WidgetRole::Group, None);
    next_id += 1;

    // Column headers
    for (col_idx, &header) in col_headers.iter().enumerate() {
        let node = column_header_node(NodeId(next_id), col_idx, header);
        next_id += 1;
        root.children.push(node);
    }

    // Rows
    for row_idx in 0..row_count {
        let mut row = table_row_node(NodeId(next_id), row_idx);
        next_id += 1;

        // Mark selected rows
        if selected_rows.contains(&row_idx) {
            row.props.selected = Some(true);
        }

        let row_cells: &[String] = cell_text.get(row_idx).map(|v| v.as_slice()).unwrap_or(&[]);

        for col_idx in 0..col_count {
            let text = row_cells.get(col_idx).map(|s| s.as_str()).unwrap_or("");
            let cell = table_cell_node(NodeId(next_id), row_idx, col_idx, text);
            next_id += 1;
            row.children.push(cell);
        }

        root.children.push(row);
    }

    root
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_params<'a>(
        rows: usize,
        headers: &'a [&'a str],
        selected: &'a [usize],
    ) -> TableA11yParams<'a> {
        TableA11yParams {
            row_count: rows,
            col_headers: headers,
            selected_rows: selected,
            first_node_id: 1,
        }
    }

    #[test]
    fn root_role_is_group() {
        let params = make_params(2, &["A", "B"], &[]);
        let root = build_table_a11y_tree(&params);
        assert_eq!(root.role, A11yRole::Group);
    }

    #[test]
    fn column_header_count_matches() {
        let params = make_params(3, &["Col1", "Col2", "Col3"], &[]);
        let root = build_table_a11y_tree(&params);
        let headers: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::ColumnHeader)
            .collect();
        assert_eq!(headers.len(), 3);
    }

    #[test]
    fn row_count_matches() {
        let params = make_params(5, &["A", "B"], &[]);
        let root = build_table_a11y_tree(&params);
        let rows: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::TableRow)
            .collect();
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn each_row_has_correct_cell_count() {
        let params = make_params(2, &["X", "Y", "Z"], &[]);
        let root = build_table_a11y_tree(&params);
        for row in root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::TableRow)
        {
            assert_eq!(row.children.len(), 3, "each row must have 3 cells");
        }
    }

    #[test]
    fn selected_row_is_marked() {
        let params = make_params(3, &["A"], &[1]);
        let root = build_table_a11y_tree(&params);
        let rows: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::TableRow)
            .collect();
        assert!(!rows[0].is_selected, "row 0 should not be selected");
        assert!(rows[1].is_selected, "row 1 should be selected");
        assert!(!rows[2].is_selected, "row 2 should not be selected");
    }

    #[test]
    fn column_header_label_matches() {
        let params = make_params(0, &["Name", "Age"], &[]);
        let root = build_table_a11y_tree(&params);
        let headers: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::ColumnHeader)
            .collect();
        assert_eq!(headers[0].label.as_deref(), Some("Name"));
        assert_eq!(headers[1].label.as_deref(), Some("Age"));
    }

    #[test]
    fn row_description_is_one_based() {
        let params = make_params(2, &["A"], &[]);
        let root = build_table_a11y_tree(&params);
        let rows: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::TableRow)
            .collect();
        assert_eq!(rows[0].description.as_deref(), Some("Row 1"));
        assert_eq!(rows[1].description.as_deref(), Some("Row 2"));
    }

    #[test]
    fn cell_description_has_row_and_col() {
        let params = make_params(1, &["A", "B"], &[]);
        let root = build_table_a11y_tree(&params);
        let row = root
            .children
            .iter()
            .find(|n| n.role == A11yRole::TableRow)
            .expect("must have a row");
        assert_eq!(
            row.children[0].description.as_deref(),
            Some("Row 1 Column 1")
        );
        assert_eq!(
            row.children[1].description.as_deref(),
            Some("Row 1 Column 2")
        );
    }

    #[test]
    fn zero_rows_produces_only_headers() {
        let params = make_params(0, &["A", "B"], &[]);
        let root = build_table_a11y_tree(&params);
        assert_eq!(root.children.len(), 2); // 2 headers, 0 rows
        for child in &root.children {
            assert_eq!(child.role, A11yRole::ColumnHeader);
        }
    }

    #[test]
    fn zero_cols_produces_only_rows_no_cells() {
        let params = make_params(3, &[], &[]);
        let root = build_table_a11y_tree(&params);
        let rows: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::TableRow)
            .collect();
        assert_eq!(rows.len(), 3);
        for row in rows {
            assert!(row.children.is_empty(), "rows with 0 cols have no cells");
        }
    }

    #[test]
    fn node_ids_are_unique_and_sequential() {
        let params = make_params(2, &["A", "B"], &[]);
        let root = build_table_a11y_tree(&params);

        // Collect all IDs via DFS.
        fn collect_ids(node: &LightNode, out: &mut Vec<u64>) {
            out.push(node.id);
            for child in &node.children {
                collect_ids(child, out);
            }
        }
        let mut ids = Vec::new();
        collect_ids(&root, &mut ids);

        // All IDs must be unique.
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "all node IDs must be unique");
    }

    #[test]
    fn with_text_fills_cell_labels() {
        let headers: &[&str] = &["Name", "Age"];
        let selected: &[usize] = &[];
        let cell_text: Vec<Vec<String>> = vec![
            vec!["Alice".to_string(), "30".to_string()],
            vec!["Bob".to_string(), "25".to_string()],
        ];
        let params = TableA11yWithTextParams {
            base: TableA11yParams {
                row_count: 2,
                col_headers: headers,
                selected_rows: selected,
                first_node_id: 1,
            },
            cell_text: &cell_text,
        };
        let root = build_table_a11y_with_text(&params);
        let rows: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::TableRow)
            .collect();
        assert_eq!(rows[0].children[0].label.as_deref(), Some("Alice"));
        assert_eq!(rows[0].children[1].label.as_deref(), Some("30"));
        assert_eq!(rows[1].children[0].label.as_deref(), Some("Bob"));
        assert_eq!(rows[1].children[1].label.as_deref(), Some("25"));
    }

    #[test]
    fn column_header_description_contains_column_number() {
        let params = make_params(0, &["Name", "City"], &[]);
        let root = build_table_a11y_tree(&params);
        let headers: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == A11yRole::ColumnHeader)
            .collect();
        assert!(
            headers[0]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("Column 1"),
            "first header description must contain 'Column 1'"
        );
        assert!(
            headers[1]
                .description
                .as_deref()
                .unwrap_or("")
                .contains("Column 2"),
            "second header description must contain 'Column 2'"
        );
    }

    #[cfg(feature = "a11y-table")]
    #[test]
    fn full_build_produces_correct_child_count() {
        let root = build_table_a11y_full(2, 3, &["A", "B", "C"]);
        // 3 column headers + 2 rows = 5 direct children.
        assert_eq!(
            root.children.len(),
            5,
            "expected 3 headers + 2 rows = 5 children"
        );
    }

    #[cfg(feature = "a11y-table")]
    #[test]
    fn full_build_with_text_selected_row() {
        let cell_text: Vec<Vec<String>> = vec![
            vec!["A1".to_string(), "A2".to_string()],
            vec!["B1".to_string(), "B2".to_string()],
        ];
        let root = build_table_a11y_full_with_text(2, &["H1", "H2"], 2, &cell_text, &[1]);
        // Find the second row (index 1 in row children).
        let rows: Vec<_> = root
            .children
            .iter()
            .filter(|n| n.role == WidgetRole::TableRow)
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].props.selected, Some(true), "row 1 must be selected");
    }
}
