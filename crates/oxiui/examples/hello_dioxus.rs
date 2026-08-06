//! Hello Dioxus — OxiUI facade using the Dioxus backend.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_dioxus --features dioxus -p oxiui
//! ```
//!
//! The Dioxus backend's native window path is not yet wired: the Pure-Rust
//! desktop renderer `dioxus-native` (Blitz/Vello) is not yet stable, and the
//! `desktop` feature pulls in wry/tao/WebKit (C/C++), which violates the Pure
//! Rust policy. Until a Pure-Rust launch path lands, `App::run()` with
//! [`oxiui::Backend::Dioxus`] returns [`oxiui::UiError::Unsupported`] rather than
//! reporting a fake successful exit. This example handles that outcome honestly.

fn main() {
    let result = oxiui::App::new(oxiui::AppConfig::new().title("Hello Dioxus"))
        .backend(oxiui::Backend::Dioxus)
        .theme(oxiui::theme::cooljapan_default())
        .content(|ui| {
            ui.heading("Hello from Dioxus");
            ui.label("OxiUI + Dioxus backend");
            let resp = ui.button("Quit");
            if resp.clicked {
                std::process::exit(0);
            }
        })
        .run();

    match result {
        Ok(exit) => println!("Dioxus backend exited: {exit:?}"),
        Err(oxiui::UiError::Unsupported(msg)) => {
            println!("Dioxus backend not available yet: {msg}");
        }
        Err(other) => {
            eprintln!("Dioxus backend error: {other}");
            std::process::exit(1);
        }
    }
}
