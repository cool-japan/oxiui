#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! `oxiui-table` — Virtualized table widget for OxiUI.
//!
//! Provides a `Table<S>` widget backed by a `RowSource` trait, with viewport-based
//! virtualization: only rows visible in the current scroll window (plus a small
//! overscan) are materialized per frame. This keeps memory and CPU usage constant
//! regardless of the total row count.
//!
//! # Features
//!
//! - `egui-table` — egui `ScrollArea::show_rows` rendering backend.
//! - `iced-table` — iced `scrollable` + windowed `column` rendering backend.
//!
//! # Example
//!
//! ```rust
//! use oxiui_table::{Table, RowSource, Cell, ColumnDef};
//!
//! struct MyData;
//!
//! impl RowSource for MyData {
//!     fn row_count(&self) -> usize { 1000 }
//!     fn row(&self, i: usize) -> Vec<Cell> {
//!         vec![Cell::Int(i as i64), Cell::Text(format!("row-{i}"))]
//!     }
//!     fn column_defs(&self) -> &[ColumnDef] {
//!         &[]
//!     }
//! }
//!
//! let table = Table::new(MyData).with_row_height(24.0);
//! let visible = table.materialize_visible(240.0, 0.0);
//! assert!(visible.len() <= 20);
//! ```

mod table;

mod align;
mod csv;
mod filter;
mod selection;
mod sort;

pub mod clipboard;
pub mod format;
pub mod header;
pub mod height;
pub mod height_cache;
pub mod nav;
pub mod pagination;

#[cfg(feature = "egui-table")]
mod egui_table;

#[cfg(feature = "iced-table")]
mod iced_table;

pub use table::{RenderedCell, Table};

pub use align::CellAlign;
pub use csv::to_csv;
pub use filter::{apply_all, filter_indices, ColumnFilter};
pub use selection::{SelectionMode, SelectionModel};
pub use sort::{sort_indices, SortDirection, SortState};

pub use clipboard::{selection_to_tsv, CaptureClipboard, ClipboardSink, NullClipboard};
pub use format::{CellFormatter, DateFormatter, DefaultFormatter, NumberFormatter};
pub use header::{handle_row_click, move_column, HeaderSortState, TableIndex};
pub use height::CumulativeHeights;
pub use height_cache::{CumulativeHeightCache, RowCache};
pub use nav::TableNav;
pub use pagination::PaginationState;

#[cfg(feature = "egui-table")]
pub use egui_table::EguiTableState;

#[cfg(feature = "iced-table")]
pub use iced_table::{render_iced, render_iced_with_filters};

/// The default row height in logical pixels, used by [`RowSource::row_height`]
/// when no per-row override is provided.
pub const DEFAULT_ROW_HEIGHT: f32 = 24.0;

/// Events emitted by the table widget in response to user interaction.
///
/// Callers receive these via a callback or event list and use them to update
/// application state (e.g. persist the new sort order, broadcast a row
/// selection to other panels, etc.).
#[derive(Debug, Clone)]
pub enum TableEvent {
    /// A row was selected (by click or keyboard navigation).
    ///
    /// The value is the **visible** row index (after sort/filter).
    RowSelected(usize),
    /// A cell's value was edited and committed.
    CellEdited {
        /// Visible row index.
        row: usize,
        /// Logical column index.
        col: usize,
        /// The new cell value as a string.
        new_value: String,
    },
    /// The sort order changed.
    SortChanged {
        /// The column that was sorted.
        col: usize,
        /// `true` = ascending, `false` = descending.
        ascending: bool,
    },
    /// A column was resized by the user.
    ColumnResized {
        /// The logical column index.
        col: usize,
        /// The new column width in logical pixels.
        new_width: f32,
    },
    /// The filter text for a column changed.
    FilterChanged {
        /// The logical column index.
        col: usize,
        /// The new filter string (empty = no filter).
        new_filter: String,
    },
}

/// Errors returned by [`RowSource`] mutating operations.
#[derive(Debug, Clone, PartialEq)]
pub enum TableError {
    /// The data source does not support mutation (read-only).
    ReadOnly,
    /// The (row, col) coordinate is outside the source's bounds.
    OutOfBounds {
        /// Row index that was out of bounds.
        row: usize,
        /// Column index that was out of bounds.
        col: usize,
    },
    /// The supplied value is not valid for this cell (e.g. wrong type).
    InvalidValue(String),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableError::ReadOnly => write!(f, "table source is read-only"),
            TableError::OutOfBounds { row, col } => {
                write!(f, "cell ({row}, {col}) is out of bounds")
            }
            TableError::InvalidValue(msg) => write!(f, "invalid cell value: {msg}"),
        }
    }
}

impl std::error::Error for TableError {}

/// Trait that provides rows to the table widget.
pub trait RowSource {
    /// Returns the total number of rows in the data source.
    fn row_count(&self) -> usize;
    /// Returns the cells for row at the given index.
    fn row(&self, index: usize) -> Vec<Cell>;
    /// Returns the column definitions (name + preferred width).
    fn column_defs(&self) -> &[ColumnDef];

    /// Attempt to set a cell value in the source.
    ///
    /// The default implementation returns [`TableError::ReadOnly`], marking the
    /// source as immutable.  Override this method on mutable sources to accept
    /// edits committed from the UI.
    fn set_cell(&mut self, _row: usize, _col: usize, _value: Cell) -> Result<(), TableError> {
        Err(TableError::ReadOnly)
    }

    /// Per-row height in logical pixels.
    ///
    /// The default returns [`DEFAULT_ROW_HEIGHT`] (24 px) for every row,
    /// producing a uniform-height table.  Override for variable-height rows.
    fn row_height(&self, _index: usize) -> f32 {
        DEFAULT_ROW_HEIGHT
    }

    /// Optional children of a row, enabling tree / grouped tables.
    ///
    /// Returns `None` for leaf rows and flat (non-grouped) tables (default).
    /// Return `Some(child_indices)` for a row that acts as a parent node.
    fn children(&self, _row: usize) -> Option<Vec<usize>> {
        None
    }

    /// Indent level for a row in a tree / grouped table.
    ///
    /// Returns `0` for root-level rows (default).  Override to return `1` for
    /// first-level children, `2` for grandchildren, and so on.
    fn indent_level(&self, _row: usize) -> usize {
        0
    }

    /// Optional footer (aggregate) row displayed below the data rows.
    ///
    /// Returns `None` (no footer) by default.  Override to return a
    /// `Vec<Cell>` whose length matches `column_defs().len()`.
    fn footer(&self) -> Option<Vec<Cell>> {
        None
    }
}

/// Blanket impl so that `Box<dyn RowSource>` itself implements `RowSource`,
/// enabling object-safe polymorphic table data sources.
impl<T: RowSource + ?Sized> RowSource for Box<T> {
    fn row_count(&self) -> usize {
        (**self).row_count()
    }
    fn row(&self, index: usize) -> Vec<Cell> {
        (**self).row(index)
    }
    fn column_defs(&self) -> &[ColumnDef] {
        (**self).column_defs()
    }
    fn set_cell(&mut self, row: usize, col: usize, value: Cell) -> Result<(), TableError> {
        (**self).set_cell(row, col, value)
    }
    fn row_height(&self, index: usize) -> f32 {
        (**self).row_height(index)
    }
    fn children(&self, row: usize) -> Option<Vec<usize>> {
        (**self).children(row)
    }
    fn indent_level(&self, row: usize) -> usize {
        (**self).indent_level(row)
    }
    fn footer(&self) -> Option<Vec<Cell>> {
        (**self).footer()
    }
}

/// Trait for custom cell rendering.
///
/// Implement this to supply a custom display string for [`Cell::Custom`] variants.
/// The trait requires [`std::fmt::Debug`] and [`Send`] so that cell values can be
/// inspected and passed across thread boundaries.
pub trait CellRenderer: std::fmt::Debug + Send {
    /// Render this value as a display string.
    fn render_str(&self) -> String;
}

/// Convert Unix milliseconds since the epoch (1970-01-01) to an ISO-8601 date
/// string `"YYYY-MM-DD"` using the proleptic Gregorian calendar.
///
/// The algorithm is adapted from the Julian Day Number conversion described at
/// <https://en.wikipedia.org/wiki/Julian_day#Julian_day_number_calculation>.
/// Unix epoch day 0 equals Julian Day Number 2440588.
fn unix_ms_to_iso8601(ms: i64) -> String {
    // Truncate to whole days (floor division, handle negative ms).
    let days = if ms >= 0 {
        ms / 86_400_000
    } else {
        // For negative milliseconds, floor towards negative infinity.
        (ms - 86_399_999) / 86_400_000
    };

    // Convert Unix day offset to Julian Day Number.
    let jdn = days + 2_440_588_i64;

    let a = jdn + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d - 4800 + m / 10;

    format!("{year:04}-{month:02}-{day:02}")
}

/// A single table cell value.
#[derive(Debug)]
pub enum Cell {
    /// Text cell.
    Text(String),
    /// Integer cell.
    Int(i64),
    /// Floating-point cell.
    Float(f64),
    /// Boolean cell.
    Bool(bool),
    /// Empty / null cell.
    Empty,
    /// Date cell, stored as Unix milliseconds since 1970-01-01 00:00:00 UTC.
    ///
    /// Displayed as an ISO-8601 date string `"YYYY-MM-DD"`.
    Date(i64),
    /// Currency cell: an exact amount in the smallest currency unit (e.g. cents)
    /// together with a three-letter ISO-4217 currency code.
    ///
    /// Displayed as `"<major>.<minor02> <code>"` (e.g. `"123.45 EUR"`).
    Currency {
        /// Amount in the smallest denomination (e.g. cents for USD/EUR).
        amount_cents: i64,
        /// ISO-4217 currency code (e.g. `"USD"`, `"EUR"`).
        code: String,
    },
    /// Hyperlink cell: a display label and a URL.
    ///
    /// The [`Display`](std::fmt::Display) impl shows the `label` only.
    Link {
        /// The text shown to the user.
        label: String,
        /// The destination URL (not shown in plain-text rendering).
        url: String,
    },
    /// Image cell with a URI pointing to the image resource.
    ///
    /// Displayed as `"[image: <uri>]"` in plain-text contexts.
    Image {
        /// A URI (file path, `data:` URL, `https://` URL, etc.) identifying the image.
        uri: String,
    },
    /// Custom cell backed by a [`CellRenderer`] implementation.
    ///
    /// The renderer is heap-allocated and not [`Clone`]; cloning a `Cell::Custom`
    /// is intentionally unsupported — wrap in `Arc` at the call site if needed.
    Custom(Box<dyn CellRenderer>),
}

impl Clone for Cell {
    fn clone(&self) -> Self {
        match self {
            Cell::Text(s) => Cell::Text(s.clone()),
            Cell::Int(n) => Cell::Int(*n),
            Cell::Float(v) => Cell::Float(*v),
            Cell::Bool(b) => Cell::Bool(*b),
            Cell::Empty => Cell::Empty,
            Cell::Date(ms) => Cell::Date(*ms),
            Cell::Currency { amount_cents, code } => Cell::Currency {
                amount_cents: *amount_cents,
                code: code.clone(),
            },
            Cell::Link { label, url } => Cell::Link {
                label: label.clone(),
                url: url.clone(),
            },
            Cell::Image { uri } => Cell::Image { uri: uri.clone() },
            // Custom cells cannot be cloned generically; fall back to Empty.
            Cell::Custom(_) => Cell::Empty,
        }
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cell::Text(s) => write!(f, "{s}"),
            Cell::Int(n) => write!(f, "{n}"),
            Cell::Float(v) => write!(f, "{v}"),
            Cell::Bool(b) => write!(f, "{b}"),
            Cell::Empty => Ok(()),
            Cell::Date(ms) => write!(f, "{}", unix_ms_to_iso8601(*ms)),
            Cell::Currency { amount_cents, code } => {
                let major = amount_cents / 100;
                let minor = amount_cents.abs() % 100;
                // Preserve the negative sign even when major is 0.
                if *amount_cents < 0 && major == 0 {
                    write!(f, "-0.{minor:02} {code}")
                } else {
                    write!(f, "{major}.{minor:02} {code}")
                }
            }
            Cell::Link { label, .. } => write!(f, "{label}"),
            Cell::Image { uri } => write!(f, "[image: {uri}]"),
            Cell::Custom(renderer) => write!(f, "{}", renderer.render_str()),
        }
    }
}

impl Cell {
    /// Returns `true` if this cell is [`Cell::Empty`].
    pub fn is_empty(&self) -> bool {
        matches!(self, Cell::Empty)
    }

    /// Total ordering between two cells for sorting.
    ///
    /// Same-typed cells compare naturally (numbers numerically, text
    /// lexicographically, bools `false < true`). Floats use a total order where
    /// `NaN` sorts last. Cross-type comparisons fall back to a stable rank so
    /// sorting never panics: `Empty < Bool < Int/Float < Text < Date < Currency < Link < Image < Custom`.
    pub fn compare(&self, other: &Cell) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Cell::Int(a), Cell::Int(b)) => a.cmp(b),
            (Cell::Float(a), Cell::Float(b)) => a.total_cmp(b),
            // Mixed numeric: promote to f64 and use a total order.
            (Cell::Int(a), Cell::Float(b)) => (*a as f64).total_cmp(b),
            (Cell::Float(a), Cell::Int(b)) => a.total_cmp(&(*b as f64)),
            (Cell::Text(a), Cell::Text(b)) => a.cmp(b),
            (Cell::Bool(a), Cell::Bool(b)) => a.cmp(b),
            (Cell::Empty, Cell::Empty) => Ordering::Equal,
            (Cell::Date(a), Cell::Date(b)) => a.cmp(b),
            (
                Cell::Currency {
                    amount_cents: a, ..
                },
                Cell::Currency {
                    amount_cents: b, ..
                },
            ) => a.cmp(b),
            // Cross-type: order by a stable type rank.
            _ => self.type_rank().cmp(&other.type_rank()),
        }
    }

    /// A stable rank used to order cells of differing variants.
    fn type_rank(&self) -> u8 {
        match self {
            Cell::Empty => 0,
            Cell::Bool(_) => 1,
            Cell::Int(_) => 2,
            Cell::Float(_) => 2, // numeric types share a rank
            Cell::Text(_) => 3,
            Cell::Date(_) => 4,
            Cell::Currency { .. } => 5,
            Cell::Link { .. } => 6,
            Cell::Image { .. } => 7,
            Cell::Custom(_) => 8,
        }
    }
}

impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Cell::Text(s.to_owned())
    }
}

impl From<String> for Cell {
    fn from(s: String) -> Self {
        Cell::Text(s)
    }
}

impl From<i64> for Cell {
    fn from(n: i64) -> Self {
        Cell::Int(n)
    }
}

impl From<i32> for Cell {
    fn from(n: i32) -> Self {
        Cell::Int(n as i64)
    }
}

impl From<f64> for Cell {
    fn from(v: f64) -> Self {
        Cell::Float(v)
    }
}

impl From<bool> for Cell {
    fn from(b: bool) -> Self {
        Cell::Bool(b)
    }
}

/// Column definition: name, preferred display width, and optional per-column
/// configuration.
pub struct ColumnDef {
    /// Display name shown in the column header.
    pub name: String,
    /// Preferred column width in logical pixels.
    pub width: f32,
    /// Minimum allowed column width when resizing (logical pixels).
    pub min_width: f32,
    /// Maximum allowed column width when resizing (logical pixels).
    pub max_width: f32,
    /// Whether the user may resize this column by dragging the header edge.
    pub resizable: bool,
    /// Optional custom cell formatter.  `None` falls back to [`DefaultFormatter`].
    pub formatter: Option<Box<dyn CellFormatter>>,
    /// Optional alignment override.  `None` falls back to [`CellAlign::default_for`].
    pub align: Option<CellAlign>,
}

impl Clone for ColumnDef {
    fn clone(&self) -> Self {
        // `CellFormatter` is not Clone; cloning drops the formatter and resets to default.
        Self {
            name: self.name.clone(),
            width: self.width,
            min_width: self.min_width,
            max_width: self.max_width,
            resizable: self.resizable,
            formatter: None,
            align: self.align,
        }
    }
}

impl std::fmt::Debug for ColumnDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnDef")
            .field("name", &self.name)
            .field("width", &self.width)
            .field("min_width", &self.min_width)
            .field("max_width", &self.max_width)
            .field("resizable", &self.resizable)
            .field("formatter", &self.formatter.as_ref().map(|_| "<formatter>"))
            .field("align", &self.align)
            .finish()
    }
}

impl Default for ColumnDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            width: 100.0,
            min_width: 40.0,
            max_width: 800.0,
            resizable: true,
            formatter: None,
            align: None,
        }
    }
}

impl ColumnDef {
    /// Create a column with the given display name and default settings.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

// ── ColumnDefBuilder ─────────────────────────────────────────────────────────

/// Fluent builder for [`ColumnDef`].
pub struct ColumnDefBuilder {
    inner: ColumnDef,
}

impl ColumnDefBuilder {
    /// Start building a column with the given display name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            inner: ColumnDef::new(name),
        }
    }

    /// Set the preferred display width (logical pixels).
    pub fn width(mut self, w: f32) -> Self {
        self.inner.width = w;
        self
    }

    /// Set the minimum allowed width during resizing.
    pub fn min_width(mut self, w: f32) -> Self {
        self.inner.min_width = w;
        self
    }

    /// Set the maximum allowed width during resizing.
    pub fn max_width(mut self, w: f32) -> Self {
        self.inner.max_width = w;
        self
    }

    /// Mark this column as resizable (default).
    pub fn resizable(mut self) -> Self {
        self.inner.resizable = true;
        self
    }

    /// Attach a custom cell formatter.
    pub fn formatter(mut self, f: impl CellFormatter + 'static) -> Self {
        self.inner.formatter = Some(Box::new(f));
        self
    }

    /// Set the cell alignment for this column.
    pub fn align(mut self, a: CellAlign) -> Self {
        self.inner.align = Some(a);
        self
    }

    /// Finalise and produce the [`ColumnDef`].
    pub fn build(self) -> ColumnDef {
        self.inner
    }
}

// ── Aggregate helpers ─────────────────────────────────────────────────────────

/// Sum all numeric cells in `cells`, treating [`Cell::Int`] and [`Cell::Float`]
/// as `f64`.  Non-numeric and [`Cell::Empty`] cells are skipped.
pub fn aggregate_sum(cells: &[Cell]) -> f64 {
    cells
        .iter()
        .filter_map(|c| match c {
            Cell::Int(n) => Some(*n as f64),
            Cell::Float(f) => Some(*f),
            _ => None,
        })
        .sum()
}

/// Count non-empty cells in `cells`.  [`Cell::Empty`] cells are excluded.
pub fn aggregate_count(cells: &[Cell]) -> usize {
    cells.iter().filter(|c| !matches!(c, Cell::Empty)).count()
}

/// Compute the arithmetic average of all numeric cells in `cells`.
///
/// Returns `None` if there are no numeric cells.
pub fn aggregate_avg(cells: &[Cell]) -> Option<f64> {
    let nums: Vec<f64> = cells
        .iter()
        .filter_map(|c| match c {
            Cell::Int(n) => Some(*n as f64),
            Cell::Float(f) => Some(*f),
            _ => None,
        })
        .collect();
    if nums.is_empty() {
        None
    } else {
        Some(nums.iter().sum::<f64>() / nums.len() as f64)
    }
}

// ── TableBuilder ─────────────────────────────────────────────────────────────

/// Fluent builder for creating a [`Table`] with a concrete data source.
pub struct TableBuilder<S: RowSource> {
    source: S,
    page_size: usize,
    zebra_striping: bool,
}

impl<S: RowSource> TableBuilder<S> {
    /// Start building a table backed by `source`.
    pub fn new(source: S) -> Self {
        Self {
            source,
            page_size: 50,
            zebra_striping: false,
        }
    }

    /// Set the number of rows per page for pagination.
    pub fn page_size(mut self, size: usize) -> Self {
        self.page_size = size;
        self
    }

    /// Enable or disable zebra row striping.
    pub fn zebra_striping(mut self, enabled: bool) -> Self {
        self.zebra_striping = enabled;
        self
    }

    /// Finalise and produce the [`Table`], propagating all builder settings.
    pub fn build(self) -> Table<S> {
        Table::new(self.source)
            .with_page_size(self.page_size)
            .with_zebra_striping(self.zebra_striping)
    }
}

// ── Cell type tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod cell_type_tests {
    use super::*;

    #[test]
    fn cell_date_epoch() {
        assert_eq!(format!("{}", Cell::Date(0)), "1970-01-01");
    }

    #[test]
    fn cell_date_one_day() {
        assert_eq!(format!("{}", Cell::Date(86_400_000)), "1970-01-02");
    }

    #[test]
    fn cell_date_leap_year_2000_02_29() {
        // 2000-02-29: year 2000 IS a leap year.
        // Days from 1970-01-01 to 2000-02-29:
        //   1970-2000 = 30 years, with leap years 1972,76,80,84,88,92,96,2000 = 8 leaps
        //   = 30*365 + 8 = 10958 days from 1970-01-01 to 2000-01-01 (exclusive)
        //   Wait: 1970-01-01 to 2000-01-01 = 10957 days (both endpoints: day 0 is 1970-01-01)
        //   Then Jan=31, Feb 1-29=29 more days → 31+29-1=59 days into 2000 = day 10957+59=11016
        let ms = 11_016_i64 * 86_400_000_i64;
        assert_eq!(format!("{}", Cell::Date(ms)), "2000-02-29");
    }

    #[test]
    fn cell_date_2100_02_28() {
        // 2100 is NOT a leap year (divisible by 100 but not 400).
        // Days from 1970-01-01 to 2100-02-28:
        // 130 years: 97 leap years (1972..2096 div by 4 minus 2100) = 31 leaps (1972,76,...,2096)
        // Actually leap years 1972 to 2096 step 4 = (2096-1972)/4 + 1 = 32 leaps, minus 2100=0
        // So from 1970 to 2100: 130*365 + 32 = 47450+32 = 47482 days from 1970-01-01 to 2100-01-01
        // Then Jan=31, Feb 1-28=28 days → 31+28-1=58 days → day 47482+58=47540
        let ms = 47_540_i64 * 86_400_000_i64;
        assert_eq!(format!("{}", Cell::Date(ms)), "2100-02-28");
    }

    #[test]
    fn cell_currency_display() {
        let c = Cell::Currency {
            amount_cents: 12345,
            code: "EUR".to_string(),
        };
        assert_eq!(format!("{c}"), "123.45 EUR");
    }

    #[test]
    fn cell_currency_negative() {
        let c = Cell::Currency {
            amount_cents: -100,
            code: "USD".to_string(),
        };
        assert_eq!(format!("{c}"), "-1.00 USD");
    }

    #[test]
    fn cell_currency_zero() {
        let c = Cell::Currency {
            amount_cents: 0,
            code: "GBP".to_string(),
        };
        assert_eq!(format!("{c}"), "0.00 GBP");
    }

    #[test]
    fn cell_link_shows_label() {
        let c = Cell::Link {
            label: "Click here".to_string(),
            url: "https://example.com".to_string(),
        };
        assert_eq!(format!("{c}"), "Click here");
    }

    #[test]
    fn cell_image_display() {
        let c = Cell::Image {
            uri: "https://example.com/img.png".to_string(),
        };
        assert_eq!(format!("{c}"), "[image: https://example.com/img.png]");
    }

    #[test]
    fn cell_custom_delegates_to_render_str() {
        #[derive(Debug)]
        struct MyRenderer;
        impl CellRenderer for MyRenderer {
            fn render_str(&self) -> String {
                "custom".to_string()
            }
        }
        let c = Cell::Custom(Box::new(MyRenderer));
        assert_eq!(format!("{c}"), "custom");
    }
}
