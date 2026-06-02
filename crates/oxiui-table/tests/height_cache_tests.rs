//! Tests for `CumulativeHeightCache`, `RowCache`, and virtual column rendering.

use oxiui_table::{Cell, ColumnDef, CumulativeHeightCache, RowCache, RowSource, Table};

// ── CumulativeHeightCache tests ───────────────────────────────────────────────

#[test]
fn test_cumulative_heights_basic() {
    let mut cache = CumulativeHeightCache::new();
    cache.set_heights(vec![20.0, 40.0, 30.0]);

    assert_eq!(cache.row_at_offset(0.0), 0, "y=0 → row 0");
    assert_eq!(cache.row_at_offset(19.9), 0, "y=19.9 → row 0");
    assert_eq!(cache.row_at_offset(20.0), 1, "y=20.0 → row 1 starts");
    assert_eq!(cache.row_at_offset(50.0), 1, "y=50.0 → still row 1");
    assert_eq!(cache.row_at_offset(60.0), 2, "y=60.0 → row 2 starts");
    assert!(
        (cache.total_height() - 90.0).abs() < f32::EPSILON,
        "total height = 20+40+30 = 90"
    );
}

#[test]
fn test_visible_range() {
    // Heights [20, 40, 30] → prefix sums [0, 20, 60, 90].
    // viewport_y=30 (inside row 1), viewport_height=50 → end_y=80 (inside row 2).
    // Should cover rows 1 and 2 → 1..3.
    let mut cache = CumulativeHeightCache::new();
    cache.set_heights(vec![20.0, 40.0, 30.0]);

    let range = cache.visible_range(30.0, 50.0);
    assert_eq!(range, 1..3, "visible_range(30, 50) should be 1..3");
}

#[test]
fn test_binary_search_10k_rows() {
    // 10,000 rows each 25 px tall → total height = 250,000 px.
    // Last row starts at 249,975.0.
    let mut cache = CumulativeHeightCache::new();
    cache.set_uniform_height(10_000, 25.0);

    // One pixel before the very end should land in the last row (9999).
    let last_row = cache.row_at_offset(10_000_f32 * 25.0 - 1.0);
    assert_eq!(last_row, 9999, "near-end offset → last row 9999");

    assert_eq!(cache.row_at_offset(0.0), 0, "offset 0 → row 0");
}

#[test]
fn test_variable_row_height_visible_range() {
    // Heights [20, 40, 30] → prefix sums [0, 20, 60, 90].
    // viewport_y=25 (inside row 1, which spans 20..60),
    // viewport_height=30 → end_y=55 (still inside row 1 since row 1 ends at 60).
    // visible_range should at least contain row 1.
    let mut cache = CumulativeHeightCache::new();
    cache.set_heights(vec![20.0, 40.0, 30.0]);

    let range = cache.visible_range(25.0, 30.0);
    assert!(range.contains(&1), "range {range:?} must contain row 1");
}

#[test]
fn test_row_y_range() {
    let mut cache = CumulativeHeightCache::new();
    cache.set_heights(vec![20.0, 40.0, 30.0]);

    let (start, end) = cache.row_y_range(1);
    assert!((start - 20.0).abs() < f32::EPSILON, "row 1 starts at y=20");
    assert!((end - 60.0).abs() < f32::EPSILON, "row 1 ends at y=60");
}

#[test]
fn test_total_height_empty_cache() {
    let mut cache = CumulativeHeightCache::new();
    assert!(
        (cache.total_height() - 0.0).abs() < f32::EPSILON,
        "empty cache → total_height=0"
    );
}

// ── RowCache tests ────────────────────────────────────────────────────────────

#[test]
fn test_row_cache_hit_and_evict() {
    // Capacity of 2: insert rows 0 and 1, then 2 → evicts 0.
    let mut cache = RowCache::new(2);
    cache.insert(0, vec![Cell::Int(0)]);
    cache.insert(1, vec![Cell::Int(1)]);
    // Both present.
    assert!(cache.get(0).is_some(), "row 0 should be cached");
    assert!(cache.get(1).is_some(), "row 1 should be cached");

    // Inserting row 2 should evict row 0 (oldest).
    cache.insert(2, vec![Cell::Int(2)]);
    assert!(cache.get(0).is_none(), "row 0 should have been evicted");
    assert!(cache.get(1).is_some(), "row 1 should still be cached");
    assert!(cache.get(2).is_some(), "row 2 should be cached");
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_row_cache_invalidate() {
    let mut cache = RowCache::new(10);
    cache.insert(0, vec![Cell::Empty]);
    cache.insert(1, vec![Cell::Empty]);
    cache.insert(2, vec![Cell::Empty]);
    assert_eq!(cache.len(), 3);

    cache.invalidate();
    assert!(cache.is_empty(), "cache should be empty after invalidate");
}

#[test]
fn test_row_cache_update_moves_to_back() {
    // Re-inserting an existing key should refresh it (bump to back).
    let mut cache = RowCache::new(2);
    cache.insert(0, vec![Cell::Int(0)]);
    cache.insert(1, vec![Cell::Int(1)]);
    // Re-insert row 0 → it goes to the back; row 1 is now the oldest.
    cache.insert(0, vec![Cell::Int(99)]);
    // Insert row 2 → should evict row 1 (oldest), not row 0.
    cache.insert(2, vec![Cell::Int(2)]);
    assert!(
        cache.get(0).is_some(),
        "row 0 (refreshed) should survive eviction"
    );
    assert!(cache.get(1).is_none(), "row 1 (oldest) should be evicted");
    assert!(cache.get(2).is_some(), "row 2 should be cached");
}

#[test]
fn test_row_cache_zero_capacity_never_caches() {
    let mut cache = RowCache::new(0);
    cache.insert(0, vec![Cell::Int(42)]);
    assert!(
        cache.is_empty(),
        "zero-capacity cache should never store entries"
    );
    assert!(cache.get(0).is_none());
}

// ── Table integration tests ───────────────────────────────────────────────────

struct SimpleSource {
    rows: usize,
    cols: Vec<ColumnDef>,
}

impl SimpleSource {
    fn new(rows: usize) -> Self {
        Self {
            rows,
            cols: vec![ColumnDef {
                name: "V".into(),
                width: 60.0,
                ..ColumnDef::default()
            }],
        }
    }
}

impl RowSource for SimpleSource {
    fn row_count(&self) -> usize {
        self.rows
    }
    fn row(&self, i: usize) -> Vec<Cell> {
        vec![Cell::Int(i as i64)]
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
}

#[test]
fn test_table_cache_row_at_offset_uniform() {
    // 5 rows × 20 px each → row starts at 0, 20, 40, 60, 80.
    let mut table = Table::new(SimpleSource::new(5)).with_row_height(20.0);
    assert_eq!(table.cache_row_at_offset(0.0), 0);
    assert_eq!(table.cache_row_at_offset(19.9), 0);
    assert_eq!(table.cache_row_at_offset(20.0), 1);
    assert_eq!(table.cache_row_at_offset(40.0), 2);
    assert_eq!(table.cache_row_at_offset(80.0), 4);
}

#[test]
fn test_table_cache_visible_range_uniform() {
    // 10 rows × 24 px → total 240 px.
    // viewport_y=24, viewport_h=72 → end_y=96 → rows 1,2,3,4 visible.
    let mut table = Table::new(SimpleSource::new(10)).with_row_height(24.0);
    let range = table.cache_visible_range(24.0, 72.0);
    assert!(range.start <= 1, "range should start at or before row 1");
    assert!(range.end >= 4, "range should include at least row 4");
    assert!(range.end <= 10);
}

#[test]
fn test_table_with_row_heights() {
    // Variable heights: [20, 40, 30].
    let mut table = Table::new(SimpleSource::new(3)).with_row_heights(vec![20.0, 40.0, 30.0]);
    assert_eq!(table.cache_row_at_offset(0.0), 0);
    assert_eq!(table.cache_row_at_offset(20.0), 1);
    assert_eq!(table.cache_row_at_offset(60.0), 2);
}

#[test]
fn test_table_get_or_fetch_row_caches() {
    let mut table = Table::new(SimpleSource::new(5)).with_row_height(24.0);
    // First fetch — cache miss, materialises from source.
    let r0_first = table.get_or_fetch_row(0);
    assert!(matches!(r0_first[0], Cell::Int(0)));
    // Second fetch — should hit the cache.
    let r0_second = table.get_or_fetch_row(0);
    assert!(matches!(r0_second[0], Cell::Int(0)));
}

#[test]
fn test_table_invalidate_row_cache() {
    let mut table = Table::new(SimpleSource::new(3)).with_row_height(24.0);
    table.get_or_fetch_row(0);
    table.get_or_fetch_row(1);
    // Invalidate should clear the internal cache without panicking.
    table.invalidate_row_cache();
    // Fetching again should still work.
    let row = table.get_or_fetch_row(0);
    assert!(matches!(row[0], Cell::Int(0)));
}

// ── Cell-edit integration test ─────────────────────────────────────────────

struct MutableEditSource {
    data: Vec<Vec<Cell>>,
    cols: Vec<ColumnDef>,
}

impl MutableEditSource {
    fn new() -> Self {
        Self {
            data: vec![
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

use oxiui_table::TableError;

impl RowSource for MutableEditSource {
    fn row_count(&self) -> usize {
        self.data.len()
    }
    fn row(&self, i: usize) -> Vec<Cell> {
        self.data[i].clone()
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
    fn set_cell(&mut self, row: usize, col: usize, value: Cell) -> Result<(), TableError> {
        if row >= self.data.len() || col >= self.cols.len() {
            return Err(TableError::OutOfBounds { row, col });
        }
        self.data[row][col] = value;
        Ok(())
    }
}

#[test]
fn test_cell_edit_via_source() {
    let mut source = MutableEditSource::new();
    // Simulate an edit: set row 0, col 1 to "gamma".
    source
        .set_cell(0, 1, Cell::Text("gamma".into()))
        .expect("edit should succeed");
    let row = source.row(0);
    assert!(
        matches!(&row[1], Cell::Text(s) if s == "gamma"),
        "cell should be updated to 'gamma'"
    );

    // Commit at invalid position returns OutOfBounds.
    let err = source
        .set_cell(99, 0, Cell::Int(0))
        .expect_err("should be out of bounds");
    assert_eq!(err, TableError::OutOfBounds { row: 99, col: 0 });
}

// ── Virtual column rendering tests ────────────────────────────────────────────

/// Helper: build a source with N columns each of the given `width`.
struct UniformColSource {
    col_defs: Vec<ColumnDef>,
}

impl UniformColSource {
    fn new(n_cols: usize, width: f32) -> Self {
        let col_defs = (0..n_cols)
            .map(|i| ColumnDef {
                name: format!("C{i}"),
                width,
                ..ColumnDef::default()
            })
            .collect();
        Self { col_defs }
    }
}

impl RowSource for UniformColSource {
    fn row_count(&self) -> usize {
        1
    }
    fn row(&self, _i: usize) -> Vec<Cell> {
        self.col_defs.iter().map(|_| Cell::Empty).collect()
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.col_defs
    }
}

#[test]
fn test_visible_columns_basic() {
    // 5 columns × 100 px each → prefix = [0, 100, 200, 300, 400, 500].
    // h_scroll=150, viewport_width=200 → visible window [150, 350).
    // start: partition_point(p ≤ 150) = 2, saturating_sub(1) = 1.
    // end_raw: partition_point(p < 350) = 4 (0,100,200,300 < 350).
    // → range 1..4, i.e. columns at render positions {1, 2, 3}.
    let table = Table::new(UniformColSource::new(5, 100.0));
    let range = table.visible_column_range(150.0, 200.0);
    assert_eq!(
        range,
        1..4,
        "viewport [150,350) should expose columns 1,2,3"
    );
}

#[test]
fn test_visible_columns_full_viewport() {
    // Viewport wider than all 5 columns (500 px total, viewport 600 px).
    // h_scroll=0, so all columns must be visible → 0..5.
    let table = Table::new(UniformColSource::new(5, 100.0));
    let range = table.visible_column_range(0.0, 600.0);
    assert_eq!(range, 0..5, "full viewport should expose all 5 columns");
}

#[test]
fn test_visible_columns_scrolled_past_end() {
    // h_scroll beyond the total width (500 px) → no columns visible.
    // start = partition_point(p ≤ 600) - 1 = 6 - 1 = 5 (clamped to 5 inside n=5).
    // end_raw = partition_point(p < 600+200=800) = 6, clamped to n=5.
    // start..end = 5..5 (empty).
    let table = Table::new(UniformColSource::new(5, 100.0));
    let range = table.visible_column_range(600.0, 200.0);
    assert!(
        range.is_empty(),
        "scrolled past end → empty range, got {range:?}"
    );
}

#[test]
fn test_visible_columns_empty_source() {
    // No columns → should return 0..0 without panicking.
    let table = Table::new(UniformColSource::new(0, 100.0));
    let range = table.visible_column_range(0.0, 400.0);
    assert_eq!(range, 0..0, "zero columns → 0..0");
}

// ── Widget trait bridge test ──────────────────────────────────────────────────

/// Minimal no-op [`oxiui_core::UiCtx`] for testing.
struct NullUiCtx {
    label_count: usize,
}

impl NullUiCtx {
    fn new() -> Self {
        Self { label_count: 0 }
    }
}

use oxiui_core::{ButtonResponse, UiCtx};

impl UiCtx for NullUiCtx {
    fn heading(&mut self, _text: &str) {}
    fn label(&mut self, _text: &str) {
        self.label_count += 1;
    }
    fn button(&mut self, _label: &str) -> ButtonResponse {
        ButtonResponse::default()
    }
}

/// A simple RowSource with 3 rows and 2 columns for Widget render testing.
struct SmallSource {
    defs: Vec<ColumnDef>,
}

impl SmallSource {
    fn new() -> Self {
        Self {
            defs: vec![ColumnDef::new("A"), ColumnDef::new("B")],
        }
    }
}

impl RowSource for SmallSource {
    fn row_count(&self) -> usize {
        3
    }
    fn row(&self, i: usize) -> Vec<Cell> {
        vec![Cell::Int(i as i64), Cell::Text(format!("row-{i}"))]
    }
    fn column_defs(&self) -> &[ColumnDef] {
        &self.defs
    }
}

#[test]
fn test_table_widget_render_no_panic() {
    use oxiui_core::Widget;
    let mut table = Table::new(SmallSource::new());
    let mut ctx = NullUiCtx::new();
    // Should not panic; each row emits one label call.
    table.render(&mut ctx);
    assert_eq!(ctx.label_count, 3, "render should emit one label per row");
}
