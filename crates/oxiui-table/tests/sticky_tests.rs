use oxiui_table::{Cell, ColumnDef, RenderedCell, RowSource, Table};

// ── Minimal test data source ──────────────────────────────────────────────────

struct StickySource {
    rows: usize,
    cols: Vec<ColumnDef>,
}

impl StickySource {
    fn new(rows: usize) -> Self {
        Self {
            rows,
            cols: vec![
                ColumnDef {
                    name: "Alpha".into(),
                    width: 80.0,
                    ..ColumnDef::default()
                },
                ColumnDef {
                    name: "Beta".into(),
                    width: 120.0,
                    ..ColumnDef::default()
                },
            ],
        }
    }
}

impl RowSource for StickySource {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_sticky_headers_default_off() {
    let table = Table::new(StickySource::new(100));
    assert!(
        !table.sticky_headers(),
        "sticky_headers should default to false"
    );
}

#[test]
fn test_with_sticky_headers_sets_flag() {
    let table = Table::new(StickySource::new(100)).with_sticky_headers(true);
    assert!(
        table.sticky_headers(),
        "with_sticky_headers(true) should set the flag"
    );
}

#[test]
fn test_header_height_default() {
    let table = Table::new(StickySource::new(10));
    assert_eq!(
        table.header_height(),
        32.0,
        "header_height should default to 32.0"
    );
}

#[test]
fn test_sticky_header_render_at_y0() {
    // With sticky_headers=true and v_scroll=50.0, the header must still be at y=0.
    let table = Table::new(StickySource::new(1000))
        .with_sticky_headers(true)
        .with_row_height(24.0);

    let v_scroll = 50.0_f32;
    let origin_y = table.header_origin_y(v_scroll);
    assert_eq!(
        origin_y, 0.0,
        "sticky header origin_y should be 0.0 regardless of v_scroll={v_scroll}"
    );

    // Also verify render_header places cells at y=0.
    let cells: Vec<RenderedCell> = table.render_header(origin_y);
    assert!(
        !cells.is_empty(),
        "render_header should return at least one cell"
    );
    for cell in &cells {
        assert_eq!(
            cell.y, 0.0,
            "sticky header cell '{}' should be at y=0.0, got y={}",
            cell.text, cell.y
        );
    }
}

#[test]
fn test_sticky_header_data_row_clamped() {
    // With sticky_headers=true and v_scroll=0.0, data row 0 starts at header_height (32.0),
    // not at 0.0 — the header occupies the top band.
    let table = Table::new(StickySource::new(1000))
        .with_sticky_headers(true)
        .with_row_height(24.0);

    let v_scroll = 0.0_f32;
    let row0_y = table.data_row_origin_y(0, v_scroll);
    assert_eq!(
        row0_y,
        table.header_height(),
        "data row 0 with sticky headers should start at header_height={}, got {row0_y}",
        table.header_height()
    );
}

#[test]
fn test_visible_range_accounts_for_header() {
    // With sticky_headers=true, visible_range should use (viewport_height - header_height)
    // as the effective data area, so fewer data rows fit.
    let row_height = 24.0_f32;
    let viewport_height = 200.0_f32;
    let header_height = 32.0_f32;
    let overscan = 0;

    let table_plain = Table::new(StickySource::new(1000))
        .with_row_height(row_height)
        .with_overscan(overscan);

    let table_sticky = Table::new(StickySource::new(1000))
        .with_row_height(row_height)
        .with_overscan(overscan)
        .with_sticky_headers(true)
        .with_header_height(header_height);

    let range_plain = table_plain.visible_range(viewport_height, 0.0);
    let range_sticky = table_sticky.visible_range(viewport_height, 0.0);

    // The sticky table must show strictly fewer rows than the plain table
    // because the header consumes header_height pixels of viewport space.
    let count_plain = range_plain.len();
    let count_sticky = range_sticky.len();
    assert!(
        count_sticky < count_plain,
        "sticky table should show fewer rows ({count_sticky}) than plain table ({count_plain})"
    );

    // The sticky table's effective data height is (viewport_height - header_height).
    let expected_rows = ((viewport_height - header_height) / row_height).ceil() as usize;
    assert_eq!(
        count_sticky, expected_rows,
        "sticky table should show ~{expected_rows} rows for effective height {}, got {count_sticky}",
        viewport_height - header_height
    );
}
