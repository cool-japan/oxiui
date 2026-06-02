//! Demonstrates the `oxiui-table` virtualized table widget via the OxiUI egui facade.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_table --features table -p oxiui
//! ```
//!
//! The table widget uses viewport-based row virtualization: only a small window
//! of rows around the current scroll position is materialized per frame. This
//! demo creates 1 000 rows but allocates memory for at most ~16 at a time.

use oxiui::table::{Cell, ColumnDef, RowSource, Table};
use oxiui::{theme, UiError};

/// Demo data source — generates 1 000 rows on demand.
struct DemoData {
    columns: Vec<ColumnDef>,
}

impl DemoData {
    fn new() -> Self {
        Self {
            columns: vec![
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
                ColumnDef {
                    name: "Value".into(),
                    width: 80.0,
                    ..ColumnDef::default()
                },
            ],
        }
    }
}

impl RowSource for DemoData {
    fn row_count(&self) -> usize {
        1_000
    }

    fn row(&self, i: usize) -> Vec<Cell> {
        vec![
            Cell::Int(i as i64),
            Cell::Text(format!("Item {i}")),
            Cell::Float(i as f64 * 1.5),
        ]
    }

    fn column_defs(&self) -> &[ColumnDef] {
        &self.columns
    }
}

fn main() -> Result<(), UiError> {
    let table = Table::new(DemoData::new());
    let row_count = table.row_count();

    oxiui::App::new(oxiui::AppConfig::new().title("OxiUI Table Demo"))
        .theme(theme::cooljapan_default())
        .content(move |ui| {
            ui.heading("OxiUI Virtualized Table");
            ui.label(&format!(
                "Table has {row_count} rows — only a small viewport window is materialized per frame."
            ));
            // Note: deep egui `ScrollArea::show_rows` integration requires the
            // `egui-table` feature on `oxiui-table` and direct access to the
            // egui `Ui`. For M3 this demonstrates the virtual row-count API
            // through the `UiCtx` facade.
            let sample = table.materialize_visible(240.0, 0.0);
            ui.label(&format!(
                "First {} rows materialized for a 240 px viewport at scroll=0:",
                sample.len()
            ));
            for row in &sample {
                let line: Vec<String> = row.iter().map(|c| c.to_string()).collect();
                ui.label(&line.join("  |  "));
            }
        })
        .run()?;
    Ok(())
}
