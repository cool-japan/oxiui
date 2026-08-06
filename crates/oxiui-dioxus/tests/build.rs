//! Build-time tests for `oxiui-dioxus`.
//!
//! These tests verify that [`DioxusCtx`] can be constructed and exercised
//! without opening a real window (headless / CI compatible).

use oxiui_core::UiCtx;
use oxiui_dioxus::DioxusCtx;

#[test]
fn dioxus_ctx_constructs() {
    let ctx = DioxusCtx::default();
    assert!(ctx.items.is_empty(), "fresh DioxusCtx should have no items");
}

#[test]
fn dioxus_ctx_heading() {
    let mut ctx = DioxusCtx::default();
    ctx.heading("Hello");
    assert_eq!(ctx.items.len(), 1);
    assert_eq!(ctx.items[0], "heading:Hello");
}

#[test]
fn dioxus_ctx_label() {
    let mut ctx = DioxusCtx::default();
    ctx.label("test label");
    assert!(!ctx.items.is_empty());
    assert_eq!(ctx.items[0], "label:test label");
}

#[test]
fn dioxus_ctx_button_not_clicked() {
    let mut ctx = DioxusCtx::default();
    let resp = ctx.button("OK");
    assert!(!resp.clicked, "headless button must not be clicked");
    assert_eq!(ctx.items[0], "button:OK");
}

#[test]
fn dioxus_ctx_multiple_widgets() {
    let mut ctx = DioxusCtx::default();
    ctx.heading("App Title");
    ctx.label("Hello, Dioxus!");
    let _ = ctx.button("Click me");
    assert_eq!(ctx.items.len(), 3);
}

#[test]
fn run_dioxus_reports_unsupported_window() {
    use oxiui_core::UiError;
    use oxiui_dioxus::run_dioxus;
    use oxiui_theme::cooljapan_default;

    // `run_dioxus` is contracted to launch a window; that path is not yet wired,
    // so it must surface a typed `UiError::Unsupported` rather than a fake
    // success. A caller must be able to tell "no window opened" from Ok.
    let theme = cooljapan_default();
    let err = run_dioxus(&*theme, |ui| {
        ui.heading("Dioxus test");
        ui.label("headless");
    })
    .expect_err("run_dioxus must not report success without opening a window");
    assert!(
        matches!(err, UiError::Unsupported(_)),
        "expected UiError::Unsupported, got {err:?}"
    );
}
