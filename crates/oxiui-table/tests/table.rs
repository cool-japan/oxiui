use oxiui_table::{
    aggregate_avg, aggregate_count, aggregate_sum, selection_to_tsv, to_csv, CaptureClipboard,
    Cell, CellAlign, ClipboardSink, ColumnDef, CumulativeHeights, NullClipboard, PaginationState,
    RowSource, SelectionMode, SelectionModel, SortDirection, Table, TableError, TableEvent,
    TableNav, DEFAULT_ROW_HEIGHT,
};

/// A simple test data source with a configurable row count.
struct TestSource {
    rows: usize,
    cols: Vec<ColumnDef>,
}

impl TestSource {
    fn new(rows: usize) -> Self {
        Self {
            rows,
            cols: vec![
                ColumnDef {
                    name: "ID".into(),
                    width: 60.0,
                    ..ColumnDef::default()
                },
                ColumnDef {
                    name: "Label".into(),
                    width: 120.0,
                    ..ColumnDef::default()
                },
            ],
        }
    }
}

impl RowSource for TestSource {
    fn row_count(&self) -> usize {
        self.rows
    }

    fn row(&self, i: usize) -> Vec<Cell> {
        vec![Cell::Int(i as i64), Cell::Text(format!("row-{i}"))]
    }

    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
}

#[test]
fn virtualization_limits_materialized_rows() {
    let table = Table::new(TestSource::new(10_000)).with_row_height(24.0);
    // Viewport height 240 px = 10 rows visible; overscan = 3 on each side → ≤ 16 rows.
    // We allow a small margin and check ≤ 20 rows materialized.
    let visible = table.materialize_visible(240.0, 0.0);
    assert!(
        visible.len() <= 20,
        "Expected at most 20 rows, got {}",
        visible.len()
    );
    assert!(!visible.is_empty(), "Expected some rows to be materialized");
}

#[test]
fn column_headers_present() {
    let source = TestSource::new(5);
    let defs = source.column_defs();
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0].name, "ID");
    assert_eq!(defs[1].name, "Label");
}

#[test]
fn scroll_offset_changes_visible_range() {
    let table = Table::new(TestSource::new(10_000)).with_row_height(24.0);
    let range0 = table.visible_range(240.0, 0.0);
    // Scroll down exactly 10 rows (10 * 24.0 = 240.0 px).
    let range1 = table.visible_range(240.0, 240.0);
    assert_ne!(
        range0.start, range1.start,
        "Scroll offset should shift the visible range"
    );
    // With overscan=3 and first_raw=10, first = 10.saturating_sub(3) = 7.
    assert!(
        range1.start >= 7,
        "Should start near row 7-10, got {}",
        range1.start
    );
}

#[test]
fn row_count_delegate() {
    let table = Table::new(TestSource::new(42));
    assert_eq!(table.row_count(), 42);
}

#[test]
fn cell_display() {
    assert_eq!(Cell::Int(7).to_string(), "7");
    assert_eq!(Cell::Float(2.5).to_string(), "2.5");
    assert_eq!(Cell::Bool(true).to_string(), "true");
    assert_eq!(Cell::Text("hello".into()).to_string(), "hello");
    assert_eq!(Cell::Empty.to_string(), "");
}

#[test]
fn visible_range_clamped_to_row_count() {
    // Only 5 rows total; viewport is much larger — should not exceed row_count.
    let table = Table::new(TestSource::new(5)).with_row_height(24.0);
    let range = table.visible_range(9999.0, 0.0);
    assert!(
        range.end <= 5,
        "range.end {} exceeds row_count 5",
        range.end
    );
}

#[test]
fn large_scroll_offset_clamped() {
    let table = Table::new(TestSource::new(100)).with_row_height(24.0);
    // Scroll far past the end.
    let range = table.visible_range(240.0, 99999.0);
    assert!(
        range.end <= 100,
        "range.end {} exceeds row_count 100",
        range.end
    );
}

// ── New feature integration ─────────────────────────────────────────────────

#[test]
fn cell_from_conversions() {
    assert!(matches!(Cell::from("x"), Cell::Text(_)));
    assert!(matches!(Cell::from(String::from("y")), Cell::Text(_)));
    assert!(matches!(Cell::from(5i64), Cell::Int(5)));
    assert!(matches!(Cell::from(5i32), Cell::Int(5)));
    assert!(matches!(Cell::from(2.5f64), Cell::Float(_)));
    assert!(matches!(Cell::from(true), Cell::Bool(true)));
}

#[test]
fn toggle_sort_cycles_and_sorts() {
    let mut table = Table::new(TestSource::new(5));
    assert!(table.sort_state().is_none());
    // First toggle on column 0 = ascending.
    let st = table.toggle_sort(0).expect("ascending");
    assert_eq!(st.direction, SortDirection::Ascending);
    let asc = table.sorted_indices();
    assert_eq!(asc, vec![0, 1, 2, 3, 4]); // Int column already ascending
                                          // Second toggle = descending.
    table.toggle_sort(0);
    let desc = table.sorted_indices();
    assert_eq!(desc, vec![4, 3, 2, 1, 0]);
    // Third toggle = cleared (identity).
    assert!(table.toggle_sort(0).is_none());
    assert_eq!(table.sorted_indices(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn non_sortable_column_ignores_toggle() {
    let mut table = Table::new(TestSource::new(3)).with_column_sortable(1, false);
    assert!(!table.is_column_sortable(1));
    assert!(table.toggle_sort(1).is_none());
}

#[test]
fn column_align_override_and_default() {
    let table = Table::new(TestSource::new(1)).with_column_align(1, CellAlign::Center);
    // Column 0 has no override: Int cell defaults to Right.
    assert_eq!(table.column_align(0, &Cell::Int(1)), CellAlign::Right);
    // Column 1 has an explicit Center override regardless of cell type.
    assert_eq!(
        table.column_align(1, &Cell::from("text")),
        CellAlign::Center
    );
}

#[test]
fn csv_export_round_trip() {
    let source = TestSource::new(3);
    let csv = to_csv(&source, ',');
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 4); // header + 3 rows
    assert_eq!(lines[0], "ID,Label");
    assert_eq!(lines[1], "0,row-0");
    assert_eq!(lines[3], "2,row-2");
}

#[test]
fn selection_model_multi() {
    let mut sel = SelectionModel::new(SelectionMode::Multi);
    sel.click(1);
    sel.shift_click(3);
    assert_eq!(sel.selected_sorted(), vec![1, 2, 3]);
    sel.ctrl_click(2);
    assert_eq!(sel.selected_sorted(), vec![1, 3]);
}

// ── CSV visible / all ────────────────────────────────────────────────────────

#[test]
fn to_csv_visible_respects_pagination() {
    // 100 rows, page size 25 — visible CSV should have exactly 25 data rows + 1 header = 26 lines.
    let table = Table::new(TestSource::new(100));
    let page = PaginationState::new(100, 25);
    let csv = table.to_csv_visible(&page, &[]);
    let lines: Vec<&str> = csv.lines().collect();
    // 1 header + 25 data rows
    assert_eq!(lines.len(), 26, "Expected 26 lines, got {}", lines.len());
}

#[test]
fn to_csv_all_has_all_rows() {
    let table = Table::new(TestSource::new(50));
    let csv = table.to_csv_all(&[]);
    let lines: Vec<&str> = csv.lines().collect();
    // 1 header + 50 data rows
    assert_eq!(lines.len(), 51, "Expected 51 lines, got {}", lines.len());
}

// ── dyn RowSource ────────────────────────────────────────────────────────────

/// Verify that `Box<dyn RowSource>` compiles and delegates correctly.
#[test]
fn dyn_row_source_compiles() {
    let boxed: Box<dyn RowSource> = Box::new(TestSource::new(10));
    assert_eq!(boxed.row_count(), 10);
    let first_row = boxed.row(0);
    assert!(matches!(first_row[0], Cell::Int(0)));
    let defs = boxed.column_defs();
    assert_eq!(defs[0].name, "ID");
}

// ── Benchmark (smoke) ────────────────────────────────────────────────────────

/// Verify `TableBuilder` propagates `page_size` and `zebra_striping` to the
/// finished `Table`.
#[test]
fn table_builder_propagates_settings() {
    use oxiui_table::TableBuilder;
    let table = TableBuilder::new(TestSource::new(10))
        .page_size(7)
        .zebra_striping(true)
        .build();
    assert_eq!(table.page_size, 7);
    assert!(table.zebra_striping);
}

/// 100K row table — `materialize_visible` over a 50-row viewport should be fast
/// enough that completing in < 1 second is trivially achievable.
#[test]
fn materialize_100k_rows_under_1s() {
    use std::time::Instant;

    let table = Table::new(TestSource::new(100_000)).with_row_height(24.0);
    let start = Instant::now();
    // Viewport of 50 rows (50 * 24 = 1200 px height), starting at row 50_000.
    let rows = table.materialize_visible(1200.0, 50_000.0 * 24.0);
    let elapsed = start.elapsed();
    // The number of materialized rows should be bounded by viewport + overscan.
    assert!(!rows.is_empty());
    assert!(
        elapsed.as_secs() < 1,
        "materialize_visible took {:?}, expected < 1s",
        elapsed
    );
}

// ── Filter tests ─────────────────────────────────────────────────────────────

/// A data source with named rows for filter testing.
struct NamedSource {
    rows: Vec<(String, i64)>,
    cols: Vec<ColumnDef>,
}

impl NamedSource {
    fn new() -> Self {
        Self {
            rows: vec![
                ("Alice".to_string(), 30),
                ("Bob".to_string(), 25),
                ("Alfred".to_string(), 40),
                ("Carol".to_string(), 28),
            ],
            cols: vec![
                ColumnDef {
                    name: "Name".into(),
                    width: 100.0,
                    ..ColumnDef::default()
                },
                ColumnDef {
                    name: "Age".into(),
                    width: 60.0,
                    ..ColumnDef::default()
                },
            ],
        }
    }
}

impl RowSource for NamedSource {
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn row(&self, i: usize) -> Vec<Cell> {
        let (name, age) = &self.rows[i];
        vec![Cell::Text(name.clone()), Cell::Int(*age)]
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
}

#[test]
fn filter_edit_filters_rows() {
    let mut table = Table::new(NamedSource::new());
    // Set filter on column 0 (name) to "al" — should match Alice and Alfred.
    table.set_column_filter(0, "al".to_string());
    let indices = table.filtered_sorted_indices();
    // Rows 0 (Alice) and 2 (Alfred) match case-insensitively.
    assert_eq!(
        indices,
        vec![0, 2],
        "Expected Alice and Alfred, got indices {indices:?}"
    );
}

#[test]
fn filter_empty_string_matches_all() {
    let mut table = Table::new(NamedSource::new());
    table.set_column_filter(0, "".to_string());
    let indices = table.filtered_sorted_indices();
    assert_eq!(indices.len(), 4);
}

#[test]
fn filter_out_of_range_is_noop() {
    let mut table = Table::new(TestSource::new(5));
    // Column 99 doesn't exist — should not panic.
    table.set_column_filter(99, "anything".to_string());
    // All rows still present.
    assert_eq!(table.filtered_sorted_indices().len(), 5);
}

// ── Column resize tests ───────────────────────────────────────────────────────

#[test]
fn resize_clamps_to_min_max() {
    let mut table = Table::new(TestSource::new(3));
    // Column 0 has default min=40, max=800, width=60.
    // Shrink by 30 → 30, clamped to min=40.
    let new_w = table.resize_column(0, -30.0).expect("should resize");
    assert!((new_w - 40.0).abs() < 0.001, "Expected 40.0, got {new_w}");

    // Reset width manually through column_widths.
    table.column_widths[0] = 60.0;
    // Expand by 10000 → clamped to max=800.
    let new_w = table.resize_column(0, 10_000.0).expect("should resize");
    assert!((new_w - 800.0).abs() < 0.001, "Expected 800.0, got {new_w}");
}

#[test]
fn resize_honors_resizable_flag() {
    let cols = vec![
        ColumnDef {
            name: "Fixed".into(),
            width: 100.0,
            resizable: false,
            ..ColumnDef::default()
        },
        ColumnDef {
            name: "Resizable".into(),
            width: 100.0,
            resizable: true,
            ..ColumnDef::default()
        },
    ];
    struct TwoColSource {
        cols: Vec<ColumnDef>,
    }
    impl RowSource for TwoColSource {
        fn row_count(&self) -> usize {
            1
        }
        fn row(&self, _: usize) -> Vec<Cell> {
            vec![Cell::Int(1), Cell::Int(2)]
        }
        fn column_defs(&self) -> &[ColumnDef] {
            &self.cols
        }
    }
    let mut table = Table::new(TwoColSource { cols });
    // Non-resizable column → None.
    assert!(table.resize_column(0, 50.0).is_none());
    // Resizable column → Some(new_width).
    assert!(table.resize_column(1, 50.0).is_some());
}

#[test]
fn resize_out_of_range_returns_none() {
    let mut table = Table::new(TestSource::new(3));
    assert!(table.resize_column(99, 10.0).is_none());
}

// ── Column pinning tests ──────────────────────────────────────────────────────

#[test]
fn pinned_columns_count() {
    let table = Table::new(TestSource::new(5)).with_pinned_columns(1);
    assert_eq!(table.pinned_columns, 1);
}

#[test]
fn pinned_columns_default_zero() {
    let table = Table::new(TestSource::new(5));
    assert_eq!(table.pinned_columns, 0);
}

// ── Keyboard navigation tests ─────────────────────────────────────────────────

#[test]
fn nav_move_down_increments_row() {
    let mut nav = TableNav::new();
    assert!(nav.move_down(5));
    assert_eq!(nav.active_row, 1);
}

#[test]
fn nav_move_down_clamps_at_last_row() {
    let mut nav = TableNav::new();
    nav.active_row = 4;
    assert!(!nav.move_down(5)); // 4 is the last row (5 total)
    assert_eq!(nav.active_row, 4);
}

#[test]
fn nav_move_up_clamps_at_zero() {
    let mut nav = TableNav::new();
    assert!(!nav.move_up(5));
    assert_eq!(nav.active_row, 0);
}

#[test]
fn nav_move_left_right() {
    let mut nav = TableNav::new();
    // At col 0 — can't go left.
    assert!(!nav.move_left(3));
    // Can go right.
    assert!(nav.move_right(3));
    assert_eq!(nav.active_col, 1);
    // At last col — can't go right further.
    nav.active_col = 2;
    assert!(!nav.move_right(3));
}

#[test]
fn nav_page_up_down() {
    let mut nav = TableNav::new();
    nav.active_row = 10;
    assert!(nav.page_up(4));
    assert_eq!(nav.active_row, 6);
    assert!(nav.page_down(20, 4));
    assert_eq!(nav.active_row, 10);
    // Page down past end clamps.
    assert!(nav.page_down(12, 100));
    assert_eq!(nav.active_row, 11);
}

#[test]
fn nav_home_end() {
    let mut nav = TableNav::new();
    nav.active_row = 7;
    assert!(nav.move_home_row());
    assert_eq!(nav.active_row, 0);
    // Already at home — no change.
    assert!(!nav.move_home_row());
    assert!(nav.move_end_row(10));
    assert_eq!(nav.active_row, 9);
    // Already at end — no change.
    assert!(!nav.move_end_row(10));
}

// ── TSV clipboard tests ───────────────────────────────────────────────────────

#[test]
fn tsv_export_tab_separated() {
    let cells = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];
    let tsv = selection_to_tsv(&cells);
    assert_eq!(tsv, "a\tb\tc");
}

#[test]
fn tsv_export_newline_between_rows() {
    let cells = vec![
        vec!["1".to_string(), "2".to_string()],
        vec!["3".to_string(), "4".to_string()],
    ];
    let tsv = selection_to_tsv(&cells);
    assert_eq!(tsv, "1\t2\n3\t4");
}

#[test]
fn capture_clipboard_records_text() {
    let mut sink = CaptureClipboard::new();
    sink.copy_text("hello".to_string());
    sink.copy_text("world".to_string());
    assert_eq!(sink.captured, vec!["hello", "world"]);
}

#[test]
fn null_clipboard_does_not_panic() {
    let mut sink = NullClipboard;
    sink.copy_text("anything".to_string());
}

// ── TableEvent variant tests ──────────────────────────────────────────────────

#[test]
fn table_event_sort_changed_variant() {
    let event = TableEvent::SortChanged {
        col: 2,
        ascending: true,
    };
    assert!(matches!(
        event,
        TableEvent::SortChanged {
            col: 2,
            ascending: true
        }
    ));
}

#[test]
fn table_event_filter_changed_variant() {
    let event = TableEvent::FilterChanged {
        col: 0,
        new_filter: "test".to_string(),
    };
    assert!(matches!(event, TableEvent::FilterChanged { col: 0, .. }));
}

#[test]
fn table_event_column_resized_variant() {
    let event = TableEvent::ColumnResized {
        col: 1,
        new_width: 150.0,
    };
    assert!(matches!(event, TableEvent::ColumnResized { col: 1, .. }));
}

#[test]
fn table_event_row_selected_variant() {
    let event = TableEvent::RowSelected(5);
    assert!(matches!(event, TableEvent::RowSelected(5)));
}

// ── Per-row background callback tests ────────────────────────────────────────

#[test]
fn row_background_callback_called() {
    let table = Table::new(TestSource::new(5)).with_row_background(|row| {
        if row % 2 == 0 {
            Some([255, 0, 0, 255]) // red for even rows
        } else {
            None
        }
    });
    // Even rows have a custom background.
    assert_eq!(table.row_bg(0), Some([255, 0, 0, 255]));
    assert_eq!(table.row_bg(2), Some([255, 0, 0, 255]));
    // Odd rows return None (fall back to theme).
    assert_eq!(table.row_bg(1), None);
    assert_eq!(table.row_bg(3), None);
}

#[test]
fn row_background_none_by_default() {
    let table = Table::new(TestSource::new(3));
    assert_eq!(table.row_bg(0), None);
    assert_eq!(table.row_bg(1), None);
}

// ── filtered_sorted_indices test ──────────────────────────────────────────────

#[test]
fn filtered_sorted_returns_all_when_no_filter() {
    let table = Table::new(TestSource::new(5));
    let idx = table.filtered_sorted_indices();
    assert_eq!(idx, vec![0, 1, 2, 3, 4]);
}

#[test]
fn effective_width_uses_runtime_width() {
    let mut table = Table::new(TestSource::new(3));
    // Initial width comes from ColumnDef.
    assert!((table.effective_width(0) - 60.0).abs() < 0.001);
    // After resize, effective_width reflects the new value.
    table.resize_column(0, 20.0).expect("should resize");
    assert!((table.effective_width(0) - 80.0).abs() < 0.001);
}

// ── ClipboardSink trait object test ──────────────────────────────────────────

#[test]
fn clipboard_sink_as_trait_object() {
    let mut sink: Box<dyn ClipboardSink> = Box::new(CaptureClipboard::new());
    sink.copy_text("trait object test".to_string());
    // Downcast to verify (use Any if needed, or just test via the impl type).
    let mut sink2 = CaptureClipboard::new();
    sink2.copy_text("direct".to_string());
    assert_eq!(sink2.captured.len(), 1);
}

// ── Slice D: RowSource capability expansion ───────────────────────────────────

// 1. Cell editing: default impl returns ReadOnly.
#[test]
fn test_set_cell_default_returns_readonly() {
    let mut source = TestSource::new(3);
    let result = source.set_cell(0, 0, Cell::Int(99));
    assert_eq!(result, Err(TableError::ReadOnly));
}

// 2. Cell editing: mutable source round-trips an edit.
struct MutableSource {
    cells: Vec<Vec<Cell>>,
    cols: Vec<ColumnDef>,
}

impl MutableSource {
    fn new() -> Self {
        Self {
            cells: vec![
                vec![Cell::Int(1), Cell::Text("alpha".into())],
                vec![Cell::Int(2), Cell::Text("beta".into())],
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
        }
    }
}

impl RowSource for MutableSource {
    fn row_count(&self) -> usize {
        self.cells.len()
    }
    fn row(&self, i: usize) -> Vec<Cell> {
        self.cells[i].clone()
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
    fn set_cell(&mut self, row: usize, col: usize, value: Cell) -> Result<(), TableError> {
        if row >= self.cells.len() || col >= self.cols.len() {
            return Err(TableError::OutOfBounds { row, col });
        }
        self.cells[row][col] = value;
        Ok(())
    }
}

#[test]
fn test_set_cell_custom_source_round_trips() {
    let mut source = MutableSource::new();
    source
        .set_cell(0, 1, Cell::Text("gamma".into()))
        .expect("edit should succeed");
    let row = source.row(0);
    assert!(matches!(&row[1], Cell::Text(s) if s == "gamma"));
}

// 3. Row height: default returns DEFAULT_ROW_HEIGHT for all rows.
#[test]
fn test_row_height_default_uniform() {
    let source = TestSource::new(5);
    for i in 0..5 {
        assert!(
            (source.row_height(i) - DEFAULT_ROW_HEIGHT).abs() < f32::EPSILON,
            "row_height({i}) should be DEFAULT_ROW_HEIGHT"
        );
    }
}

// 4. CumulativeHeights: variable heights produce correct prefix sum.
struct VarHeightSource {
    heights: Vec<f32>,
    cols: Vec<ColumnDef>,
}

impl VarHeightSource {
    fn new(heights: Vec<f32>) -> Self {
        Self {
            cols: vec![ColumnDef {
                name: "H".into(),
                width: 80.0,
                ..ColumnDef::default()
            }],
            heights,
        }
    }
}

impl RowSource for VarHeightSource {
    fn row_count(&self) -> usize {
        self.heights.len()
    }
    fn row(&self, _i: usize) -> Vec<Cell> {
        vec![Cell::Empty]
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
    fn row_height(&self, index: usize) -> f32 {
        self.heights
            .get(index)
            .copied()
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }
}

#[test]
fn test_cumulative_heights_variable() {
    let source = VarHeightSource::new(vec![20.0, 40.0, 30.0]);
    let ch = CumulativeHeights::build(&source);
    // Total height = 20 + 40 + 30 = 90
    assert!((ch.total_height() - 90.0).abs() < f32::EPSILON);
}

// 5. CumulativeHeights: binary search for row at offset.
#[test]
fn test_row_at_offset_binary_search() {
    // Heights: [20, 40, 30] → cumulative boundaries: [0, 20, 60, 90]
    // offset=25 is between 20 and 60, so row 1.
    let source = VarHeightSource::new(vec![20.0, 40.0, 30.0]);
    let ch = CumulativeHeights::build(&source);
    assert_eq!(
        ch.row_at_offset(25.0),
        1,
        "offset 25 should land in row 1 (starts at 20)"
    );
    assert_eq!(ch.row_at_offset(0.0), 0, "offset 0 should land in row 0");
    assert_eq!(
        ch.row_at_offset(19.9),
        0,
        "offset 19.9 should still be row 0"
    );
    assert_eq!(ch.row_at_offset(60.0), 2, "offset 60 starts row 2");
}

// 6. CumulativeHeights: visible_range maps scroll offset correctly.
#[test]
fn test_visible_range_maps_offset() {
    // Heights: [20, 40, 30] → cumulative: [0, 20, 60, 90]
    // scroll_offset=20, viewport=50 → covers rows 1 (20-60) and 2 (60-90).
    let source = VarHeightSource::new(vec![20.0, 40.0, 30.0]);
    let ch = CumulativeHeights::build(&source);
    let range = ch.visible_range(20.0, 50.0);
    assert_eq!(range.start, 1, "visible range should start at row 1");
    assert!(range.end >= 2, "visible range should include row 2");
    assert!(range.end <= 3, "visible range must not exceed row count");
}

// 7. Row grouping: default children() returns None.
#[test]
fn test_children_default_none() {
    let source = TestSource::new(5);
    assert!(source.children(0).is_none());
    assert!(source.children(4).is_none());
    assert_eq!(source.indent_level(0), 0);
}

// 8. Expand/collapse state machine via EguiTableState.
#[cfg(feature = "egui-table")]
#[test]
fn test_tree_table_expand_collapse() {
    use oxiui_table::EguiTableState;
    let mut state = EguiTableState::default();
    assert!(state.expanded_rows.is_empty());
    state.toggle_expand(2);
    assert!(state.expanded_rows.contains(&2), "row 2 should be expanded");
    state.toggle_expand(5);
    assert!(state.expanded_rows.contains(&5));
    // Toggle expanded row back to collapsed.
    state.toggle_expand(2);
    assert!(
        !state.expanded_rows.contains(&2),
        "row 2 should be collapsed again"
    );
}

// 9. Footer: default returns None.
#[test]
fn test_footer_default_none() {
    let source = TestSource::new(5);
    assert!(source.footer().is_none());
}

// 10. Aggregate helpers: sum, count, avg on a mixed cell slice.
#[test]
fn test_aggregate_sum_count_avg() {
    let cells = vec![
        Cell::Int(10),
        Cell::Float(20.5),
        Cell::Empty,
        Cell::Text("x".into()),
    ];
    assert!((aggregate_sum(&cells) - 30.5).abs() < 1e-9);
    // count excludes Empty only; Text("x") is non-empty.
    assert_eq!(aggregate_count(&cells), 3);
    // avg is over numeric cells only: (10 + 20.5) / 2 = 15.25
    let avg = aggregate_avg(&cells).expect("should have numeric cells");
    assert!((avg - 15.25).abs() < 1e-9, "expected 15.25, got {avg}");
}

// Edit-mode state machine via EguiTableState.
#[cfg(feature = "egui-table")]
#[test]
fn test_edit_mode_commit_cancel() {
    use oxiui_table::EguiTableState;
    let mut state = EguiTableState::default();
    // Start editing.
    state.begin_edit(1, 2, "old".into());
    assert_eq!(state.edit_mode, Some((1, 2)));
    assert_eq!(state.edit_buffer, "old");
    // Modify buffer as if user typed.
    state.edit_buffer = "new".into();
    // Commit.
    let committed = state.commit_edit().expect("commit should return Some");
    assert_eq!(committed, (1, 2, "new".to_string()));
    assert!(state.edit_mode.is_none());
    assert!(state.edit_buffer.is_empty());
    // Cancel path.
    state.begin_edit(0, 0, "x".into());
    state.cancel_edit();
    assert!(state.edit_mode.is_none());
    assert!(state.edit_buffer.is_empty());
}
