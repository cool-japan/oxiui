//! iced rendering backend for the table widget.
//!
//! ## Sticky header
//!
//! The header row is placed in a non-scrollable `row` container that sits above
//! the `scrollable` body.  This gives a frozen/sticky header: the header stays
//! in place while only the body rows scroll.
//!
//! ## Virtualization note (M3 limitation)
//!
//! iced's `scrollable` widget does not expose its current scroll offset to
//! application code without setting up a `scrollable::Id` + subscribing to
//! `scrollable::scroll_to` events. For M3 we accept a `scroll_offset: usize`
//! parameter (row index) that the caller must track and pass in. This keeps the
//! API simple while still supporting the windowed-rows model.
//!
//! A future revision can wire up `scrollable::Id`-based offset tracking to
//! derive `scroll_offset` automatically from the widget event stream.
//!
//! ## Column resize (deferred)
//!
//! iced 0.14 does not expose a drag primitive suitable for column resize handles.
//! This feature is deferred until a future iced version ships native drag support
//! or a custom widget is introduced.  See `TODO.md` for details.
//!
//! ## Filter inputs
//!
//! Use [`render_iced_with_filters`] for a variant that adds a `text_input` row
//! per column beneath the sort header.  The caller is responsible for tracking
//! filter state and mapping the `on_filter_change(col, text)` message.

use iced::widget::{column, row, scrollable, text, text_input};
use iced::Element;

use crate::{header::HeaderSortState, RowSource};

/// Render a windowed subset of rows from `source` as an iced widget with a
/// sticky (non-scrolling) header row.
///
/// # Parameters
///
/// - `source`: the `RowSource` to render.
/// - `viewport_rows`: how many rows fit in the visible viewport area.
/// - `scroll_offset`: first visible row index (caller-tracked; 0 = top).
/// - `sort_state`: the current sort state, used to display ▲/▼ indicators in
///   the header cells.
///
/// # Virtualization
///
/// Only rows `scroll_offset .. (scroll_offset + viewport_rows + overscan)` are
/// materialised. `overscan` is hardcoded to 3 rows on each side for M3.
pub fn render_iced<'a, Msg>(
    source: &'a dyn RowSource,
    viewport_rows: usize,
    scroll_offset: usize,
    sort_state: &HeaderSortState,
) -> Element<'a, Msg>
where
    Msg: Clone + 'a,
{
    const OVERSCAN: usize = 3;
    let col_defs = source.column_defs();
    let total = source.row_count();
    let start = scroll_offset.saturating_sub(OVERSCAN).min(total);
    let end = (scroll_offset + viewport_rows + OVERSCAN * 2).min(total);

    // ── Sticky header row (NOT inside scrollable) ──────────────────────────
    // Each header cell shows the column name plus the sort indicator if active.
    let header_cells: Vec<Element<'a, Msg>> = col_defs
        .iter()
        .enumerate()
        .map(|(col_idx, c)| {
            let indicator = sort_state.indicator(col_idx);
            let label = if indicator.is_empty() {
                c.name.clone()
            } else {
                format!("{} {indicator}", c.name)
            };
            text(label).size(14).into()
        })
        .collect();
    let header = row(header_cells).spacing(8);

    // ── Scrollable body rows — only the windowed slice is materialised ─────
    let data_rows: Vec<Element<'a, Msg>> = (start..end)
        .map(|i| {
            let cells = source.row(i);
            let cell_els: Vec<Element<'a, Msg>> = cells
                .iter()
                .map(|cell| text(cell.to_string()).size(13).into())
                .collect();
            row(cell_els).spacing(8).into()
        })
        .collect();

    let body = scrollable(column(data_rows).spacing(2).padding(4));

    // Stack the sticky header above the scrollable body.
    column(vec![header.into(), body.into()])
        .spacing(2)
        .padding(4)
        .into()
}

/// Render a windowed subset of rows with an additional per-column filter
/// `text_input` row in the header.
///
/// # Parameters
///
/// - `source`: the `RowSource` to render.
/// - `viewport_rows`: how many rows fit in the visible viewport area.
/// - `scroll_offset`: first visible row index (caller-tracked; 0 = top).
/// - `sort_state`: the current sort state used to display ▲/▼ indicators.
/// - `filter_values`: current filter text per column (index matches column).
/// - `on_filter_change`: message constructor called with `(col_index, new_text)`
///   when the user edits a filter input.
///
/// # Column resize (deferred)
///
/// iced 0.14 lacks a drag primitive.  Resize is deferred — see `TODO.md`.
pub fn render_iced_with_filters<'a, Msg, F>(
    source: &'a dyn RowSource,
    viewport_rows: usize,
    scroll_offset: usize,
    sort_state: &HeaderSortState,
    filter_values: &'a [String],
    on_filter_change: F,
) -> Element<'a, Msg>
where
    Msg: Clone + 'a,
    F: Fn(usize, String) -> Msg + Clone + 'a,
{
    const OVERSCAN: usize = 3;
    let col_defs = source.column_defs();
    let total = source.row_count();
    let start = scroll_offset.saturating_sub(OVERSCAN).min(total);
    let end = (scroll_offset + viewport_rows + OVERSCAN * 2).min(total);

    // ── Sort header row ────────────────────────────────────────────────────
    let header_cells: Vec<Element<'a, Msg>> = col_defs
        .iter()
        .enumerate()
        .map(|(col_idx, c)| {
            let indicator = sort_state.indicator(col_idx);
            let label = if indicator.is_empty() {
                c.name.clone()
            } else {
                format!("{} {indicator}", c.name)
            };
            text(label).size(14).into()
        })
        .collect();
    let header = row(header_cells).spacing(8);

    // ── Filter input row ───────────────────────────────────────────────────
    let filter_row_cells: Vec<Element<'a, Msg>> = col_defs
        .iter()
        .enumerate()
        .map(|(col_idx, _c)| {
            let current = filter_values.get(col_idx).map(|s| s.as_str()).unwrap_or("");
            let cb = on_filter_change.clone();
            text_input("Filter…", current)
                .on_input(move |new_text| cb(col_idx, new_text))
                .size(12)
                .into()
        })
        .collect();
    let filter_row = row(filter_row_cells).spacing(8);

    // ── Scrollable body rows ───────────────────────────────────────────────
    let data_rows: Vec<Element<'a, Msg>> = (start..end)
        .map(|i| {
            let cells = source.row(i);
            let cell_els: Vec<Element<'a, Msg>> = cells
                .iter()
                .map(|cell| text(cell.to_string()).size(13).into())
                .collect();
            row(cell_els).spacing(8).into()
        })
        .collect();

    let body = scrollable(column(data_rows).spacing(2).padding(4));

    // Stack: sort header → filter row → scrollable body.
    column(vec![header.into(), filter_row.into(), body.into()])
        .spacing(2)
        .padding(4)
        .into()
}
