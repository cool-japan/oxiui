//! Hello Dioxus — OxiUI facade using the Dioxus backend.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_dioxus --features dioxus -p oxiui
//! ```
//!
//! In M5, this example runs in headless collection mode (no window is opened).
//! The content closure executes against [`oxiui_dioxus::DioxusCtx`], and all
//! widget descriptions are collected in memory. This satisfies the "example
//! builds" acceptance criterion for M5 without requiring a display or any
//! C/C++ dependencies (the `desktop` feature of dioxus, which pulls in
//! wry/tao/WebKit, is intentionally excluded).
//!
//! Full Dioxus native rendering via `dioxus-native` (Pure Rust Blitz renderer)
//! is planned for M6.

fn main() -> Result<(), oxiui::UiError> {
    oxiui::App::new(oxiui::AppConfig::new().title("Hello Dioxus"))
        .backend(oxiui::Backend::Dioxus)
        .theme(oxiui::theme::cooljapan_default())
        .content(|ui| {
            ui.heading("Hello from Dioxus");
            ui.label("OxiUI + Dioxus backend (M5 headless mode)");
            let resp = ui.button("Quit");
            if resp.clicked {
                std::process::exit(0);
            }
        })
        .run()?;
    Ok(())
}
