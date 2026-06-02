//! Headless egui tests for `EguiUiCtx` extended widget forwarding.
//!
//! Uses `egui::Context::run` with a `CentralPanel` to drive widgets in a
//! frame without a real window or GPU.

use oxiui_core::UiCtx;
use oxiui_egui::EguiUiCtx;

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_ctx() -> egui::Context {
    egui::Context::default()
}

fn run_frame<F: FnMut(&mut EguiUiCtx<'_>)>(ctx: &egui::Context, mut f: F) {
    let raw_input = egui::RawInput::default();
    let _ = ctx.run_ui(raw_input, |ui| {
        let mut adapter = EguiUiCtx::new(ui);
        f(&mut adapter);
    });
}

// ── text_input ────────────────────────────────────────────────────────────────

#[test]
fn text_input_forwards_seed_and_reports_supported() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.text_input("hello");
        assert!(resp.supported);
        // Seed text is forwarded: no user edit, so text should equal the seed.
        assert_eq!(resp.text, "hello");
    });
}

// ── checkbox ─────────────────────────────────────────────────────────────────

#[test]
fn checkbox_forwards_state() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.checkbox("opt", true);
        assert!(resp.supported);
        // Initial state is preserved (no click in headless mode).
        assert!(resp.checked);
    });
}

// ── slider ───────────────────────────────────────────────────────────────────

#[test]
fn slider_reports_value_in_range() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.slider(0.5, 0.0..=1.0);
        assert!(resp.supported);
        // The value must remain within the supplied range.
        assert!(resp.value >= 0.0 && resp.value <= 1.0);
    });
}

// ── dropdown ─────────────────────────────────────────────────────────────────

#[test]
fn dropdown_returns_selected() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.dropdown(&["a", "b", "c"], 1);
        assert!(resp.supported);
        // No interaction in headless — selection is unchanged.
        assert_eq!(resp.selected, 1);
    });
}

#[test]
fn dropdown_two_widgets_no_id_panic() {
    // Two dropdowns on the SAME adapter so id_seq increments (0 → 1).
    // This guards that distinct salts are generated even within one frame.
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let r1 = adapter.dropdown(&["x", "y"], 0);
        let r2 = adapter.dropdown(&["p", "q"], 0);
        assert!(r1.supported);
        assert!(r2.supported);
    });
}

// ── separator ────────────────────────────────────────────────────────────────

#[test]
fn separator_supported() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.separator();
        assert!(resp.supported);
    });
}

// ── spacer ───────────────────────────────────────────────────────────────────

#[test]
fn spacer_supported() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.spacer(16.0);
        assert!(resp.supported);
    });
}

// ── image ────────────────────────────────────────────────────────────────────

#[test]
fn image_supported_without_loader() {
    // egui::Image will try to fetch the URI asynchronously; in headless mode
    // without a loader installed it renders a placeholder but does not panic.
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.image("file://nonexistent.png", None);
        assert!(resp.supported);
    });
}

// ── scroll_area ───────────────────────────────────────────────────────────────

#[test]
fn scroll_area_invokes_content_closure() {
    let ctx = make_ctx();
    let mut invoked = false;
    run_frame(&ctx, |adapter| {
        let resp = adapter.scroll_area(&mut |inner| {
            invoked = true;
            inner.label("inside scroll");
        });
        assert!(resp.supported);
    });
    assert!(invoked, "scroll_area must invoke the content closure");
}

// ── tooltip ───────────────────────────────────────────────────────────────────

#[test]
fn tooltip_no_prior_widget_unsupported() {
    // When no widget has been rendered yet, tooltip returns unsupported.
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        let resp = adapter.tooltip("tip text");
        assert!(!resp.supported);
    });
}

#[test]
fn tooltip_attaches_after_separator() {
    // After a separator is rendered, tooltip should succeed.
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        adapter.separator();
        let resp = adapter.tooltip("hover text");
        assert!(resp.supported);
    });
}

// ── response accessor ─────────────────────────────────────────────────────────

#[test]
fn response_accessor_returns_last_widget() {
    let ctx = make_ctx();
    run_frame(&ctx, |adapter| {
        // Initially no response.
        assert!(adapter.response().is_none());
        // After rendering a separator, response is populated.
        adapter.separator();
        assert!(adapter.response().is_some());
    });
}

// ── popup ─────────────────────────────────────────────────────────────────────

#[test]
fn popup_invokes_content_closure() {
    let ctx = make_ctx();
    let mut invoked = false;
    run_frame(&ctx, |adapter| {
        let resp = adapter.popup(&mut |inner| {
            invoked = true;
            inner.label("popup content");
        });
        assert!(resp.supported);
    });
    assert!(invoked, "popup must invoke the content closure");
}

// ── modal ─────────────────────────────────────────────────────────────────────

#[test]
fn modal_invokes_content_closure() {
    let ctx = make_ctx();
    let mut invoked = false;
    run_frame(&ctx, |adapter| {
        let resp = adapter.modal("Dialog", &mut |inner| {
            invoked = true;
            inner.label("modal body");
        });
        assert!(resp.supported);
    });
    assert!(invoked, "modal must invoke the content closure");
}

// ── load_font_into_egui ───────────────────────────────────────────────────────

#[test]
fn load_font_empty_bytes_is_err() {
    let ctx = make_ctx();
    let result = oxiui_egui::load_font_into_egui(&ctx, vec![]);
    assert!(result.is_err(), "empty font bytes must return Err");
}
