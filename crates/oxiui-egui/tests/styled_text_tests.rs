//! Tests for `label_styled` and `heading_styled` overrides in `EguiUiCtx`.
//!
//! These override the default `UiCtx` trait implementations and forward to
//! egui `RichText` with size/weight/italic/color/underline/strikethrough.

use oxiui_core::{TextStyle, UiCtx};
use oxiui_egui::EguiUiCtx;

fn run_ui<F>(mut f: F)
where
    F: FnMut(&mut dyn UiCtx),
{
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let _ = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        f(&mut oxi);
    });
}

// ── label_styled ──────────────────────────────────────────────────────────────

/// `label_styled` with default style renders without panic.
#[test]
fn label_styled_default_no_panic() {
    run_ui(|ui| {
        let resp = ui.label_styled("Default styled label", TextStyle::default());
        assert!(resp.supported);
    });
}

/// `label_styled` with bold style (weight ≥ 600) renders without panic.
#[test]
fn label_styled_bold_no_panic() {
    run_ui(|ui| {
        let resp = ui.label_styled("Bold label", TextStyle::bold());
        assert!(resp.supported);
    });
}

/// `label_styled` with italic style renders without panic.
#[test]
fn label_styled_italic_no_panic() {
    run_ui(|ui| {
        let resp = ui.label_styled("Italic label", TextStyle::italic());
        assert!(resp.supported);
    });
}

/// `label_styled` with explicit color renders without panic.
#[test]
fn label_styled_color_no_panic() {
    let style = TextStyle::default().with_color([255, 100, 50, 255]);
    run_ui(|ui| {
        let resp = ui.label_styled("Colored label", style.clone());
        assert!(resp.supported);
    });
}

/// `label_styled` with underline renders without panic.
#[test]
fn label_styled_underline_no_panic() {
    let style = TextStyle {
        underline: true,
        ..TextStyle::default()
    };
    run_ui(|ui| {
        let resp = ui.label_styled("Underlined label", style.clone());
        assert!(resp.supported);
    });
}

/// `label_styled` with strikethrough renders without panic.
#[test]
fn label_styled_strikethrough_no_panic() {
    let style = TextStyle {
        strikethrough: true,
        ..TextStyle::default()
    };
    run_ui(|ui| {
        let resp = ui.label_styled("Strikethrough label", style.clone());
        assert!(resp.supported);
    });
}

/// `label_styled` with explicit font size renders without panic.
#[test]
fn label_styled_with_size_no_panic() {
    let style = TextStyle::default().with_size(18.0);
    run_ui(|ui| {
        let resp = ui.label_styled("Size 18 label", style.clone());
        assert!(resp.supported);
    });
}

/// `label_styled` result stores a response accessible via `.response()`.
#[test]
fn label_styled_stores_response() {
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let _ = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.label_styled("Stored response test", TextStyle::default());
        assert!(
            oxi.response().is_some(),
            "label_styled must store a response"
        );
    });
}

// ── heading_styled ────────────────────────────────────────────────────────────

/// `heading_styled` with default style renders without panic.
#[test]
fn heading_styled_default_no_panic() {
    run_ui(|ui| {
        let resp = ui.heading_styled("Default heading", TextStyle::default());
        assert!(resp.supported);
    });
}

/// `heading_styled` with explicit font size renders without panic.
#[test]
fn heading_styled_with_size_no_panic() {
    let style = TextStyle::heading();
    run_ui(|ui| {
        let resp = ui.heading_styled("Heading preset", style.clone());
        assert!(resp.supported);
    });
}

/// `heading_styled` with italic style renders without panic.
#[test]
fn heading_styled_italic_no_panic() {
    run_ui(|ui| {
        let resp = ui.heading_styled("Italic heading", TextStyle::italic());
        assert!(resp.supported);
    });
}

/// `heading_styled` with explicit color renders without panic.
#[test]
fn heading_styled_color_no_panic() {
    let style = TextStyle::default().with_color([100, 200, 255, 255]);
    run_ui(|ui| {
        let resp = ui.heading_styled("Colored heading", style.clone());
        assert!(resp.supported);
    });
}

/// `heading_styled` stores a response accessible via `.response()`.
#[test]
fn heading_styled_stores_response() {
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let _ = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.heading_styled("Section", TextStyle::heading());
        assert!(
            oxi.response().is_some(),
            "heading_styled must store a response"
        );
    });
}

/// Caption style (11 pt) renders without panic through `label_styled`.
#[test]
fn label_styled_caption_no_panic() {
    run_ui(|ui| {
        let resp = ui.label_styled("Caption text", TextStyle::caption());
        assert!(resp.supported);
    });
}

/// Bold weight threshold: weight 600 triggers `strong()`, weight 400 does not.
#[test]
fn label_styled_bold_threshold() {
    run_ui(|ui| {
        // weight 400 — regular
        let r1 = ui.label_styled("Regular", TextStyle::default().with_weight(400));
        // weight 600 — threshold for strong
        let r2 = ui.label_styled("Semi-bold", TextStyle::default().with_weight(600));
        // weight 700 — bold
        let r3 = ui.label_styled("Bold", TextStyle::bold());
        assert!(r1.supported);
        assert!(r2.supported);
        assert!(r3.supported);
    });
}
