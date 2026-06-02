/// Slice S7 / Round-5 headless tests for `oxiui-egui`.
///
/// Covers:
///   - `scroll_area` forwarding executes the child closure.
///   - `tooltip` forwarding does not panic.
///   - `popup` forwarding executes the child closure.
///   - `palette_to_egui_visuals` round-trip: primary colour maps to
///     `selection.bg_fill` and `hyperlink_color`.
///   - `scroll_area` returns a supported `WidgetResponse`.
use oxiui_core::{Color, Palette, UiCtx};
use oxiui_egui::{palette_to_egui_visuals, EguiUiCtx};

// ── headless harness ─────────────────────────────────────────────────────────

/// Run a closure against an `EguiUiCtx` in a headless egui frame.
///
/// Mirrors the pattern established in `slice_h_tests.rs` (Round-4).
fn run_ui<F: FnMut(&mut EguiUiCtx<'_>)>(mut f: F) {
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        f(&mut oxi);
    });
}

// ── scroll_area ───────────────────────────────────────────────────────────────

/// The scroll area closure must execute (child label rendered) without panicking,
/// and the outer response must be supported.
#[test]
fn test_scroll_area_executes_child() {
    let mut executed = false;
    run_ui(|ctx| {
        let r = ctx.scroll_area(&mut |ui| {
            ui.label("inside");
            executed = true;
        });
        assert!(r.supported, "scroll_area should return supported=true");
    });
    assert!(executed, "scroll_area closure must be executed");
}

// ── scroll_area returns supported ────────────────────────────────────────────

/// Explicit check that the returned `WidgetResponse` has `supported == true`.
#[test]
fn test_scroll_area_returns_supported() {
    run_ui(|ctx| {
        let r = ctx.scroll_area(&mut |ui| {
            ui.label("x");
        });
        assert!(r.supported);
    });
}

// ── tooltip ───────────────────────────────────────────────────────────────────

/// `tooltip` takes a `&str` (not a closure); it must not panic when there is no
/// preceding widget response (returns `unsupported` in that case).
#[test]
fn test_tooltip_no_panic_without_prior_response() {
    run_ui(|ctx| {
        // No previous widget — tooltip falls back to unsupported, but must not panic.
        let _r = ctx.tooltip("tip text");
    });
}

/// When a widget that stores `last_response` (e.g. `text_input`) has been
/// rendered first, `tooltip` attaches hover text to it and returns supported.
#[test]
fn test_tooltip_after_text_input_returns_supported() {
    run_ui(|ctx| {
        // text_input stores last_response; tooltip can then attach to it.
        let _ = ctx.text_input("initial text");
        let r = ctx.tooltip("hover tip");
        assert!(
            r.supported,
            "tooltip after a text_input should return supported=true"
        );
    });
}

// ── popup ─────────────────────────────────────────────────────────────────────

/// The popup closure must execute without panicking and return supported.
#[test]
fn test_popup_executes_child() {
    let mut executed = false;
    run_ui(|ctx| {
        let r = ctx.popup(&mut |ui| {
            ui.label("popup content");
            executed = true;
        });
        assert!(r.supported, "popup should return supported=true");
    });
    assert!(executed, "popup closure must be executed");
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Build a `Palette` with all fields filled in (no `Default` impl on `Palette`).
fn make_palette(
    background: Color,
    surface: Color,
    primary: Color,
    on_primary: Color,
    text: Color,
    muted: Color,
) -> Palette {
    Palette::new(background, surface, primary, on_primary, text, muted)
}

// ── palette round-trip ────────────────────────────────────────────────────────

/// `palette_to_egui_visuals` must map `palette.primary` to both
/// `visuals.selection.bg_fill` and `visuals.hyperlink_color`.
#[test]
fn test_palette_round_trip_primary_color() {
    let primary = Color(255, 0, 128, 255); // non-zero, distinctive hue
    let palette = make_palette(
        Color(20, 20, 20, 255),
        Color(30, 30, 30, 255),
        primary,
        Color(255, 255, 255, 255),
        Color(200, 200, 200, 255),
        Color(100, 100, 100, 255),
    );

    let visuals = palette_to_egui_visuals(&palette);

    let expected = egui::Color32::from_rgba_unmultiplied(255, 0, 128, 255);
    assert_eq!(
        visuals.selection.bg_fill, expected,
        "primary must map to selection.bg_fill"
    );
    assert_eq!(
        visuals.hyperlink_color, expected,
        "primary must map to hyperlink_color"
    );
}

/// `palette.background` must map to `visuals.panel_fill`.
#[test]
fn test_palette_round_trip_background_color() {
    let background = Color(30, 42, 55, 255);
    let palette = make_palette(
        background,
        Color(40, 40, 40, 255),
        Color(0, 120, 212, 255),
        Color(255, 255, 255, 255),
        Color(220, 220, 220, 255),
        Color(120, 120, 120, 255),
    );

    let visuals = palette_to_egui_visuals(&palette);

    let expected = egui::Color32::from_rgba_unmultiplied(30, 42, 55, 255);
    assert_eq!(
        visuals.panel_fill, expected,
        "background must map to panel_fill"
    );
}
