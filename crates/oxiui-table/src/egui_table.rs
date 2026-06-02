//! egui rendering backend for [`Table`].
//!
//! Uses `egui::ScrollArea::show_rows` for true virtualization — egui only
//! invokes the row-rendering closure for rows in the visible viewport.
//!
//! ## Extended render state
//!
//! The plain `render_egui` entry point accepts mutable state via
//! [`EguiTableState`] so that filter text, resize deltas, and events can flow
//! back to the caller without requiring `Table` to carry UI-only mutable state.
//!
//! ## Column resize
//!
//! A narrow drag-handle is rendered at the right edge of each header cell.
//! Dragging the handle emits [`TableEvent::ColumnResized`] and immediately
//! updates `table.column_widths[col]` through [`Table::resize_column`].
//!
//! ## Per-column filter inputs
//!
//! A second sub-row beneath the column-name row shows a `TextEdit` per column.
//! Changes emit [`TableEvent::FilterChanged`] and call
//! [`Table::set_column_filter`].
//!
//! ## Column pinning
//!
//! Pinned columns (`Table::pinned_columns`) are displayed with bold text as a
//! visual marker.  True fixed-left-panel pinning would require egui `SidePanel`
//! integration, which is deferred to a future revision.
//!
//! ## iced resize
//!
//! iced lacks the drag primitive required for column resize.  That feature is
//! documented as deferred to a future iced version or a custom widget.

use egui::{ScrollArea, Ui};

use crate::{header::HeaderSortState, CellAlign, RowSource, Table, TableEvent};

/// Caller-managed state for the egui table renderer.
///
/// Persist this between frames (e.g. in your `eframe::App` struct).
#[derive(Debug, Clone, Default)]
pub struct EguiTableState {
    /// Events collected during the last rendered frame.
    ///
    /// Clear or drain this after processing.
    pub events: Vec<TableEvent>,
    /// The `(row, col)` cell currently being edited, or `None` when no edit is
    /// in progress.
    pub edit_mode: Option<(usize, usize)>,
    /// The text buffer holding the in-progress edit value.
    pub edit_buffer: String,
    /// Set of row indices that are currently expanded in a tree/grouped table.
    pub expanded_rows: std::collections::HashSet<usize>,
    /// Shared horizontal scroll offset (logical pixels) used to keep the header
    /// row and the body scroll area in sync.
    ///
    /// Both the header and body [`egui::ScrollArea`]s are seeded with this value
    /// at the start of each frame.  After rendering the body, the actual offset
    /// reported by egui (which may be clamped) is written back here so subsequent
    /// frames reflect the true position.
    pub h_scroll_offset: f32,
}

impl EguiTableState {
    /// Begin editing the cell at `(row, col)`, initialising the edit buffer
    /// with `current` (the cell's current display string).
    pub fn begin_edit(&mut self, row: usize, col: usize, current: String) {
        self.edit_mode = Some((row, col));
        self.edit_buffer = current;
    }

    /// Commit the current edit.
    ///
    /// Returns `Some((row, col, value))` with the committed coordinates and
    /// value string, then clears the edit state.  Returns `None` if no edit
    /// was in progress.
    pub fn commit_edit(&mut self) -> Option<(usize, usize, String)> {
        let (row, col) = self.edit_mode.take()?;
        let value = std::mem::take(&mut self.edit_buffer);
        Some((row, col, value))
    }

    /// Cancel the current edit without committing, discarding the edit buffer.
    pub fn cancel_edit(&mut self) {
        self.edit_mode = None;
        self.edit_buffer.clear();
    }

    /// Toggle the expanded/collapsed state of `row` in a tree/grouped table.
    ///
    /// If `row` is currently expanded it is removed from [`expanded_rows`](Self::expanded_rows);
    /// if it is collapsed it is inserted.
    pub fn toggle_expand(&mut self, row: usize) {
        if self.expanded_rows.contains(&row) {
            self.expanded_rows.remove(&row);
        } else {
            self.expanded_rows.insert(row);
        }
    }
}

impl<S: RowSource> Table<S> {
    /// Render the table into an egui [`Ui`] using `ScrollArea::show_rows` for
    /// virtualized row rendering.
    ///
    /// # Parameters
    ///
    /// - `sort_state` — mutable: clicking a column header calls
    ///   [`HeaderSortState::toggle`] on it.  The caller is responsible for
    ///   persisting `sort_state` between frames.
    /// - `render_state` — mutable: collects [`TableEvent`]s emitted this frame
    ///   and holds any renderer-local UI state.
    ///
    /// Only rows in the visible scroll region are passed to egui; rows outside
    /// the viewport are replaced by blank space of the correct height so the
    /// scrollbar reflects the full row count.
    pub fn render_egui(
        &mut self,
        ui: &mut Ui,
        sort_state: &mut HeaderSortState,
        render_state: &mut EguiTableState,
    ) {
        render_state.events.clear();

        let row_height = self.row_height();
        let col_count = self.source().column_defs().len();

        // Resolve the render order for columns.
        let order: Vec<usize> = if self.column_order.len() == col_count {
            self.column_order.clone()
        } else {
            (0..col_count).collect()
        };

        // ── Snapshot data needed for header and filter rows ───────────────────
        // Collect header/resize/filter info outside closures to avoid borrow conflicts.
        let mut resize_events: Vec<TableEvent> = Vec::new();
        let mut sort_events: Vec<TableEvent> = Vec::new();

        // Snapshot the current h_scroll_offset to seed both scroll areas this frame.
        let h_offset = render_state.h_scroll_offset;

        // ── Header row: column-name buttons + resize handles ──────────────────
        // Wrap in a horizontal ScrollArea that is seeded with the shared offset.
        // The header is not draggable by the user (show_scrollbar=false), it
        // merely mirrors the body's horizontal position.
        ScrollArea::horizontal()
            .id_salt("oxiui_table_header_h")
            .scroll_offset(egui::Vec2::new(h_offset, 0.0))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for &col_idx in &order {
                        let effective_w = self.effective_width(col_idx);
                        let col_name = self
                            .source()
                            .column_defs()
                            .get(col_idx)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        let resizable_flag = self
                            .source()
                            .column_defs()
                            .get(col_idx)
                            .map(|c| c.resizable)
                            .unwrap_or(false);

                        let indicator = sort_state.indicator(col_idx);
                        let label_text = if indicator.is_empty() {
                            col_name
                        } else {
                            format!("{col_name} {indicator}")
                        };

                        // Sort-toggle button.
                        if ui
                            .add_sized(
                                [effective_w - 8.0, row_height],
                                egui::Button::new(egui::RichText::new(label_text).strong()),
                            )
                            .clicked()
                        {
                            sort_state.toggle(col_idx);
                            let ascending = sort_state
                                .column
                                .map(|c| c == col_idx && sort_state.ascending)
                                .unwrap_or(false);
                            sort_events.push(TableEvent::SortChanged {
                                col: col_idx,
                                ascending,
                            });
                        }

                        // Resize drag-handle (6 px strip at right edge of header cell).
                        if resizable_flag {
                            let drag_id = egui::Id::new(("col_resize", col_idx));
                            let drag_resp = ui.interact(
                                egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    egui::vec2(6.0, row_height),
                                ),
                                drag_id,
                                egui::Sense::drag(),
                            );
                            if drag_resp.hovered() || drag_resp.dragged() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            }
                            if drag_resp.dragged() {
                                let delta = drag_resp.drag_delta().x;
                                if let Some(new_w) = self.resize_column(col_idx, delta) {
                                    resize_events.push(TableEvent::ColumnResized {
                                        col: col_idx,
                                        new_width: new_w,
                                    });
                                }
                            }
                        }
                    }
                });
            });

        render_state.events.extend(sort_events);
        render_state.events.extend(resize_events);

        // ── Filter sub-row: one TextEdit per column ───────────────────────────
        let mut filter_events: Vec<TableEvent> = Vec::new();
        ScrollArea::horizontal()
            .id_salt("oxiui_table_filter_h")
            .scroll_offset(egui::Vec2::new(h_offset, 0.0))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for &col_idx in &order {
                        if col_idx < self.column_filters.len() {
                            let effective_w = self.effective_width(col_idx);
                            let filter_text = &mut self.column_filters[col_idx];
                            let resp = ui.add_sized(
                                [effective_w - 8.0, row_height - 4.0],
                                egui::TextEdit::singleline(filter_text).hint_text("Filter…"),
                            );
                            if resp.changed() {
                                let new_filter = filter_text.clone();
                                filter_events.push(TableEvent::FilterChanged {
                                    col: col_idx,
                                    new_filter,
                                });
                            }
                        }
                    }
                });
            });
        render_state.events.extend(filter_events);

        ui.separator();

        // Compute filtered+sorted indices.
        let filtered = self.filtered_sorted_indices();
        let total_visible = filtered.len();

        // ── Virtual column range: only render columns in the horizontal viewport ─
        // We obtain the current viewport width from the available rect.  If the
        // rect is zero-width (not yet laid out) we conservatively render all columns.
        let viewport_w = ui.available_width().max(0.0);
        let vis_col_range = if viewport_w > 0.0 {
            self.visible_column_range(h_offset, viewport_w)
        } else {
            0..order.len()
        };

        // Capture table config needed in the closure.
        let zebra = self.zebra_striping;
        let pinned = self.pinned_columns;
        // Restrict the order/widths snapshots to only the visible column range.
        let snap_order: Vec<usize> = order[vis_col_range.clone()].to_vec();
        let snap_widths: Vec<f32> = snap_order
            .iter()
            .map(|&ci| self.effective_width(ci))
            .collect();

        // Snapshot per-column alignment values for the visible columns only.
        let col_aligns: Vec<CellAlign> = snap_order
            .iter()
            .map(|&ci| self.column_align(ci, &crate::Cell::Empty))
            .collect();

        // Gather row-bg results before entering the closure (avoids borrow conflict).
        let row_bgs: Vec<Option<[u8; 4]>> =
            (0..total_visible).map(|vis| self.row_bg(vis)).collect();

        // Build cell data outside scroll closure to keep the closure non-borrowing.
        let rows_data: Vec<Vec<crate::Cell>> = filtered
            .iter()
            .map(|&src_i| self.source().row(src_i))
            .collect();

        // ── Scrollable body ───────────────────────────────────────────────────
        // Use both horizontal and vertical scroll.  The horizontal offset is
        // seeded from `h_scroll_offset` so the body starts aligned with the
        // header; after the frame we read back the actual offset (egui may
        // clamp it) and persist it for the next frame.
        let body_output = ScrollArea::vertical()
            .scroll_offset(egui::Vec2::new(h_offset, 0.0))
            .show_rows(ui, row_height, total_visible, |ui, row_range| {
                for vis_i in row_range {
                    // Custom row-background or zebra striping.
                    let bg = row_bgs
                        .get(vis_i)
                        .copied()
                        .flatten()
                        .map(|[r, g, b, a]| egui::Color32::from_rgba_premultiplied(r, g, b, a))
                        .or_else(|| {
                            if zebra && vis_i % 2 == 1 {
                                Some(ui.style().visuals.faint_bg_color)
                            } else {
                                None
                            }
                        });

                    let frame = if let Some(color) = bg {
                        egui::Frame::new().fill(color)
                    } else {
                        egui::Frame::new()
                    };

                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (render_pos, &col_idx) in snap_order.iter().enumerate() {
                                let cell_str = rows_data
                                    .get(vis_i)
                                    .and_then(|row| row.get(col_idx))
                                    .map(|c| c.to_string())
                                    .unwrap_or_default();

                                let align = col_aligns
                                    .get(render_pos)
                                    .copied()
                                    .unwrap_or(CellAlign::Left);
                                let w = snap_widths.get(render_pos).copied().unwrap_or(100.0);
                                let is_pinned = render_pos < pinned;

                                // Visual hint for pinned columns (bold text).
                                let rich = if is_pinned {
                                    egui::RichText::new(&cell_str).strong()
                                } else {
                                    egui::RichText::new(&cell_str)
                                };

                                // Alignment is expressed through layout, but for now we
                                // use add_sized which left-aligns by default.  The align
                                // value is preserved for future renderer refinement.
                                let _ = align;
                                ui.add_sized([w, row_height], egui::Label::new(rich));
                            }
                        });
                    });
                }
            });

        // Read back the actual horizontal offset reported by egui (egui may
        // clamp the value we set) and persist it so the next frame uses the
        // true position.
        render_state.h_scroll_offset = body_output.state.offset.x;
    }
}
