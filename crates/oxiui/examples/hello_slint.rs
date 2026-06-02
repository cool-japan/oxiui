//! Hello Slint — OxiUI facade using the slint backend.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_slint --features slint -p oxiui
//! ```
//!
//! In M5, this example runs in headless collection mode (no window is opened).
//! The content closure executes against [`oxiui_slint::SlintCtx`], and all
//! widget descriptions are collected in memory. This satisfies the "example
//! builds" acceptance criterion for M5 without requiring a display.
//!
//! A native slint window (via `slint::run_event_loop()`) is planned for M6.

fn main() -> Result<(), oxiui::UiError> {
    oxiui::App::new(oxiui::AppConfig::new().title("Hello Slint"))
        .backend(oxiui::Backend::Slint)
        .theme(oxiui::theme::cooljapan_default())
        .content(|ui| {
            ui.heading("Hello from Slint");
            ui.label("OxiUI + slint backend (M5 headless mode)");
            let resp = ui.button("Quit");
            if resp.clicked {
                std::process::exit(0);
            }
        })
        .run()?;
    Ok(())
}
