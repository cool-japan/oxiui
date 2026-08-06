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
//! Native slint window rendering (`slint::run_event_loop`) is not yet wired, so
//! [`oxiui_slint::run_slint`] currently returns
//! [`oxiui_core::UiError::Unsupported`]. This example demonstrates the
//! fully-supported headless path — driving an [`oxiui_slint::SlintCtx`] directly
//! and inspecting the collected widget descriptions — and then shows that the
//! native window path reports its unsupported status honestly.

use oxiui_core::{UiCtx, UiError};
use oxiui_slint::{run_slint, SlintCtx};
use oxiui_theme::cooljapan_default;

fn main() {
    // Supported today: headless widget collection through `SlintCtx`.
    let mut ctx = SlintCtx::default();
    ctx.heading("Hello from Slint");
    ctx.label("OxiUI + slint adapter (headless collection mode)");
    let _ = ctx.button("Quit");
    println!("collected {} slint widget description(s):", ctx.items.len());
    for item in &ctx.items {
        println!("  - {item}");
    }

    // The native window path is deliberately honest about not being implemented.
    let theme = cooljapan_default();
    match run_slint(&*theme, |ui: &mut dyn UiCtx| {
        ui.heading("Hello from Slint");
    }) {
        Ok(()) => println!("slint window closed"),
        Err(UiError::Unsupported(msg)) => {
            println!("native slint window not available yet: {msg}");
        }
        Err(other) => eprintln!("unexpected slint error: {other}"),
    }
}
