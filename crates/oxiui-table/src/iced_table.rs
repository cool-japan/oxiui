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
//! ## Column sorting
//!
//! Use [`render_iced_sortable`] for a variant where each column header is a
//! pressable `button` that emits an `on_sort_toggle(col_index)` message.
//! The caller tracks [`HeaderSortState`] and passes the updated state back on
//! the next frame.  Combine with [`sort_indices`](crate::sort_indices) to
//! reorder rows before rendering.
//!
//! ## Row selection
//!
//! Use [`render_iced_with_selection`] to render selected rows with a
//! highlighted background.  The caller owns a [`SelectionModel`] and maps
//! row-click messages through [`handle_row_click`] to keep it up-to-date.
//!
//! ## Scroll-offset tracking via `scrollable::Id`
//!
//! Pass a `scrollable_id: Option<widget::Id>` (from `iced::widget::Id::new("body")`)
//! to any of the `_with_scroll_id` variants.  The id is attached to the
//! scrollable body widget so that the host application can subscribe to
//! `scrollable::scroll_to` events and derive the current `scroll_offset`.
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

use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Color, Element};

use crate::{header::HeaderSortState, selection::SelectionModel, RowSource};

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

// ── Sortable header variant ────────────────────────────────────────────────────

/// Render a table with clickable sort headers.
///
/// Each column header is a pressable `button` that emits
/// `on_sort_toggle(col_index)` when clicked.  The caller is responsible for
/// tracking [`HeaderSortState`] and calling [`HeaderSortState::toggle`] in
/// their `update` function to cycle `None → Ascending → Descending → None`.
///
/// # Sort + render workflow
///
/// ```text
/// 1. sort_state.toggle(col);              // in update()
/// 2. let perm = sort_indices(source, ...); // from crate::sort
/// 3. render_iced_sortable(&sorted_source, ...);
/// ```
///
/// # Parameters
///
/// - `source`: the `RowSource` to render.
/// - `viewport_rows`: how many rows fit in the visible viewport area.
/// - `scroll_offset`: first visible row index (caller-tracked; 0 = top).
/// - `sort_state`: the current header sort state (controls ▲/▼ indicators).
/// - `on_sort_toggle`: message constructor called with the clicked column index.
/// - `scrollable_id`: optional `iced::widget::Id` attached to the scrollable
///   body so the host can subscribe to scroll events.
pub fn render_iced_sortable<'a, Msg, F>(
    source: &'a dyn RowSource,
    viewport_rows: usize,
    scroll_offset: usize,
    sort_state: &HeaderSortState,
    on_sort_toggle: F,
    scrollable_id: Option<iced::widget::Id>,
) -> Element<'a, Msg>
where
    Msg: Clone + 'a,
    F: Fn(usize) -> Msg + Clone + 'a,
{
    const OVERSCAN: usize = 3;
    let col_defs = source.column_defs();
    let total = source.row_count();
    let start = scroll_offset.saturating_sub(OVERSCAN).min(total);
    let end = (scroll_offset + viewport_rows + OVERSCAN * 2).min(total);

    // ── Clickable sort header ──────────────────────────────────────────────
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
            let cb = on_sort_toggle.clone();
            button(text(label).size(14)).on_press(cb(col_idx)).into()
        })
        .collect();
    let header = row(header_cells).spacing(8);

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

    let body_col = column(data_rows).spacing(2).padding(4);
    let body = if let Some(id) = scrollable_id {
        scrollable(body_col).id(id)
    } else {
        scrollable(body_col)
    };

    column(vec![header.into(), body.into()])
        .spacing(2)
        .padding(4)
        .into()
}

// ── Selection-aware variant ───────────────────────────────────────────────────

/// Background colour applied to selected rows.
const SELECTION_BG: Color = Color {
    r: 0.22,
    g: 0.56,
    b: 0.92,
    a: 0.28,
};

/// Render a table with row-selection highlighting.
///
/// Selected rows (as tracked by `selection`) are rendered inside a coloured
/// `container` so they visually stand out from unselected rows.  The caller
/// must emit row-click messages and call
/// [`handle_row_click`](crate::handle_row_click) / the [`SelectionModel`]
/// methods to keep `selection` up-to-date between frames.
///
/// # Parameters
///
/// - `source`: the `RowSource` to render.
/// - `viewport_rows`: how many rows fit in the visible viewport area.
/// - `scroll_offset`: first visible row index (caller-tracked; 0 = top).
/// - `sort_state`: the current sort state (controls ▲/▼ indicators in headers).
/// - `selection`: the set of selected row indices.
/// - `on_row_click`: message constructor called with the clicked row index.
/// - `scrollable_id`: optional `iced::widget::Id` for scroll-event tracking.
#[allow(clippy::too_many_arguments)]
pub fn render_iced_with_selection<'a, Msg, F>(
    source: &'a dyn RowSource,
    viewport_rows: usize,
    scroll_offset: usize,
    sort_state: &HeaderSortState,
    selection: &SelectionModel,
    on_row_click: F,
    scrollable_id: Option<iced::widget::Id>,
) -> Element<'a, Msg>
where
    Msg: Clone + 'a,
    F: Fn(usize) -> Msg + Clone + 'a,
{
    const OVERSCAN: usize = 3;
    let col_defs = source.column_defs();
    let total = source.row_count();
    let start = scroll_offset.saturating_sub(OVERSCAN).min(total);
    let end = (scroll_offset + viewport_rows + OVERSCAN * 2).min(total);

    // ── Sticky header (static text, no click handlers) ─────────────────────
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

    // ── Scrollable body with selection highlight ───────────────────────────
    let data_rows: Vec<Element<'a, Msg>> =
        (start..end)
            .map(|row_idx| {
                let cells = source.row(row_idx);
                let cell_els: Vec<Element<'a, Msg>> = cells
                    .iter()
                    .map(|cell| text(cell.to_string()).size(13).into())
                    .collect();
                let row_widget = row(cell_els).spacing(8);
                let cb = on_row_click.clone();
                if selection.is_selected(row_idx) {
                    // Wrap in a highlighted container.
                    container(button(row_widget).on_press(cb(row_idx)).style(
                        move |_theme, _status| iced::widget::button::Style {
                            background: Some(iced::Background::Color(SELECTION_BG)),
                            ..iced::widget::button::Style::default()
                        },
                    ))
                    .into()
                } else {
                    button(row_widget).on_press(cb(row_idx)).into()
                }
            })
            .collect();

    let body_col = column(data_rows).spacing(2).padding(4);
    let body = if let Some(id) = scrollable_id {
        scrollable(body_col).id(id)
    } else {
        scrollable(body_col)
    };

    column(vec![header.into(), body.into()])
        .spacing(2)
        .padding(4)
        .into()
}
