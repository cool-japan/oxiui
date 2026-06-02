//! Performance-regression tests for `oxiui-egui` caching and allocation optimisations.
//!
//! Covers:
//! - Visuals are recomputed only when the palette changes (not every frame).
//! - `set_fonts` is called at most once regardless of how many frames are run.
//! - `EguiUiCtx::button` returns `ButtonResponse` with correct fields (no
//!   hidden allocation path in the hot path).

use oxiui_core::{Color, Palette, UiCtx};
use oxiui_egui::{EguiUiCtx, StatefulEguiAdapter};

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_ctx() -> egui::Context {
    egui::Context::default()
}

fn make_palette(r: u8) -> Palette {
    Palette::new(
        Color(r, 27, 38, 255),
        Color(36, 40, 59, 255),
        Color(122, 162, 247, 255),
        Color(26, 27, 38, 255),
        Color(192, 202, 245, 255),
        Color(86, 95, 137, 255),
    )
}

fn run_adapter_frame(adapter: &mut StatefulEguiAdapter, ctx: &egui::Context) {
    let _ = ctx.run_ui(egui::RawInput::default(), |_ui| {
        adapter.apply(ctx);
    });
}

// ── 1. test_visuals_cache_not_recomputed ─────────────────────────────────────

/// Calling `apply` twice with an *unchanged* palette must recompute visuals
/// exactly once (on the first call), not on the second.
#[test]
fn test_visuals_cache_not_recomputed() {
    let ctx = make_ctx();
    let mut adapter = StatefulEguiAdapter::new().with_palette(make_palette(26));

    // Frame 1 — first call should trigger recompute.
    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(
        adapter.visuals_recompute_count, 1,
        "first apply with a palette must recompute visuals once"
    );

    // Frame 2 — palette unchanged; recompute must NOT happen again.
    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(
        adapter.visuals_recompute_count, 1,
        "second apply with the same palette must NOT recompute visuals"
    );
}

/// Changing the palette between frames must trigger a fresh recompute.
#[test]
fn test_visuals_recomputed_on_palette_change() {
    let ctx = make_ctx();
    let mut adapter = StatefulEguiAdapter::new().with_palette(make_palette(26));

    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(adapter.visuals_recompute_count, 1);

    // Change the palette.
    adapter.set_palette(make_palette(99));
    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(
        adapter.visuals_recompute_count, 2,
        "palette change must trigger a second recompute"
    );
}

// ── 2. test_fonts_loaded_only_once ───────────────────────────────────────────

/// `set_fonts` (tracked via `fonts_load_count`) must be attempted exactly
/// once, even when `apply` is invoked multiple times.
///
/// Empty bytes are supplied so `load_font_into_egui` is called (and fails
/// validation, which is silently ignored), confirming the one-shot behaviour.
#[test]
fn test_fonts_loaded_only_once() {
    let ctx = make_ctx();
    // Empty bytes → validation fails inside load_font_into_egui and is
    // silently discarded, but fonts_load_count still records the one attempt.
    let mut adapter = StatefulEguiAdapter::new().with_font_bytes(vec![0u8; 4]);

    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(
        adapter.fonts_load_count, 1,
        "fonts_load_count must be 1 after the first frame"
    );

    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(
        adapter.fonts_load_count, 1,
        "fonts_load_count must still be 1 after the second frame (loaded once)"
    );
}

/// Without any font bytes, `fonts_load_count` stays at 0 (no attempt made).
#[test]
fn test_no_fonts_no_load() {
    let ctx = make_ctx();
    let mut adapter = StatefulEguiAdapter::new();

    run_adapter_frame(&mut adapter, &ctx);
    assert_eq!(
        adapter.fonts_load_count, 0,
        "fonts_load_count must stay 0 when no font bytes were provided"
    );
}

// ── 3. test_button_no_string_alloc ───────────────────────────────────────────

/// `EguiUiCtx::button` forwards the `&str` label to egui directly without
/// allocating a `String`.  We verify the public contract: a `ButtonResponse`
/// is returned with `clicked == false` and `hovered == false` in headless mode.
/// (No click or hover events are injected, so both must be false.)
#[test]
fn test_button_no_string_alloc_contract() {
    let ctx = make_ctx();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut adapter = EguiUiCtx::new(ui);
        let resp = adapter.button("Click me");
        // In headless mode no pointer events are synthesised.
        assert!(
            !resp.clicked,
            "button.clicked must be false without a pointer event"
        );
        assert!(
            !resp.hovered,
            "button.hovered must be false without a hover event"
        );
    });
}

/// Multiple `button` calls in the same frame all return valid responses.
#[test]
fn test_button_multiple_calls_no_panic() {
    let ctx = make_ctx();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut adapter = EguiUiCtx::new(ui);
        for label in &["OK", "Cancel", "Apply", "Help"] {
            let resp = adapter.button(label);
            // No allocation path should panic.
            let _ = resp;
        }
    });
}

// ── 4. Adapter without palette — stable state ─────────────────────────────────

/// An adapter with no palette set must not panic on apply and must not
/// increment `visuals_recompute_count`.
#[test]
fn test_adapter_no_palette_stable() {
    let ctx = make_ctx();
    let mut adapter = StatefulEguiAdapter::new();

    run_adapter_frame(&mut adapter, &ctx);
    run_adapter_frame(&mut adapter, &ctx);

    assert_eq!(
        adapter.visuals_recompute_count, 0,
        "no palette set → visuals_recompute_count must stay 0"
    );
}
