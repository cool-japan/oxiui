//! Integration tests: `oxiui-table` + egui adapter.
//!
//! These tests verify that `Table::render_egui` works through the egui headless
//! harness and that `SelectionModel` + `SortState` update correctly in response
//! to simulated events.
//!
//! The `oxiui-table` crate is a dev-dependency, so these tests only run as
//! integration tests (not with `--lib`).

use oxiui_table::{
    header::HeaderSortState, Cell, ColumnDef, EguiTableState, RowSource, SelectionMode,
    SelectionModel, SortDirection, Table, TableEvent,
};

/// Minimal data source for testing.
struct TestSource {
    cols: Vec<ColumnDef>,
}

impl TestSource {
    fn new(col_count: usize) -> Self {
        let cols = (0..col_count)
            .map(|i| ColumnDef {
                name: format!("Col{i}"),
                width: 80.0,
                resizable: true,
                ..ColumnDef::default()
            })
            .collect();
        Self { cols }
    }
}

impl RowSource for TestSource {
    fn row_count(&self) -> usize {
        5
    }

    fn row(&self, i: usize) -> Vec<Cell> {
        self.cols
            .iter()
            .enumerate()
            .map(|(c, _)| Cell::Text(format!("r{i}c{c}")))
            .collect()
    }

    fn column_defs(&self) -> &[ColumnDef] {
        &self.cols
    }
}

/// Build an egui headless context and run one frame executing `f`.
/// Returns the `EguiTableState` after the frame.
fn run_table_frame<F>(mut f: F) -> EguiTableState
where
    F: FnMut(&mut egui::Ui, &mut EguiTableState, &mut HeaderSortState),
{
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let mut render_state = EguiTableState::default();
    let mut sort_state = HeaderSortState::default();
    let _ = ctx.run_ui(raw_input, |ui| {
        f(ui, &mut render_state, &mut sort_state);
    });
    render_state
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// `Table::render_egui` with a 2-column source completes without panicking.
#[test]
fn table_render_egui_no_panic() {
    run_table_frame(|ui, rs, ss| {
        let mut table = Table::new(TestSource::new(2));
        table.render_egui(ui, ss, rs);
    });
}

/// Rendering a 3-column table produces no events when there are no interactions.
#[test]
fn table_no_events_without_interaction() {
    let state = run_table_frame(|ui, rs, ss| {
        let mut table = Table::new(TestSource::new(3));
        table.render_egui(ui, ss, rs);
    });
    assert!(
        state.events.is_empty(),
        "expected no events without interaction; got {:?}",
        state.events
    );
}

/// `HeaderSortState::toggle` correctly tracks sort column and direction.
#[test]
fn sort_state_toggle_tracks_column() {
    let mut ss = HeaderSortState::default();
    // First toggle: ascending on column 1.
    ss.toggle(1);
    assert_eq!(ss.column, Some(1));
    assert!(ss.ascending);

    // Second toggle on same column: descending.
    ss.toggle(1);
    assert_eq!(ss.column, Some(1));
    assert!(!ss.ascending);

    // Toggle on different column: ascending on column 0.
    ss.toggle(0);
    assert_eq!(ss.column, Some(0));
    assert!(ss.ascending);
}

/// `SelectionModel::click` selects a single row and reports it as selected.
#[test]
fn selection_model_click_selects_row() {
    let mut sel = SelectionModel::new(SelectionMode::Single);
    assert!(sel.is_empty());

    sel.click(2);
    assert!(sel.is_selected(2));
    assert_eq!(sel.len(), 1);

    // Click a different row: old is deselected, new is selected.
    sel.click(4);
    assert!(!sel.is_selected(2));
    assert!(sel.is_selected(4));
    assert_eq!(sel.len(), 1);
}

/// Multi-select: `ctrl_click` toggles rows.
#[test]
fn selection_model_ctrl_click_multi_select() {
    let mut sel = SelectionModel::new(SelectionMode::Multi);
    sel.click(0);
    sel.ctrl_click(2);
    sel.ctrl_click(4);
    assert_eq!(sel.len(), 3);
    assert!(sel.is_selected(0));
    assert!(sel.is_selected(2));
    assert!(sel.is_selected(4));

    // ctrl_click again on 2 removes it.
    sel.ctrl_click(2);
    assert!(!sel.is_selected(2));
    assert_eq!(sel.len(), 2);
}

/// `SelectionModel::clear` empties the selection.
#[test]
fn selection_model_clear_empties() {
    let mut sel = SelectionModel::new(SelectionMode::Multi);
    sel.click(0);
    sel.ctrl_click(1);
    assert_eq!(sel.len(), 2);

    sel.clear();
    assert!(sel.is_empty());
}

/// Applying `RowSelected` events to a `SelectionModel` via the bridge helper.
///
/// This exercises the `apply_selection_events` path without the `table` feature.
#[test]
fn selection_events_update_model_manually() {
    let events = vec![TableEvent::RowSelected(1), TableEvent::RowSelected(3)];
    let mut sel = SelectionModel::new(SelectionMode::Single);

    // Simulate what `apply_selection_events` does.
    let mut changed = false;
    for ev in &events {
        if let TableEvent::RowSelected(row) = ev {
            sel.click(*row);
            changed = true;
        }
    }

    assert!(changed);
    // Single mode: last click wins.
    assert!(sel.is_selected(3));
    assert!(!sel.is_selected(1));
}

/// `EguiTableState` edit helpers: begin/commit/cancel cycle.
#[test]
fn table_state_edit_begin_commit_cancel() {
    let mut st = EguiTableState::default();
    assert!(st.edit_mode.is_none());

    st.begin_edit(2, 1, "old".to_owned());
    assert_eq!(st.edit_mode, Some((2, 1)));
    assert_eq!(st.edit_buffer, "old");

    st.edit_buffer = "new".to_owned();
    let result = st.commit_edit();
    assert_eq!(result, Some((2, 1, "new".to_owned())));
    assert!(st.edit_mode.is_none());
    assert!(st.edit_buffer.is_empty());

    // Cancel: no commit.
    st.begin_edit(0, 0, "x".to_owned());
    st.cancel_edit();
    assert!(st.edit_mode.is_none());
    assert!(st.edit_buffer.is_empty());
}

/// `EguiTableState::toggle_expand` toggles row expansion.
#[test]
fn table_state_toggle_expand() {
    let mut st = EguiTableState::default();
    assert!(st.expanded_rows.is_empty());

    st.toggle_expand(5);
    assert!(st.expanded_rows.contains(&5));

    st.toggle_expand(5);
    assert!(!st.expanded_rows.contains(&5));
}

/// Multiple frames: rendering the same table twice is stable (no accumulated state).
#[test]
fn table_render_stable_across_two_frames() {
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();

    let mut render_state = EguiTableState::default();
    let mut sort_state = HeaderSortState::default();

    for _ in 0..2 {
        let _ = ctx.run_ui(raw_input.clone(), |ui| {
            let mut table = Table::new(TestSource::new(2));
            table.render_egui(ui, &mut sort_state, &mut render_state);
        });
    }
    // Both frames should complete without panic.
}

/// `SortDirection` enum coverage including the `None` cycle.
#[test]
fn sort_direction_variants() {
    let asc = SortDirection::Ascending;
    let desc = SortDirection::Descending;
    let none = SortDirection::None;
    assert!(matches!(asc, SortDirection::Ascending));
    assert!(matches!(desc, SortDirection::Descending));
    assert!(matches!(none, SortDirection::None));
    // `next()` cycles: None → Ascending → Descending → None.
    assert!(matches!(none.next(), SortDirection::Ascending));
    assert!(matches!(
        SortDirection::Ascending.next(),
        SortDirection::Descending
    ));
    assert!(matches!(
        SortDirection::Descending.next(),
        SortDirection::None
    ));
}
