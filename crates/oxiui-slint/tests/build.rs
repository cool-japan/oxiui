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

#[test]
fn run_slint_reports_unsupported_window() {
    use oxiui_core::UiError;
    use oxiui_slint::run_slint;
    use oxiui_theme::cooljapan_default;

    // `run_slint` is contracted to open a native window; that path is not yet
    // wired, so it must surface a typed `UiError::Unsupported` rather than a
    // fake success. A caller must be able to tell "no window opened" from Ok.
    let theme = cooljapan_default();
    let err = run_slint(&*theme, |ui| {
        ui.heading("Slint test");
        ui.label("headless");
    })
    .expect_err("run_slint must not report success without opening a window");
    assert!(
        matches!(err, UiError::Unsupported(_)),
        "expected UiError::Unsupported, got {err:?}"
    );
}
