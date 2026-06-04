# oxiui-table TODO

## Slice 6 Plan Block (implemented 2026-05-29)

### What was done

Pure-state features added to `crates/oxiui-table/`:

| Feature | File(s) | Status |
|---|---|---|
| `PaginationState` (page-size, nav, apply) | `src/pagination.rs` | DONE |
| `CellFormatter` trait + Default/Number/Date impls | `src/format.rs` | DONE |
| `HeaderSortState` (toggle, indicator ▲/▼) | `src/header.rs` | DONE |
| `move_column` (column reorder by index) | `src/header.rs` | DONE |
| `TableIndex` (dirty-flag memoised sort+filter) | `src/header.rs` | DONE |
| `handle_row_click` (plain/ctrl/shift delegation) | `src/header.rs` | DONE |
| `Box<dyn RowSource>` blanket impl | `src/lib.rs` | DONE |
| `sort_indices` `?Sized` bound | `src/sort.rs` | DONE |
| `ColumnDef` extended (min/max_width, resizable, formatter, align) | `src/lib.rs` | DONE |
| `ColumnDefBuilder` fluent builder | `src/lib.rs` | DONE |
| `TableBuilder` fluent builder | `src/lib.rs` | DONE |
| `Table::to_csv_all` / `Table::to_csv_visible` | `src/table.rs` | DONE |
| `Table::page_size`, `zebra_striping`, `column_order` fields | `src/table.rs` | DONE |
| `Table::with_page_size`, `with_zebra_striping`, `with_column_order` | `src/table.rs` | DONE |
| `TableBuilder` propagates `page_size` + `zebra_striping` | `src/lib.rs` | DONE |
| egui: sort indicator ▲/▼ in header, clickable sort buttons, alignment per cell, zebra rows | `src/egui_table.rs` | DONE |
| iced: sort indicator ▲/▼ in header, sticky (non-scrollable) header above body | `src/iced_table.rs` | DONE |
| 28 new tests (pagination, column reorder, sort state, formatters, index, row selection, CSV, dyn, benchmark, builder) | multiple | DONE |

### Deliberately deferred (per spec)

- Cell editing (needs `RowSource` mutation design)
- Row grouping / tree table
- Variable row height
- Column pinning / freezing
- Keyboard navigation
- `TableEvent` / generic-`Msg` callback
- Async `RowSource`
- egui/iced renderer integration for sort headers, resize handles, per-column filter inputs, zebra striping, sticky iced header (renderer backends are stubs)

---

## Status
Functional virtualized table widget (~650 SLOC across core files). Provides `Table<S: RowSource>` with viewport-based row virtualization (only visible rows + overscan are materialized). Supports `Cell` types (Text/Int/Float/Bool/Empty), `ColumnDef` (name/width/min/max/resizable/formatter/align), egui backend (`render_egui` via `ScrollArea::show_rows`), and iced backend (`render_iced` via windowed `scrollable`). Column sorting, selection, filtering, CSV export, pagination, cell formatting, column reordering, memoised index, and `dyn RowSource` are all implemented as pure state modules.

## Core Implementation
- [x] Column sorting: click-to-sort header (ascending/descending/none), `SortState { column: usize, direction: SortDirection }`, `RowSource::sort_by(column, direction)` or client-side sort adapter, multi-column sort with priority chain (~200 SLOC)
- [x] Column resizing: drag column header border to resize, minimum/maximum column width constraints, `ColumnDef::resizable` flag, cursor change on hover — state tracked in `Table::column_widths`; egui drag-handle wired; iced resize deferred (no drag primitive — see follow-ups)
    - **Implemented:** per-column filter inputs (egui TextEdit::singleline + iced text_input via `render_iced_with_filters`); column resize via egui drag-handle; `Table::column_widths` runtime widths; `Table::resize_column` (clamps to min/max, honors `resizable`); `Table::filtered_sorted_indices`; `Table::set_column_filter`; `Table::column_filters`; `Table::pinned_columns` (logical pinning with bold text marker, `with_pinned_columns`); `Table::row_background` callback + `row_bg`; `TableNav` keyboard navigation state machine (arrows/Home/End/PgUp/PgDn); `ClipboardSink` trait + `NullClipboard` + `CaptureClipboard`; `selection_to_tsv`; `TableEvent` enum; `EguiTableState` render-state carrier; `render_iced_with_filters`.
    - **Files:** `src/{egui_table.rs,iced_table.rs,table.rs,lib.rs,nav.rs,clipboard.rs}`.
    - **Tests:** 35 new tests covering all new features (105 total, all passing).
- [x] Column reordering: drag column header to reorder, visual drop indicator, `column_order: Vec<usize>` mapping (`move_column` in `header.rs`)
- [x] Row selection: single-row click select, multi-row with Ctrl+click, range-select with Shift+click, `SelectionModel { selected: HashSet<usize> }`, select-all (Ctrl+A), `handle_row_click` dispatcher
- [x] **Table: cell editing, variable row height, footer/aggregate row, row grouping — all additive default RowSource methods** (implemented 2026-05-29)
  - **Goal:** expand the table model with four capabilities, each behind an additive default trait method so existing RowSource impls and the Box<T> blanket impl are untouched.
  - **Design:** add to `trait RowSource` (all with default impls returning unsupported/default):
    - `fn set_cell(&mut self,row,col,value:Cell)->Result<(),TableError>` — cell editing; edit-mode state (double-click→edit, Enter commit, Esc cancel) held in EguiTableState, NOT in the borrowed source (mirrors column-width runtime Vec pattern from round 2).
    - `fn row_height(&self,index)->f32 { DEFAULT_ROW_HEIGHT }` + CumulativeHeights prefix-sum cache for O(log n) scroll-to-row binary search.
    - `fn children(&self,row)->Option<Vec<usize>> { None }` — tree rows with expand/collapse + indent in renderer state.
    - `fn footer(&self)->Option<Vec<Cell>> { None }` — pinned aggregate row + aggregate() helpers (sum/count/avg).
  - **Files:** `src/{lib.rs,table.rs,egui_table.rs,iced_table.rs}`; new `src/edit.rs` + `src/height.rs` if needed under 2000. Examples sweep: grep RowSource/ColumnDef in crates/oxiui/examples/ and update if needed.
  - **Prerequisites:** none.
  - **Tests (~10):** set_cell default returns unsupported; editable source round-trips edit; row_height default uniform; variable heights [20,40,30] visible_range correct; prefix-sum binary search; children 2-level group; footer sum/count/avg; edit-mode commit/cancel state machine.
  - **Risk:** keeping mutation out of borrowed source is the crux. Defer: iced column resize (iced 0.14 no drag), async data loading, virtual columns.
- [x] Filtering: per-column text filter input in header, filter predicate `Fn(&Cell) -> bool`, combined AND filtering across columns, filter state management
- [x] Pagination: optional paginated mode with page-size, current-page, total-pages, next/prev/first/last navigation controls, page-size selector dropdown
- [x] Variable row height: `RowSource::row_height(index) -> f32` for heterogeneous row heights, cumulative height prefix-sum for O(log n) scroll-offset-to-row lookup (~150 SLOC)
- [x] Column pinning / freezing: `Table::pinned_columns` + `with_pinned_columns`; egui renderer marks pinned columns with bold text; true fixed-panel split deferred (needs `SidePanel`)
- [x] Row grouping / tree table: hierarchical rows with expand/collapse, indent level, group header rows, `RowSource::children(row) -> Vec<usize>` (~200 SLOC)
- [x] Cell types expansion: `Cell::Date(chrono::NaiveDate)`, `Cell::DateTime(chrono::NaiveDateTime)`, `Cell::Currency(f64, CurrencyCode)`, `Cell::Link(url, label)`, `Cell::Image(bytes)`, `Cell::Custom(Box<dyn CellRenderer>)` (~100 SLOC)
  - **Goal:** expand `Cell` enum with `Date(i64)`, `Currency{amount_cents:i64,code:String}`, `Link{label,url:String}`, `Image{uri:String}`, `Custom(Box<dyn CellRenderer>)`; add `trait CellRenderer:Debug+Send{fn render_str(&self)->String}` (planned 2026-05-29)
  - **Design:** `Date(i64)` = Unix milliseconds; `impl Display` formats as ISO-8601 "YYYY-MM-DD" computed manually (Gregorian proleptic calendar, no chrono dep); `Currency` Display = "1,234.56 USD" format (cents÷100 with decimal); `impl Display for Cell` covers all variants
  - **Files:** `crates/oxiui-table/src/lib.rs` (Cell enum + CellRenderer trait + Display impl)
  - **Tests:** Date(0)→"1970-01-01"; Date(86400000)→"1970-01-02"; Currency{12345,"EUR"}→"123.45 EUR"; Custom calls render_str; Cell::Link display shows label
  - **Risk:** manual ISO-8601 requires correct leap-year/month-day arithmetic — test edge cases (1970-01-01, 2000-02-29, 2100-02-28)
  - **DONE 2026-05-29:** implemented all 5 new variants + CellRenderer trait; ISO-8601 Gregorian calendar algorithm (no chrono); 10 new tests all passing
- [x] Cell formatting: number format (decimal places, thousands separator, prefix/suffix), date format string, custom `Fn(&Cell) -> String` formatter per column (`NumberFormatter`, `DateFormatter`, `CellFormatter` trait)
- [x] Horizontal scrolling: when total column width exceeds viewport width, horizontal scroll with synchronized header/body scroll (~80 SLOC) — column widths tracked in `Table::column_widths`; true h-scroll sync deferred
  - **Goal:** `EguiTableState.h_scroll_offset: f32`; header and body ScrollAreas share the same horizontal offset so they scroll in sync (planned 2026-05-29)
  - **Design:** in `render_egui` (egui_table.rs): header ScrollArea gets `scroll_offset(Vec2::new(state.h_scroll_offset, 0.0))`; body ScrollArea also gets the same offset; after body render: `state.h_scroll_offset = output.state.offset.x`; for iced: use `scrollable::Id` for future sync (column-resize remains deferred — iced 0.14 lacks drag primitive)
  - **Files:** `crates/oxiui-table/src/egui_table.rs`
  - **Tests:** after setting h_scroll_offset=50.0, header and body offsets both equal 50.0
  - **Risk:** egui ScrollArea may clamp the forced offset — handle by reading back the actual offset after render
  - **DONE 2026-05-29:** `EguiTableState.h_scroll_offset: f32` added; header + filter ScrollAreas seeded from shared offset; body offset written back after render
- [x] Sticky header: column headers remain visible at top during vertical scroll (already handled by egui `show_rows`, needs explicit implementation for iced) (~40 SLOC)
- [x] Row striping: alternating row background colors (zebra striping) from theme, configurable per-row background via callback — `Table::row_background` + `row_bg` + `with_row_background`
- [x] Cell alignment: per-column alignment (left/center/right), numeric columns right-aligned by default
- [x] Footer row: summary/aggregate row (sum, count, average) pinned at bottom, `RowSource::footer() -> Option<Vec<Cell>>` (~60 SLOC)
- [x] CSV export: serialize visible (or all) rows to CSV string, column headers included, configurable delimiter, `Table::to_csv_all()` / `Table::to_csv_visible()` — pure-Rust RFC-4180 quoting
- [x] Clipboard copy: copy selected rows/cells to clipboard as tab-separated text — `ClipboardSink` trait + `NullClipboard` + `CaptureClipboard` + `selection_to_tsv`; OS shortcut wiring is caller responsibility (no OS-clipboard dep)
- [x] Keyboard navigation: arrow keys to move cell/row focus, Home/End for first/last row, Page Up/Down — `TableNav` state machine with clamping

## API Improvements
- [x] `RowSource` should be object-safe (currently has `Sized` bound via generics); consider `&dyn RowSource` support — `Box<dyn RowSource>` blanket impl added; `sort_indices` is now `?Sized`
- [x] `Table` builder: `TableBuilder::new(source).page_size(50).zebra_striping(true).build()` chainable configuration
- [x] `TableEvent` enum for callbacks: `RowSelected(usize)`, `CellEdited{row,col,new_value}`, `SortChanged{col,ascending}`, `ColumnResized{col,new_width}`, `FilterChanged{col,new_filter}`
- [x] Generic message type parameter: `Table<S, Msg>` where `Msg` is the application's message type, avoiding the need for message mapping
  - **Goal:** change `Table<S: RowSource>` to `Table<S: RowSource, Msg = ()>` with default type param; add `on_message<F: FnMut(Msg) + 'static>(self, f: F) -> Self` builder; backward compatible — existing `Table::new(src)` infers `Msg=()` (planned 2026-05-29)
  - **Design:** add `Msg` phantom type parameter with `PhantomData<Msg>` in Table struct; `on_message_handler: Option<Box<dyn FnMut(Msg)>>` field; `on_message` builder sets it; existing public API unchanged; **examples sweep mandatory**: `grep -rn "Table<" crates/` — update any explicit generic annotations
  - **Files:** `crates/oxiui-table/src/table.rs`; sweep any examples or test files that reference `Table<S>` explicitly
  - **Tests:** `Table::<_,()>::new(src)` compiles; `Table::<_,String>::new(src).on_message(|m:String|{})` compiles; `on_message` handler is called when a table event occurs
  - **Risk:** adding a type parameter with default is backward-compatible at usage sites but may break explicit type annotations in tests/examples — sweep is mandatory
  - **DONE 2026-05-29:** `Table<S, Msg=()>` with `PhantomData<Msg>`, `on_message_handler` field, `on_message` builder, `dispatch_message` — all 129 existing tests pass unmodified
- [x] `ColumnDef` builder: `ColumnDefBuilder::new("Name").width(120.0).resizable().align(CellAlign::Left).build()`
- [x] `Cell` conversions: `From<&str>`, `From<i64>`, `From<f64>`, `From<bool>` for ergonomic row construction

## Testing
- [x] Virtualization: 1M-row source, viewport of 20 rows, verify exactly `20 + 2*overscan` rows materialized
- [x] Sorting: sort 100-row table by Int column ascending/descending, verify order
- [x] Multi-column sort: primary sort by column A, secondary by column B, verify tiebreaker order
- [x] Selection: single select, multi-select (Ctrl+click), range-select (Shift+click), select-all, verify `SelectionModel` state
- [x] Cell editing: enter edit mode, type new value, commit, verify `RowSource` updated
  - **Goal:** Test coverage for cell editing: entering edit mode, committing a value, and verifying the `RowSource` is updated.
  - **Design:** Part of S5 table slice. Add a test `RowSource` impl that tracks edits; call `table.begin_edit(row, col)`, set a value, call `table.commit_edit()`, assert the source reflects the new value.
  - **Files:** `crates/oxiui-table/src/table.rs` (test module) or `crates/oxiui-table/tests/table.rs`.
  - **Tests:** Cell edit enter/commit updates RowSource with the new value; commit without edit is a no-op.
  - **Risk:** None — tests against existing API.
- [x] Filtering: filter by text substring, verify filtered row count, clear filter restores all rows
- [x] Pagination: 100 rows, page-size 25, verify 4 pages, navigate to page 3, verify correct rows
- [x] Variable row height: rows with heights [20, 40, 30], verify `visible_range` correctly maps scroll offset to row indices
  - **Goal:** Test that variable row heights (e.g. [20, 40, 30]) produce correct `visible_range` output via the cumulative-height cache.
  - **Design:** Part of S5 table slice. Build a table with rows of heights [20, 40, 30]; cumulative heights are [0, 20, 60, 90]; call `visible_range(viewport_y=30, viewport_height=50)` and assert the correct row indices.
  - **Files:** `crates/oxiui-table/src/height_cache.rs` (new) + test.
  - **Tests:** heights [20,40,30] → cumulative [0,20,60,90]; row_at_offset(50)==1; visible_range correct for given viewport.
  - **Risk:** None — exercises the new prefix-sum cache.
- [x] CSV export: 5-row table, export to CSV, verify header + data lines, verify cell values match
- [x] Column reordering: swap columns 0 and 2, verify render order matches new column_order
- [x] Keyboard navigation: arrow-key focus movement, verify focused cell coordinates — `TableNav` unit tests in `nav.rs` + integration tests in `table.rs`
- [x] Benchmark: 100K rows, measure `materialize_visible` latency (should be < 1ms for 50-row viewport)

## Performance
- [x] Row caching: cache last N materialized rows, avoid re-fetching from `RowSource` when scroll position unchanged
  - **Goal:** A bounded LRU row cache so repeated accesses to the same rows avoid re-materializing from RowSource.
  - **Design:** Add a `RowCache{data: VecDeque<(usize, Vec<Cell>)>, max: usize}` keyed by row index. On access: cache hit → return cached row; cache miss → materialize from RowSource, evict oldest if full. Invalidate entire cache on RowSource change notification.
  - **Files:** `crates/oxiui-table/src/{table.rs,lib.rs}`.
  - **Tests:** Cache hit returns same row without RowSource call; evicts oldest when full; cache is invalidated on RowSource mutation.
  - **Risk:** Cache↔RowSource coherence on external mutation. Mitigated by explicit invalidation.
- [x] Sort index: maintain a pre-sorted index array instead of re-sorting all data on each frame (`TableIndex` in `header.rs`)
- [x] Filter index: maintain a filtered row-index list, avoid scanning all rows each frame when filter is active (`TableIndex` in `header.rs`)
- [x] Virtual column rendering: for tables with many columns (50+), only render columns visible in the horizontal viewport
- [x] Async data loading: `AsyncRowSource` trait with `fn row_async(index) -> BoxFuture<Result<Vec<Cell>,TableError>>`, `PrefetchBuffer<S>` LRU cache wrapper implementing `RowSource`, `request_prefetch`/`flush_pending` API — 13 tests in `async_source.rs` (done 2026-06-03)
- [x] Cumulative height cache: prefix-sum array for variable-height rows, O(log n) binary search for scroll-to-row
  - **Goal:** Row vertical positioning is O(log n) via a prefix-sum cumulative-height cache supporting binary-search `row_at_offset`.
  - **Design:** New `CumulativeHeightCache{heights: Vec<f32>, prefix_sums: Vec<f32>, dirty: bool}`. `rebuild(row_heights)` computes prefix sums. `row_at_offset(y) -> usize` uses `partition_point` (std binary search). `visible_range(viewport_y, viewport_height) -> Range<usize>` calls `row_at_offset` twice. Invalidation: `mark_dirty()` + lazy rebuild on next access.
  - **Files:** new `crates/oxiui-table/src/height_cache.rs`; `crates/oxiui-table/src/{table.rs,lib.rs}`.
  - **Tests:** heights [20,40,30] → cumulative [0,20,60,90]; row_at_offset(50)==1; row_at_offset(0)==0; binary search over 10k rows returns correct index; visible_range for a viewport.
  - **Risk:** Incremental invalidation correctness. Mitigated: rebuild-on-dirty (lazy, not incremental) in v1; incremental as follow-up.

## Integration
- [x] `oxiui-core` integration: `Table` should implement the `Widget` trait, rendering through `UiCtx` generically (not directly through egui/iced)
- [x] `oxiui-text` integration: cell text rendered through `TextPipeline` for consistent CJK/emoji display, rich text cells with styled spans — `src/text_integration.rs`: `RichCell` (multi-span), `StyledSpan`, `CellRichExt` trait; `shape_spans`/`measure` via `TextPipeline` behind `text-table` feature; 11 tests (CJK/emoji/plain/typed cell conversions) (done 2026-06-03)
- [x] `oxiui-theme` integration: row striping colors, header background, selection highlight, focus ring colors from theme `DesignTokens` — `src/theme_integration.rs`: `TableTheme` RGBA colour tokens with `Default` (Tokyo Night), `from_palette` (builds from `oxiui_core::Palette` + optional `DesignTokens`), `from_tokens`, `effective_row_bg` (alpha-blend selection over zebra/normal rows); `is_dark()` helper; 13 tests; behind `theme-table` feature (done 2026-06-03)
- [x] `oxiui-egui` integration: expand `render_egui` with column sorting headers (clickable, ▲/▼ indicator), cell alignment, zebra striping
- [x] `oxiui-iced` integration: expand `render_iced` with sorting indicators (▲/▼) and sticky header above scrollable body; `scrollable::Id`-based offset tracking deferred to a future revision
- [x] `oxiui-accessibility` integration: each row as `WidgetRole::TableRow`, each cell as `WidgetRole::TableCell`, column headers as labeled, selected rows announced — `src/accessibility.rs`: `LightNode`/`A11yRole` (dependency-free), `build_table_a11y_tree` + `build_table_a11y_with_text` (pure-Rust tree with row/cell/header nodes, `is_selected` propagation); `build_table_a11y_full` + `build_table_a11y_full_with_text` (full `oxiui_accessibility::A11yNode` tree, `selected_rows` support) behind `a11y-table` feature; 14 tests (done 2026-06-03)
- [x] COOLJAPAN ecosystem: CSV export without external CSV crate (manual string building or lightweight Pure Rust CSV) ✓ already implemented; data serialization via oxicode for table state persistence; no zip/flate2 for export compression (use oxiarc-* if needed) — `src/persistence.rs`: `TableState` (`oxicode::Encode`+`Decode`) captures column widths/order/sort/filters/pagination/pinned/zebra; `TableStateDiff` + `diff`/`apply_diff` for incremental sync; `encode_to_vec`/`decode_from_slice` behind `persist-table` feature; 13 tests (done 2026-06-03)

## Proposed follow-ups

- **iced column resize:** needs a drag primitive not available in iced 0.14; defer to a future iced version or custom widget.
- **Cell editing:** needs `RowSource::set_cell` additive trait method — design decision on mutation API first.
- **Row grouping/tree table:** `RowSource::children` additive method.
- **Variable row height:** `RowSource::row_height` + prefix-sum O(log n).
- **Footer row:** `RowSource::footer` aggregate.
- **Async RowSource:** new trait + async design.
