//! Hello Slint — exercises the `oxiui-slint` adapter directly (headless).
//!
//! `oxiui-slint` is a KNOWN-NON-PURE third-party adapter: enabling its `slint`
//! feature pulls `slint` -> parley/fontique -> `yeslogic-fontconfig-sys`
//! (a C fontconfig binding) on Linux. It is therefore NOT part of the OxiUI
//! Pure-Rust L1 set; depend on it directly only if you accept that boundary.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_slint --features slint -p oxiui-slint
//! ```
//!
//! This runs in headless collection mode (no window is opened): the content
//! closure executes through `oxiui_slint::SlintCtx` and all widget descriptions
//! are collected in memory, satisfying the "example builds" acceptance
//! criterion without requiring a display.

use oxiui_core::UiCtx;
use oxiui_slint::run_slint;
use oxiui_theme::cooljapan_default;

fn main() -> Result<(), oxiui_core::UiError> {
    let theme = cooljapan_default();
    run_slint(&*theme, |ui: &mut dyn UiCtx| {
        ui.heading("Hello from Slint");
        ui.label("OxiUI + slint adapter (headless collection mode)");
        let resp = ui.button("Quit");
        if resp.clicked {
            std::process::exit(0);
        }
    })?;
    Ok(())
}
