//! Build-time tests for `oxiui-slint`.
//!
//! These tests verify that [`SlintCtx`] can be constructed and exercised
//! without opening a real window (headless / CI compatible).

use oxiui_core::UiCtx;
use oxiui_slint::SlintCtx;

#[test]
fn slint_ctx_constructs() {
    let ctx = SlintCtx::default();
    assert!(ctx.items.is_empty(), "fresh SlintCtx should have no items");
}

#[test]
fn slint_ctx_heading() {
    let mut ctx = SlintCtx::default();
    ctx.heading("Hello");
    assert_eq!(ctx.items.len(), 1);
    assert_eq!(ctx.items[0], "heading:Hello");
}

#[test]
fn slint_ctx_label() {
    let mut ctx = SlintCtx::default();
    ctx.label("test label");
    assert!(!ctx.items.is_empty());
    assert_eq!(ctx.items[0], "label:test label");
}

#[test]
fn slint_ctx_button_not_clicked() {
    let mut ctx = SlintCtx::default();
    let resp = ctx.button("OK");
    assert!(!resp.clicked, "headless button must not be clicked");
    assert_eq!(ctx.items[0], "button:OK");
}

#[test]
fn slint_ctx_multiple_widgets() {
    let mut ctx = SlintCtx::default();
    ctx.heading("Window");
    ctx.label("Status: ok");
    let _ = ctx.button("Continue");
    assert_eq!(ctx.items.len(), 3);
}

#[cfg(feature = "slint")]
#[test]
fn run_slint_headless_ok() {
    use oxiui_slint::run_slint;
    use oxiui_theme::cooljapan_default;

    let theme = cooljapan_default();
    run_slint(&*theme, |ui| {
        ui.heading("Slint test");
        ui.label("headless");
    })
    .expect("run_slint should return Ok in M5 headless mode");
}
