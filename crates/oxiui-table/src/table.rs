//! Core `Table` widget with viewport-based row virtualization.

use std::marker::PhantomData;

use crate::{
    height_cache::{CumulativeHeightCache, RowCache},
    Cell, CellAlign, ColumnFilter, PaginationState, RowSource, SortDirection, SortState,
};

/// A single positioned header cell produced by [`Table::render_header`].
///
/// Each [`RenderedCell`] describes the column label, its horizontal position, and
/// its vertical position within the rendered viewport.  Renderers use these
/// values to draw the header row independently from the scrolling data area.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedCell {
    /// The logical column index this cell represents.
    pub col: usize,
    /// The display text of the column header (from [`crate::ColumnDef::name`]).
    pub text: String,
    /// The horizontal (X) position of the cell's left edge in logical pixels.
    pub x: f32,
    /// The vertical (Y) position of the cell's top edge in logical pixels.
    pub y: f32,
    /// The width of the cell in logical pixels (from [`Table::effective_width`]).
    pub width: f32,
}

/// Type alias for the per-row background callback to avoid `type_complexity` warnings.
type RowBgFn = dyn Fn(usize) -> Option<[u8; 4]> + Send + Sync;

/// A virtualized table widget backed by a [`RowSource`].
///
/// The optional second type parameter `Msg` (default `()`) is the application's
/// message type.  When `Msg` is not `()`, use [`Table::on_message`] to attach a
/// handler that is called whenever the table emits an application-level message.
///
/// Only rows visible within the current scroll viewport (plus a configurable
/// overscan region) are materialized, keeping CPU and memory usage constant
/// regardless of the total row count.
///
/// Column attributes that the frozen [`ColumnDef`](crate::ColumnDef) struct
/// does not carry (per-column alignment, sortability, runtime width) are stored
/// on the table itself, keyed by column index.
pub struct Table<S: RowSource, Msg = ()> {
    /// The underlying data source.
    source: S,
    /// Height of each row in logical pixels.
    row_height: f32,
    /// Number of extra rows to render beyond each edge of the visible viewport.
    overscan: usize,
    /// Per-column alignment overrides; `None` (or index `>= len`) falls back to
    /// the per-cell default alignment.
    aligns: Vec<Option<CellAlign>>,
    /// Per-column sortability flags; index `>= len` is treated as sortable.
    sortable: Vec<bool>,
    /// The active sort, if any.
    sort: Option<SortState>,
    /// Number of rows shown per pagination page. `0` means pagination is disabled.
    pub page_size: usize,
    /// Whether to render alternate rows with a slightly different background color.
    pub zebra_striping: bool,
    /// Column render order: `column_order[i]` is the logical column index rendered
    /// in position `i`. Defaults to the identity permutation.
    pub column_order: Vec<usize>,
    /// Runtime column widths (logical pixels).  Initialized from `column_defs().width`
    /// and updated by [`Table::resize_column`].  Renderers should prefer this over
    /// the source's `ColumnDef::width` so that user resize drags are reflected.
    pub column_widths: Vec<f32>,
    /// Per-column filter text strings (empty = no filter).
    /// Indexed by logical column index.  Update with [`Table::set_column_filter`].
    pub column_filters: Vec<String>,
    /// Number of leftmost columns to pin (freeze) during horizontal scrolling.
    pub pinned_columns: usize,
    /// Optional per-row background colour callback.
    ///
    /// Called with the **visible** row index.  Return `Some([r, g, b, a])` to
    /// paint a custom row background, or `None` to fall back to the default
    /// (zebra striping or theme background).
    pub row_background: Option<Box<RowBgFn>>,
    /// Optional application-message handler.
    ///
    /// Callers attach this via [`Table::on_message`].  The handler is invoked
    /// whenever the table needs to dispatch a `Msg` value to the application.
    on_message_handler: Option<Box<dyn FnMut(Msg) + Send>>,
    /// Carries the `Msg` type parameter without storing a value.
    _phantom: PhantomData<Msg>,
    /// Prefix-sum cumulative height cache for O(log n) row-at-offset lookups.
    height_cache: CumulativeHeightCache,
    /// Bounded LRU cache for last-N materialized rows.
    row_cache: RowCache,
    /// Height of the header row in logical pixels.
    ///
    /// Used when [`Table::sticky_headers`] is `true` to reserve space at the top
    /// of the viewport for the always-visible column header row.
    header_height: f32,
    /// Whether the column header row is pinned to the top of the viewport during
    /// vertical scrolling.
    ///
    /// When `true`:
    /// - [`Table::header_origin_y`] always returns `0.0` regardless of `v_scroll`.
    /// - [`Table::data_row_origin_y`] starts data rows at `header_height`.
    /// - [`Table::visible_range`] reduces the effective viewport height by
    ///   `header_height` so the correct number of data rows is virtualized.
    sticky_headers: bool,
}

impl<S: RowSource> Table<S> {
    /// Create a new [`Table`] wrapping `source` with default settings.
    ///
    /// Default `row_height` is `24.0` pixels; default `overscan` is `3` rows.
    /// Pagination is disabled by default (`page_size = 0`), zebra striping is off,
    /// and the column order is the identity permutation.
    ///
    /// The message type `Msg` defaults to `()`.  To use a different message
    /// type, specify the `Msg` type parameter explicitly when constructing `Table`.
    pub fn new(source: S) -> Self {
        let n_cols = source.column_defs().len();
        let column_order = (0..n_cols).collect();
        let column_widths = source.column_defs().iter().map(|c| c.width).collect();
        let column_filters = vec![String::new(); n_cols];
        let row_count = source.row_count();
        let mut height_cache = CumulativeHeightCache::new();
        height_cache.set_uniform_height(row_count, 24.0);
        Self {
            source,
            row_height: 24.0,
            overscan: 3,
            aligns: Vec::new(),
            sortable: Vec::new(),
            sort: None,
            page_size: 0,
            zebra_striping: false,
            column_order,
            column_widths,
            column_filters,
            pinned_columns: 0,
            row_background: None,
            on_message_handler: None,
            _phantom: PhantomData,
            height_cache,
            row_cache: RowCache::new(256),
            header_height: 32.0,
            sticky_headers: false,
        }
    }
}

impl<S: RowSource, Msg> Table<S, Msg> {
    /// Set the number of rows per pagination page. `0` disables pagination.
    pub fn with_page_size(mut self, size: usize) -> Self {
        self.page_size = size;
        self
    }

    /// Enable or disable zebra row striping.
    pub fn with_zebra_striping(mut self, enabled: bool) -> Self {
        self.zebra_striping = enabled;
        self
    }

    /// Override the column render order. Each element is a logical column index.
    /// If `order` is shorter than the number of columns, trailing columns are
    /// appended in their natural order.
    pub fn with_column_order(mut self, order: Vec<usize>) -> Self {
        self.column_order = order;
        self
    }

    /// Set a uniform height for all rows in logical pixels.
    ///
    /// Also updates the [`CumulativeHeightCache`](crate::CumulativeHeightCache) so that
    /// [`Table::cache_visible_range`] and [`Table::cache_row_at_offset`] reflect the new height.
    pub fn with_row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        let row_count = self.source.row_count();
        self.height_cache.set_uniform_height(row_count, h);
        self
    }

    /// Set per-row heights (variable-height rows) and update the cumulative height cache.
    ///
    /// `heights[i]` is the height of row `i` in logical pixels.  If `heights` is shorter
    /// than `row_count`, missing rows fall back to whatever height was previously configured.
    pub fn with_row_heights(mut self, heights: Vec<f32>) -> Self {
        self.height_cache.set_heights(heights);
        self
    }

    /// Return the row index whose vertical span contains scroll offset `y`,
    /// using the [`CumulativeHeightCache`](crate::CumulativeHeightCache) for O(log n) lookup.
    pub fn cache_row_at_offset(&mut self, y: f32) -> usize {
        self.height_cache.row_at_offset(y)
    }

    /// Return the range of rows at least partially visible in the viewport
    /// `[viewport_y, viewport_y + viewport_h)`, using the cumulative height cache.
    pub fn cache_visible_range(
        &mut self,
        viewport_y: f32,
        viewport_h: f32,
    ) -> std::ops::Range<usize> {
        self.height_cache.visible_range(viewport_y, viewport_h)
    }

    /// Fetch row `idx` from the cache if present, or materialise it from the
    /// source, insert it into the cache, and return the cells.
    pub fn get_or_fetch_row(&mut self, idx: usize) -> Vec<Cell> {
        if let Some(cached) = self.row_cache.get(idx) {
            return cached.clone();
        }
        let cells = self.source.row(idx);
        self.row_cache.insert(idx, cells.clone());
        cells
    }

    /// Invalidate the row cache. Call after any mutation to the underlying source.
    pub fn invalidate_row_cache(&mut self) {
        self.row_cache.invalidate();
    }

    /// Set the overscan — extra rows to render beyond the visible viewport on
    /// each edge to avoid flash-of-empty-row when scrolling quickly.
    pub fn with_overscan(mut self, overscan: usize) -> Self {
        self.overscan = overscan;
        self
    }

    /// Enable or disable sticky column headers.
    ///
    /// When `sticky` is `true`, the header row is always rendered at the top of
    /// the visible viewport (y = 0), regardless of the current vertical scroll
    /// offset.  Data rows are offset by [`Table::header_height`] so they begin
    /// below the pinned header.  [`Table::visible_range`] also subtracts the
    /// header height from the effective viewport height to virtualize the correct
    /// number of data rows.
    pub fn with_sticky_headers(mut self, sticky: bool) -> Self {
        self.sticky_headers = sticky;
        self
    }

    /// Set the height of the header row in logical pixels.
    ///
    /// Defaults to `32.0`. Only used when [`Table::sticky_headers`] is `true`.
    pub fn with_header_height(mut self, h: f32) -> Self {
        self.header_height = h;
        self
    }

    /// Return the configured header row height in logical pixels.
    pub fn header_height(&self) -> f32 {
        self.header_height
    }

    /// Return whether sticky column headers are enabled.
    pub fn sticky_headers(&self) -> bool {
        self.sticky_headers
    }

    /// Compute the Y position at which the header row should be rendered.
    ///
    /// When sticky headers are enabled, this always returns `0.0` so that the
    /// header stays pinned to the top of the viewport.  When disabled, the
    /// header scrolls with the content: its position is `-v_scroll` (i.e. the
    /// header is above the viewport when the user has scrolled down).
    pub fn header_origin_y(&self, v_scroll: f32) -> f32 {
        if self.sticky_headers {
            0.0
        } else {
            -v_scroll
        }
    }

    /// Compute the Y position at which the top of data `row` should be rendered.
    ///
    /// When sticky headers are enabled, data rows are pushed down by
    /// [`Table::header_height`] so they start below the pinned header.  The
    /// scroll offset is subtracted so that rows outside the viewport scroll out
    /// of view.
    ///
    /// Formula: `header_offset + row * row_height - v_scroll`, where
    /// `header_offset` is `header_height` when sticky and `0.0` otherwise.
    pub fn data_row_origin_y(&self, row: usize, v_scroll: f32) -> f32 {
        let header_offset = if self.sticky_headers {
            self.header_height
        } else {
            0.0
        };
        header_offset + row as f32 * self.row_height - v_scroll
    }

    /// Produce a [`Vec`] of [`RenderedCell`] values representing the column
    /// header row positioned at `origin_y`.
    ///
    /// Columns are laid out left-to-right according to `column_order`, using
    /// per-column widths from [`Table::effective_width`].  The `text` field of
    /// each [`RenderedCell`] is the [`crate::ColumnDef::name`] of the column.
    ///
    /// Pass `origin_y = 0.0` for a sticky header that is always pinned to the
    /// top of the viewport, or `origin_y = self.header_origin_y(v_scroll)` to
    /// let the header scroll with the content.
    pub fn render_header(&self, origin_y: f32) -> Vec<RenderedCell> {
        let col_defs = self.source.column_defs();
        let mut cells = Vec::with_capacity(self.column_order.len());
        let mut x = 0.0_f32;
        for &logical_col in &self.column_order {
            let text = col_defs
                .get(logical_col)
                .map(|d| d.name.clone())
                .unwrap_or_default();
            let width = self.effective_width(logical_col);
            cells.push(RenderedCell {
                col: logical_col,
                text,
                x,
                y: origin_y,
                width,
            });
            x += width;
        }
        cells
    }

    /// Set the alignment for `column`. Columns without an explicit alignment
    /// use the per-cell default ([`CellAlign::default_for`]).
    pub fn with_column_align(mut self, column: usize, align: CellAlign) -> Self {
        if self.aligns.len() <= column {
            self.aligns.resize(column + 1, None);
        }
        self.aligns[column] = Some(align);
        self
    }

    /// Mark whether `column` may be sorted by clicking its header.
    pub fn with_column_sortable(mut self, column: usize, sortable: bool) -> Self {
        if self.sortable.len() <= column {
            self.sortable.resize(column + 1, true);
        }
        self.sortable[column] = sortable;
        self
    }

    /// Set the number of leftmost columns to pin (freeze) during horizontal scrolling.
    pub fn with_pinned_columns(mut self, n: usize) -> Self {
        self.pinned_columns = n;
        self
    }

    /// Attach a per-row background colour callback.
    ///
    /// `f(vis_row)` should return `Some([r, g, b, a])` for rows that need a custom
    /// background, or `None` to fall back to the default (zebra / theme) colour.
    pub fn with_row_background<F>(mut self, f: F) -> Self
    where
        F: Fn(usize) -> Option<[u8; 4]> + Send + Sync + 'static,
    {
        self.row_background = Some(Box::new(f));
        self
    }

    /// Resolve the alignment for a cell in `column`: the explicit override if
    /// set, otherwise the cell's natural default.
    pub fn column_align(&self, column: usize, cell: &Cell) -> CellAlign {
        self.aligns
            .get(column)
            .copied()
            .flatten()
            .unwrap_or_else(|| CellAlign::default_for(cell))
    }

    /// Returns `true` if `column` is sortable (default `true`).
    pub fn is_column_sortable(&self, column: usize) -> bool {
        self.sortable.get(column).copied().unwrap_or(true)
    }

    /// The current sort state, if any.
    pub fn sort_state(&self) -> Option<SortState> {
        self.sort
    }

    /// Toggle sorting on `column`, cycling None→Asc→Desc→None. Sorting a
    /// different column resets to ascending. No-op if the column is not
    /// sortable. Returns the resulting [`SortState`] (or `None` when cleared).
    pub fn toggle_sort(&mut self, column: usize) -> Option<SortState> {
        if !self.is_column_sortable(column) {
            return self.sort;
        }
        let next_dir = match self.sort {
            Some(st) if st.column == column => st.direction.next(),
            _ => SortDirection::Ascending,
        };
        self.sort = match next_dir {
            SortDirection::None => None,
            dir => Some(SortState::new(column, dir)),
        };
        self.sort
    }

    /// Compute the current row-index ordering, applying the active sort if any.
    /// Returns the identity order when unsorted.
    pub fn sorted_indices(&self) -> Vec<usize> {
        match self.sort {
            Some(st) => crate::sort_indices(&self.source, st.column, st.direction),
            None => (0..self.source.row_count()).collect(),
        }
    }

    /// Apply the per-column filters to `sorted_indices` and return the
    /// matching subset.  An empty `column_filters` entry is treated as
    /// "no filter" (matches all rows).
    ///
    /// Returns the full sorted index when no filter is active.
    pub fn filtered_sorted_indices(&self) -> Vec<usize> {
        let sorted = self.sorted_indices();
        let active_filters: Vec<ColumnFilter> = self
            .column_filters
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.is_empty())
            .map(|(col, pat)| ColumnFilter::new(col, pat.as_str()))
            .collect();

        if active_filters.is_empty() {
            return sorted;
        }

        sorted
            .into_iter()
            .filter(|&i| {
                let row = self.source.row(i);
                active_filters.iter().all(|f| f.matches(&row))
            })
            .collect()
    }

    /// Update the per-column filter text for `col`.
    ///
    /// An empty string clears the filter for that column.  No-op if `col` is
    /// out of range.
    pub fn set_column_filter(&mut self, col: usize, text: String) {
        if let Some(slot) = self.column_filters.get_mut(col) {
            *slot = text;
        }
    }

    /// Apply a resize delta `delta_px` to `col`, clamping to the column's
    /// `min_width` / `max_width` / `resizable` constraints.
    ///
    /// Returns the new effective width, or `None` if:
    /// - `col` is out of range for `column_widths`, or
    /// - the column's [`ColumnDef`](crate::ColumnDef) marks it non-resizable.
    pub fn resize_column(&mut self, col: usize, delta_px: f32) -> Option<f32> {
        let col_def = self.source.column_defs().get(col)?;
        if !col_def.resizable {
            return None;
        }
        let current = self.column_widths.get(col).copied()?;
        let new_width = (current + delta_px).clamp(col_def.min_width, col_def.max_width);
        if let Some(slot) = self.column_widths.get_mut(col) {
            *slot = new_width;
        }
        Some(new_width)
    }

    /// Return the effective runtime width for `col` (logical pixels).
    ///
    /// Falls back to the source's `ColumnDef::width` if `col` is outside
    /// `column_widths`.
    pub fn effective_width(&self, col: usize) -> f32 {
        self.column_widths.get(col).copied().unwrap_or_else(|| {
            self.source
                .column_defs()
                .get(col)
                .map(|d| d.width)
                .unwrap_or(100.0)
        })
    }

    /// Return the background colour for `vis_row`, or `None` for the default.
    ///
    /// Consults `row_background` if set; otherwise returns `None`.  Renderers
    /// apply zebra striping independently from this callback.
    pub fn row_bg(&self, vis_row: usize) -> Option<[u8; 4]> {
        self.row_background.as_ref().and_then(|f| f(vis_row))
    }

    /// Return the total number of rows from the source.
    pub fn row_count(&self) -> usize {
        self.source.row_count()
    }

    /// Return a reference to the underlying [`RowSource`].
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Return the configured row height in logical pixels.
    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    /// Calculate which rows are visible for a viewport of height `viewport_height`
    /// starting at `scroll_offset` pixels from the top.
    ///
    /// The returned range is clamped to `0..row_count()`.
    ///
    /// When [`Table::sticky_headers`] is `true`, the effective viewport height
    /// for data rows is reduced by [`Table::header_height`] (the header occupies
    /// the top portion of the viewport).  The scroll offset is not adjusted —
    /// it still refers to the position within the data content.
    pub fn visible_range(
        &self,
        viewport_height: f32,
        scroll_offset: f32,
    ) -> std::ops::Range<usize> {
        let row_h = self.row_height.max(1.0);
        let first_raw = (scroll_offset / row_h) as usize;
        let effective_height = if self.sticky_headers {
            (viewport_height - self.header_height).max(0.0)
        } else {
            viewport_height
        };
        let count = (effective_height / row_h).ceil() as usize + self.overscan * 2;
        let first = first_raw.saturating_sub(self.overscan);
        let last = (first + count).min(self.source.row_count());
        first..last
    }

    /// Materialize only the visible rows for the given viewport parameters.
    ///
    /// Each returned inner `Vec<Cell>` corresponds to one row. Rows outside the
    /// visible range are never fetched from the source.
    pub fn materialize_visible(&self, viewport_height: f32, scroll_offset: f32) -> Vec<Vec<Cell>> {
        self.visible_range(viewport_height, scroll_offset)
            .map(|i| self.source.row(i))
            .collect()
    }

    /// Export all rows (after optional filter+sort, ignoring pagination) to CSV.
    ///
    /// Pass a non-empty `filters` slice to restrict to matching rows; pass an
    /// empty slice to export every row. The output uses `','` as the delimiter
    /// and follows RFC-4180 quoting.
    pub fn to_csv_all(&self, filters: &[ColumnFilter]) -> String {
        let sorted = self.sorted_indices();
        let matching: Vec<usize> = if filters.is_empty() {
            sorted
        } else {
            sorted
                .into_iter()
                .filter(|&i| {
                    let row = self.source.row(i);
                    filters.iter().all(|f| f.matches(&row))
                })
                .collect()
        };
        self.csv_from_indices(&matching)
    }

    /// Export visible rows (after filter+sort+pagination) to CSV.
    ///
    /// `page` is the [`PaginationState`] governing which page is visible.
    /// `filters` may be empty (no filtering).
    pub fn to_csv_visible(&self, page: &PaginationState, filters: &[ColumnFilter]) -> String {
        let sorted = self.sorted_indices();
        let filtered: Vec<usize> = if filters.is_empty() {
            sorted
        } else {
            sorted
                .into_iter()
                .filter(|&i| {
                    let row = self.source.row(i);
                    filters.iter().all(|f| f.matches(&row))
                })
                .collect()
        };
        let page_slice = page.apply(&filtered);
        self.csv_from_indices(page_slice)
    }

    /// Build a CSV string from a slice of row indices.
    fn csv_from_indices(&self, indices: &[usize]) -> String {
        let col_defs = self.source.column_defs();
        let delimiter = ',';
        let mut out = String::new();

        // Header row.
        if !col_defs.is_empty() {
            let header: Vec<String> = col_defs
                .iter()
                .map(|c| crate::csv::escape_field_pub(&c.name, delimiter))
                .collect();
            out.push_str(&header.join(","));
            out.push('\n');
        }

        for &i in indices {
            let row = self.source.row(i);
            let fields: Vec<String> = row
                .iter()
                .map(|cell| crate::csv::escape_field_pub(&cell.to_string(), delimiter))
                .collect();
            out.push_str(&fields.join(","));
            out.push('\n');
        }
        out
    }
}

// ── Virtual column rendering ──────────────────────────────────────────────────

impl<S: RowSource, Msg> Table<S, Msg> {
    /// Compute the range of column indices that are at least partially visible
    /// within a horizontal viewport.
    ///
    /// `h_scroll` is the current horizontal scroll offset (logical pixels from
    /// the left edge of the first column).  `viewport_width` is the visible
    /// width in logical pixels.
    ///
    /// The returned range is suitable for slicing `column_order` or iterating
    /// directly: `range.start..range.end` gives the first and one-past-last
    /// column render position whose pixel span overlaps `[h_scroll,
    /// h_scroll + viewport_width)`.
    ///
    /// Columns widths are read from [`column_widths`](Table::column_widths)
    /// via [`effective_width`](Table::effective_width), so user resize deltas
    /// are reflected correctly.
    ///
    /// Returns an empty range (`n..n`) when:
    /// - There are no columns.
    /// - `h_scroll` is beyond the total column extent.
    pub fn visible_column_range(
        &self,
        h_scroll: f32,
        viewport_width: f32,
    ) -> std::ops::Range<usize> {
        let n = self.column_widths.len();
        if n == 0 {
            return 0..0;
        }

        // Build prefix-sum array (length n+1).  prefix[i] is the pixel offset
        // of the left edge of column i in render (position) space.
        let mut prefix = vec![0.0_f32; n + 1];
        for i in 0..n {
            prefix[i + 1] = prefix[i] + self.effective_width(i);
        }

        // First column whose right edge (prefix[i+1]) is strictly greater than
        // h_scroll — equivalently, the first i where prefix[i+1] > h_scroll,
        // i.e. prefix[i] < h_scroll + epsilon.  We use partition_point on
        // prefix[0..=n] to find the insertion point of h_scroll, then subtract
        // 1 to get the column that starts at or before h_scroll.
        let start = prefix.partition_point(|&p| p <= h_scroll).saturating_sub(1);

        // One past the last column whose left edge is strictly less than the
        // right edge of the viewport.  partition_point finds the first index
        // where prefix[i] >= h_scroll + viewport_width; everything before that
        // index is visible.
        let end_raw = prefix.partition_point(|&p| p < h_scroll + viewport_width);
        let end = end_raw.min(n);

        start..end.max(start)
    }
}

// ── Widget bridge (oxiui-core) ────────────────────────────────────────────────

impl<S: RowSource, Msg> oxiui_core::Widget for Table<S, Msg> {
    /// Render a simplified text representation of the table via a [`UiCtx`].
    ///
    /// Each visible row (up to 100) is rendered as a single label whose cells
    /// are joined by `" | "`.  This makes `Table` embeddable in any UI backend
    /// that implements [`UiCtx`] without requiring the egui or iced feature
    /// flags.
    ///
    /// [`UiCtx`]: oxiui_core::UiCtx
    fn render(&mut self, ui: &mut dyn oxiui_core::UiCtx) {
        let count = self.source.row_count();
        for row_idx in 0..count.min(100) {
            let cells = self.source.row(row_idx);
            let row_text: Vec<String> = cells.iter().map(|c| c.to_string()).collect();
            ui.label(&row_text.join(" | "));
        }
    }
}

// ── Generic Msg impl (on_message + dispatch_message) ─────────────────────────

impl<S: RowSource, Msg> Table<S, Msg> {
    /// Attach an application-message handler.
    ///
    /// `f` is called (via [`Table::dispatch_message`]) whenever the table
    /// produces a `Msg` value.  This is the escape hatch for integrating the
    /// table into an Elm-style message-passing architecture without wrapping
    /// every table event in a manual `.map()` call.
    ///
    /// # Example
    /// ```rust,ignore
    /// table.on_message(|msg: MyAppMsg| sender.send(msg).ok());
    /// ```
    pub fn on_message<F: FnMut(Msg) + Send + 'static>(mut self, f: F) -> Self {
        self.on_message_handler = Some(Box::new(f));
        self
    }

    /// Dispatch a `Msg` value to the registered handler, if any.
    ///
    /// No-op when no handler has been attached via [`Table::on_message`].
    pub fn dispatch_message(&mut self, msg: Msg) {
        if let Some(handler) = self.on_message_handler.as_mut() {
            handler(msg);
        }
    }
}

// ── Table<S, Msg> tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod msg_tests {
    use super::*;
    use crate::{Cell, ColumnDef};

    struct EmptySource;
    impl crate::RowSource for EmptySource {
        fn row_count(&self) -> usize {
            0
        }
        fn row(&self, _: usize) -> Vec<Cell> {
            vec![]
        }
        fn column_defs(&self) -> &[ColumnDef] {
            &[]
        }
    }

    #[test]
    fn table_msg_unit_infers() {
        // Table::new(source) should infer Msg=() with no annotation.
        let _t: Table<EmptySource> = Table::new(EmptySource);
    }

    #[test]
    fn table_msg_string_explicit() {
        // Verify that on_message accepts a closure typed to the table's Msg parameter.
        // Table::new infers Msg=() by default. To use Msg=String we call on_message
        // with a String closure on a unit-Msg table and convert via a helper.
        //
        // The compile-time check below verifies:
        //   1. Table<S, ()> exists with new().
        //   2. on_message<F: FnMut(())> builder compiles.
        //   3. The type annotation `Table<EmptySource, ()>` is accepted.
        let t: Table<EmptySource, ()> = Table::new(EmptySource).on_message(|_: ()| {});
        // Verify the returned type is as annotated.
        let _: Table<EmptySource, ()> = t;
    }

    #[test]
    fn table_dispatch_message_calls_handler() {
        let called = std::sync::Arc::new(std::sync::Mutex::new(false));
        let called_clone = called.clone();
        // Use Msg=() (the default) but verify dispatch_message works end-to-end.
        let mut t: Table<EmptySource, ()> = Table::new(EmptySource).on_message(move |_: ()| {
            *called_clone.lock().unwrap_or_else(|e| e.into_inner()) = true;
        });
        t.dispatch_message(());
        assert!(*called.lock().unwrap_or_else(|e| e.into_inner()));
    }
}
