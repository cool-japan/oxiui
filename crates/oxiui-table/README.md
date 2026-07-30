# oxiui-table — Virtualized table / data-grid widget for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-table.svg)](https://crates.io/crates/oxiui-table)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-table` is the table / data-grid widget of the COOLJAPAN OxiUI toolkit. It provides a `Table<S>` driven by a `RowSource` trait with **viewport-based virtualization**: only the rows visible in the current scroll window (plus a small overscan) are materialized per frame, keeping memory and CPU usage constant regardless of total row count. On top of that core it layers sorting, per-column filtering, pagination, multi-row selection, keyboard navigation, sticky/movable/pinned/resizable columns, zebra striping, tree/grouped rows, footer aggregates, cell formatting and alignment, clipboard (TSV) export, CSV export, async/prefetched data loading, and a typed cell model.

The data/logic layer is `#![forbid(unsafe_code)]` and 100% Pure Rust with no mandatory rendering dependency — it is fully unit-testable headlessly. Optional rendering backends for **egui** and **iced**, plus optional cross-crate bridges for rich text (`text-table`), theming (`theme-table`), accessibility (`a11y-table`), and state persistence (`persist-table`), are all gated behind feature flags.

## Installation

```toml
[dependencies]
# Headless core only (default):
oxiui-table = "0.2.1"

# With the egui rendering backend:
oxiui-table = { version = "0.2.1", features = ["egui-table"] }

# With the iced rendering backend:
oxiui-table = { version = "0.2.1", features = ["iced-table"] }
```

## Quick Start

Implement `RowSource` for your data, build a `Table`, and materialize only the visible rows:

```rust
use oxiui_table::{Table, RowSource, Cell, ColumnDef};

struct MyData;

impl RowSource for MyData {
    fn row_count(&self) -> usize { 1000 }
    fn row(&self, i: usize) -> Vec<Cell> {
        vec![Cell::Int(i as i64), Cell::Text(format!("row-{i}"))]
    }
    fn column_defs(&self) -> &[ColumnDef] { &[] }
}

// 240 px viewport at the default 24 px row height -> ~10 visible rows.
let table = Table::new(MyData).with_row_height(24.0);
let visible = table.materialize_visible(240.0, 0.0);
assert!(visible.len() <= 20);
```

### Sorting and filtering

```rust
# use oxiui_table::{Table, RowSource, Cell, ColumnDef};
# struct MyData;
# impl RowSource for MyData {
#     fn row_count(&self) -> usize { 3 }
#     fn row(&self, i: usize) -> Vec<Cell> { vec![Cell::Int(i as i64)] }
#     fn column_defs(&self) -> &[ColumnDef] { &[] }
# }
let mut table = Table::new(MyData);
table.toggle_sort(0);                       // cycle the sort on column 0
table.set_column_filter(0, "1".to_string()); // substring filter on column 0
let view: Vec<usize> = table.filtered_sorted_indices();
let _ = view;
```

## API Overview

### Core widget (crate root)

| Item | Description |
|------|-------------|
| `Table<S, Msg>` | The virtualized table. Builder: `with_row_height`, `with_row_heights`, `with_page_size`, `with_zebra_striping`, `with_column_order`, `with_overscan`, `with_sticky_headers`, `with_header_height`, `with_column_align`, `with_column_sortable`, `with_pinned_columns`, `with_row_background`. Behaviour: `materialize_visible`, `render_header`, `toggle_sort`, `sort_state`, `sorted_indices`, `set_column_filter`, `filtered_sorted_indices`, `resize_column`, `visible_column_range`, message hooks `on_message`/`dispatch_message`, row cache `cache_row_at_offset`/`get_or_fetch_row`/`invalidate_row_cache` |
| `RenderedCell` | A positioned, formatted cell ready to draw |
| `RowSource` (trait) | Data provider: required `row_count`, `row`, `column_defs`; defaulted `set_cell`, `row_height`, `children`, `indent_level`, `footer`. Blanket-impl for `Box<dyn RowSource>` |
| `CellRenderer` (trait) | Custom display string for `Cell::Custom` (`Debug + Send`) |
| `DEFAULT_ROW_HEIGHT` | `24.0` logical pixels |

### `Cell` enum — typed cell model

`Text`, `Int`, `Float`, `Bool`, `Empty`, `Date(i64 unix-ms → ISO-8601)`, `Currency { amount_cents, code }`, `Link { label, url }`, `Image { uri }`, `Custom(Box<dyn CellRenderer>)`. Implements `Display`, a custom `Clone` (custom cells fall back to `Empty`), `is_empty`, and a panic-free total-order `compare` (cross-type comparisons use a stable type rank). `From` impls for `&str`, `String`, `i64`, `i32`, `f64`, `bool`.

### `ColumnDef` and builder

`ColumnDef` — `name`, `width`, `min_width`, `max_width`, `resizable`, optional `formatter`, optional `align`. Construct with `ColumnDef::new(name)` or the fluent `ColumnDefBuilder` (`width`, `min_width`, `max_width`, `resizable`, `formatter`, `align`, `build`).

### `TableBuilder`

Fluent builder for a `Table` from a `RowSource`: `new(source)`, `page_size`, `zebra_striping`, `build`.

### Aggregate helpers

`aggregate_sum(cells)`, `aggregate_count(cells)`, `aggregate_avg(cells)` — sum / non-empty count / average over numeric cells.

### `TableEvent` enum

Interaction events for the caller: `RowSelected(usize)`, `CellEdited { row, col, new_value }`, `SortChanged { col, ascending }`, `ColumnResized { col, new_width }`, `FilterChanged { col, new_filter }`.

### `sort` module

`SortState` + `SortDirection` (`Ascending`, `Descending`, `None`; `next()` cycles). `sort_indices(source, …)` returns sorted row indices.

### `filter` module

`ColumnFilter` — substring filter on a column: `new(column, pattern)`, `is_inactive`, `matches`, `apply`. Free functions `filter_indices(source, predicate)` and `apply_all(source, filters)`.

### `selection` module

`SelectionModel` + `SelectionMode` (`None`, `Single`, `Multi`). `click`, `ctrl_click`, `shift_click` (range from anchor), `select_all`, `is_selected`, `selected_sorted`, `clear`.

### `nav` module

`TableNav` — keyboard cursor: `move_up/down/left/right`, `move_home_row`, `move_end_row`, `page_up`, `page_down`, `set_position`.

### `pagination` module

`PaginationState` — `new(total_rows, page_size)`, `total_pages`, `go_to`, `next`/`prev`/`first`/`last`, `row_range`, `apply(sorted_indices)`.

### `header` module

| Item | Description |
|------|-------------|
| `HeaderSortState` | Per-column sort tracking; `toggle`, `indicator` (▲/▼ glyph), `as_sort_state` |
| `TableIndex` | Cached sort/filter index with dirty tracking; `invalidate_sort`/`invalidate_filter`, `sort_index`, `filter_index` |
| `move_column(order, from, to)` | Reorder the column-order vector |
| `handle_row_click(...)` | Translate a click into selection / sort intent |

### `height` & `height_cache` modules

| Item | Description |
|------|-------------|
| `CumulativeHeights` | Prefix-sum of row heights; `build`, `total_height`, `row_at_offset`, `visible_range` |
| `CumulativeHeightCache` | Mutable, dirty-tracked variant; `set_heights`, `set_uniform_height`, `mark_dirty`, `row_at_offset`, `row_y_range`, `visible_range`, `total_height` |
| `RowCache` | Bounded cache of materialized rows; `new(max)`, `get`, `insert`, `invalidate` |

### `format` & `align` modules

`CellFormatter` (trait) with `DefaultFormatter`, `NumberFormatter`, `DateFormatter`. `CellAlign` (`Left`, `Center`, `Right`; `default_for(cell)` — numbers default right).

### `clipboard` module

`ClipboardSink` (trait) with `NullClipboard` and `CaptureClipboard` (records writes for tests). `selection_to_tsv(cells)` serializes a cell grid to tab-separated values.

### `csv` module

`to_csv(source, delimiter)` — export the whole `RowSource` to a CSV/DSV string.

### Backend integration (feature-gated)

| Item | Feature | Description |
|------|---------|-------------|
| `EguiTableState` | `egui-table` | egui `ScrollArea::show_rows` rendering state |
| `render_iced`, `render_iced_with_filters` | `iced-table` | iced `scrollable` + windowed `column` renderers |

### `async_source` module — async data loading

Always available (no feature required). Re-exported at the crate root.

| Item | Description |
|------|-------------|
| `AsyncRowSource` (trait) | `Send` data source with `row_async(index) -> BoxFuture<Result<Vec<Cell>, TableError>>` |
| `PrefetchBuffer<S>` | `RowSource`-implementing LRU wrapper around an `AsyncRowSource`; `new(source, max_rows, prefetch_ahead)`, `request_prefetch`, `store_row`, `invalidate`, `cached_count`, `is_cached`, `source()` |
| `BoxFuture<T>` | Boxed, pinned future type alias used by `AsyncRowSource` |

### `text_integration` module (`text-table` feature)

`RichCell`/`StyledSpan`/`CellRichExt` are always compiled (so non-text callers are unaffected), but shaping needs the feature.

| Item | Description |
|------|-------------|
| `RichCell` | Cell content as a sequence of `StyledSpan`s; `new`, `plain(text)`, `push_span`, `spans`, `plain_text`, `to_plain_cell` |
| `StyledSpan` | `{ text, style: oxiui_text::TextStyle }` under the feature; `{ text }` only otherwise |
| `RichCell::shape_spans(&mut TextPipeline)` | *(feature-only)* Shape all spans, returning per-span `ShapedText` |
| `RichCell::measure(&mut TextPipeline)` | *(feature-only)* Combined `(width, height)` bounding box in logical pixels |
| `CellRichExt` (trait) | `to_rich_cell()` — wrap any `Cell`'s display string into a single-span `RichCell`; blanket-implemented for `Cell` |

### `theme_integration` module (`theme-table` feature)

| Item | Description |
|------|-------------|
| `TableTheme` | RGBA colour tokens (`header_bg/fg`, `row_bg`, `row_stripe_bg`, `selection_bg/fg`, `border_color`, `focus_ring_color`, `cell_fg`, `footer_bg/fg`, `cell_padding_x/y`, `focus_radius`); `Default` is the COOLJAPAN Tokyo Night palette and needs no feature |
| `TableTheme::from_palette(&Palette, Option<&DesignTokens>)` | *(feature-only)* Derive from an `oxiui_core::Palette` / `oxiui_theme::DesignTokens` |
| `TableTheme::from_tokens(&DesignTokens)` | *(feature-only)* Derive directly from `DesignTokens` |
| `is_dark()` / `effective_row_bg(row_index, is_selected, zebra)` | Always available; alpha-blend selection over zebra/normal row colour |

### `accessibility` module (`a11y-table` feature)

A dependency-free `LightNode` tree is always available; the full AccessKit-compatible tree needs the feature.

| Item | Description |
|------|-------------|
| `LightNode` / `A11yRole` | Dependency-free a11y tree node (`id`, `role`, `label`, `description`, `is_selected`, `children`) / role enum (`Group`, `ColumnHeader`, `TableRow`, `TableCell`) |
| `build_table_a11y_tree(&TableA11yParams)` | Build a `LightNode` tree: rows, cells, column headers, selection state |
| `build_table_a11y_with_text(&TableA11yWithTextParams)` | Same, with cell text content included |
| `build_table_a11y_full(row_count, col_count, col_headers)` | *(feature-only)* Full `oxiui_accessibility::A11yNode` tree, 1:1 `WidgetRole` mapping |
| `build_table_a11y_full_with_text(...)` | *(feature-only)* Full tree variant with cell text and `selected_rows` |

### `persistence` module (`persist-table` feature)

| Item | Description |
|------|-------------|
| `TableState` | Serialisable UI-state snapshot: `column_widths`, `column_order`, `sort_column`/`sort_ascending`, `column_filters`, `current_page`/`page_size`, `pinned_columns`, `zebra_striping`; `from_table_fields(...)` |
| `TableState::encode_to_vec()` / `decode_from_slice(bytes)` | *(feature-only)* `oxicode`-based binary round-trip (no `bincode`) |
| `TableStateDiff` | Sparse diff between two `TableState`s |
| `diff(old, new) -> TableStateDiff` / `apply_diff(state, &diff)` | Compute / apply an incremental state update |

## Feature Flags

| Feature | Default | Pulls in | Description |
|---------|---------|----------|-------------|
| `egui-table` | off | `egui`, `eframe` | egui rendering backend (`EguiTableState`) |
| `iced-table` | off | `iced` | iced rendering backend (`render_iced*`) |
| `text-table` | off | `oxiui-text` | Rich-text shaping/measurement on `RichCell` (`shape_spans`, `measure`) |
| `theme-table` | off | `oxiui-theme` | Derive `TableTheme` from `oxiui-theme` design tokens / `oxiui-core::Palette` |
| `a11y-table` | off | `oxiui-accessibility`, `accesskit` | Full AccessKit-compatible `A11yNode` accessibility tree |
| `persist-table` | off | `oxicode` | Binary encode/decode of `TableState` via the COOLJAPAN `oxicode` codec |

The default build has **no** rendering dependency and no optional integration dependency — the entire sort/filter/selection/virtualization core, plus the dependency-free parts of `text_integration`/`theme_integration`/`accessibility`/`persistence`, are headless and feature-free.

## Error variants — `TableError`

| Variant | Description |
|---------|-------------|
| `ReadOnly` | The data source does not support mutation |
| `OutOfBounds { row, col }` | The coordinate is outside the source's bounds |
| `InvalidValue(String)` | The supplied value is not valid for this cell |

Returned by `RowSource::set_cell`. Implements `Display` and `std::error::Error`.

## Related crates

- [`oxiui-core`](https://crates.io/crates/oxiui-core) — the only mandatory dependency
- [`oxiui`](https://crates.io/crates/oxiui) — the OxiUI facade
- [`oxiui-accessibility`](https://crates.io/crates/oxiui-accessibility) — full a11y tree for table rows/cells/headers, behind `a11y-table`
- [`oxiui-text`](https://crates.io/crates/oxiui-text) — rich-text span shaping/measurement for `RichCell`, behind `text-table`
- [`oxiui-theme`](https://crates.io/crates/oxiui-theme) — design tokens for `TableTheme::from_palette`/`from_tokens`, behind `theme-table`
- `oxicode` — COOLJAPAN binary codec used by `TableState::encode_to_vec`/`decode_from_slice`, behind `persist-table`
- [`oxiui-egui`](https://crates.io/crates/oxiui-egui) / [`oxiui-iced`](https://crates.io/crates/oxiui-iced) — the adapters matching the `egui-table` / `iced-table` backends

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
