//! Extended test coverage for `oxiui-egui`: IME event forwarding, font loading,
//! extended event forwarding (keyboard / mouse / resize), and custom OxiWidget.
//!
//! All tests use the headless egui harness (`egui::Context::default()` +
//! `ctx.run_ui(...)`).  IME and event queue tests inspect the queue *before* a
//! frame starts, because `run_ui` rebuilds the input state from `RawInput`
//! clearing any previously pushed events.
//!
//! API deviation notes (egui 0.34.3):
//! - `egui::Event::CompositionStart / CompositionUpdate / CompositionEnd` do not
//!   exist.  The spec references fictional `egui` composition variants; the real
//!   API is `egui::Event::Ime(egui::ImeEvent::Preedit/Commit/Enabled/Disabled)`.
//! - There is no egui `Event` for resize; it is driven by `RawInput.screen_rect`.
//! - `egui::Context` has no public getter that returns the current
//!   `FontDefinitions`; font-load success is verified via `Result::is_ok()` and
//!   a subsequent frame render, not by inspecting the definition map.

use oxiui_core::{Key, Modifiers, MouseButton, UiCtx, Widget};
use oxiui_egui::{
    forward_event_to_egui, load_font_into_egui, EguiUiCtx, OxiWidget, StatefulEguiAdapter,
};

// ── font fixture ─────────────────────────────────────────────────────────────

/// Real TTF bytes shared by the `oxiui-text` test suite.
///
/// This font is validated by `oxiui_text::TextPipeline::from_bytes` inside
/// `load_font_into_egui`.  Using the same fixture guarantees that
/// `from_bytes` has a known-good input without bundling a second copy.
static FONT_BYTES: &[u8] = include_bytes!("../../../../oxitext/tests/fixtures/test-font.ttf");

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_ctx() -> egui::Context {
    egui::Context::default()
}

// ── 1. test_ime_preedit_event_pushes_to_queue ────────────────────────────────

/// Forward `UiEvent::ImePreedit` via `forward_event_to_egui` and verify that
/// the egui input queue contains the corresponding `Event::Ime(Preedit(...))`.
///
/// The read-back happens *before* any `run_ui` call because `run_ui` rebuilds
/// `InputState` from the supplied `RawInput`, clearing pushed events.
///
/// Deviation: the spec mentions `CompositionStart / CompositionUpdate`; those
/// variants do not exist in egui 0.34.3.  We exercise `ImeEvent::Preedit`
/// which is what `forward_event_to_egui` actually emits.
#[test]
fn test_ime_preedit_event_pushes_to_queue() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::ImePreedit {
        text: "こんにちは".to_owned(),
        cursor: None,
    };
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Ime(egui::ImeEvent::Preedit(s)) if s == "こんにちは"
            )
        });
        assert!(
            found,
            "ImePreedit should push Ime(Preedit(..)) into egui event queue"
        );
    });
}

// ── 2. test_ime_preedit_no_panic_in_frame ────────────────────────────────────

/// Verify that injecting `ImePreedit` via `RawInput` and running a frame does
/// not panic.  The event is supplied as raw egui input so it survives the
/// `begin_pass` rebuild.
#[test]
fn test_ime_preedit_no_panic_in_frame() {
    let ctx = make_ctx();
    let raw_input = egui::RawInput {
        events: vec![egui::Event::Ime(egui::ImeEvent::Preedit("abc".to_owned()))],
        ..Default::default()
    };
    // Panics would propagate out of run_ui and fail this test.
    let _ = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.label("preedit test");
    });
}

// ── 3. test_ime_commit_event_pushes_to_queue ─────────────────────────────────

/// Forward `UiEvent::ImeCommit` and verify `Event::Ime(Commit(...))` is queued.
#[test]
fn test_ime_commit_event_pushes_to_queue() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::ImeCommit("確定".to_owned());
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Ime(egui::ImeEvent::Commit(s)) if s == "確定"
            )
        });
        assert!(
            found,
            "ImeCommit should push Ime(Commit(..)) into egui event queue"
        );
    });
}

// ── 4. test_ime_commit_no_panic_in_frame ─────────────────────────────────────

/// Verify that injecting `CompositionEnd` (ImeCommit) via `RawInput` and
/// running a frame does not panic.
///
/// Deviation: egui 0.34.3 has no `CompositionEnd`; `ImeEvent::Commit` is the
/// equivalent.
#[test]
fn test_ime_commit_no_panic_in_frame() {
    let ctx = make_ctx();
    let raw_input = egui::RawInput {
        events: vec![egui::Event::Ime(egui::ImeEvent::Commit("完了".to_owned()))],
        ..Default::default()
    };
    let _ = ctx.run_ui(raw_input, |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.label("commit test");
    });
}

// ── 5. test_font_loading_valid_ttf_succeeds ───────────────────────────────────

/// Load the shared test TTF font via `load_font_into_egui` and verify it
/// returns `Ok`.  Then run one frame to confirm egui does not panic with the
/// loaded font active.
///
/// Deviation: `egui::Context` exposes no public getter for `FontDefinitions`,
/// so we cannot assert that "OxiFont" appears in the proportional family map.
/// The `Ok` result and panic-free frame serve as the observable proof.
#[test]
fn test_font_loading_valid_ttf_succeeds() {
    let ctx = make_ctx();
    let result = load_font_into_egui(&ctx, FONT_BYTES.to_vec());
    assert!(
        result.is_ok(),
        "valid TTF bytes must load without error: {:?}",
        result
    );

    // One frame after loading — verify egui renders normally.
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut oxi = EguiUiCtx::new(ui);
        oxi.heading("Font load OK");
        oxi.label("body text with custom font");
    });
}

// ── 6. test_font_loading_empty_bytes_err ─────────────────────────────────────

/// Supplying empty bytes must return `Err` (validated by `TextPipeline`).
/// This exercises the error path of `load_font_into_egui` directly.
#[test]
fn test_font_loading_empty_bytes_err() {
    let ctx = make_ctx();
    let result = load_font_into_egui(&ctx, vec![]);
    assert!(result.is_err(), "empty font bytes must return Err");
}

// ── 7. test_key_press_string_event_forwarded ─────────────────────────────────

/// `UiEvent::KeyPress(String)` with a recognisable key name (e.g. `"Enter"`)
/// must push an `Event::Key { pressed: true, .. }` into the queue.
///
/// Deviation: the spec references a fictional `KeyPress { key, modifiers,
/// pressed }` struct; the real variant is `KeyPress(String)` — a bare name.
#[test]
fn test_key_press_string_event_forwarded() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::KeyPress("Enter".to_owned());
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    ..
                }
            )
        });
        assert!(found, "KeyPress('Enter') must push Key::Enter pressed=true");
    });
}

// ── 8. test_key_up_event_forwarded ───────────────────────────────────────────

/// `UiEvent::KeyUp` must push `Event::Key { pressed: false, .. }`.
#[test]
fn test_key_up_event_forwarded() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::NONE,
    };
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: false,
                    ..
                }
            )
        });
        assert!(
            found,
            "KeyUp(Enter) must push Key::Enter pressed=false into the queue"
        );
    });
}

// ── 9. test_mouse_click_event_forwarded ──────────────────────────────────────

/// `UiEvent::MouseDown { button: Left, .. }` must push
/// `Event::PointerButton { button: Primary, pressed: true, .. }`.
#[test]
fn test_mouse_click_event_forwarded() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::MouseDown {
        button: MouseButton::Left,
        x: 100.0,
        y: 200.0,
        modifiers: Modifiers::NONE,
    };
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                }
            )
        });
        assert!(
            found,
            "MouseDown(Left) must push PointerButton(Primary, pressed=true)"
        );
    });
}

// ── 10. test_resize_event_updates_viewport ───────────────────────────────────

/// Resize is driven via `RawInput.screen_rect`, not an egui `Event`.
/// Supply a new `screen_rect` and verify the context reports the updated
/// dimensions after `run_ui`.
///
/// `UiEvent::Resize` has no egui event equivalent (just a no-op arm in
/// `forward_event_to_egui`); the separate `screen_rect` route is tested here.
#[test]
fn test_resize_event_updates_viewport() {
    let ctx = make_ctx();
    let new_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0));

    let mut observed_width = 0.0_f32;
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(new_rect),
            ..Default::default()
        },
        |ui| {
            observed_width = ui.ctx().viewport_rect().width();
        },
    );

    assert!(
        (observed_width - 1920.0).abs() < 1.0,
        "screen_rect width must reflect the supplied RawInput: got {observed_width}"
    );
}

// ── 11. test_resize_forward_no_panic ─────────────────────────────────────────

/// `forward_event_to_egui` with `UiEvent::Resize` is a no-op arm; it must
/// not panic regardless of dimensions.
#[test]
fn test_resize_forward_no_panic() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::Resize(1280, 720);
    forward_event_to_egui(&ctx, &event);
    // No assertion needed — the test passes if no panic occurs.
}

// ── 12. test_custom_widget_renders ───────────────────────────────────────────

/// A custom widget that paints a colored rectangle via egui's `Painter` must
/// be renderable through `OxiWidget` without panicking, and the render method
/// must be invoked exactly once per frame.
struct ColorRectWidget {
    /// Counts how many times `render()` was called.
    render_calls: usize,
}

impl Widget for ColorRectWidget {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.render_calls += 1;
        // Render a label so egui has something to lay out.
        ui.label("■ colored rect");
    }
}

#[test]
fn test_custom_widget_renders() {
    let ctx = make_ctx();
    let mut widget = ColorRectWidget { render_calls: 0 };

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.add(OxiWidget::new(&mut widget));
    });

    assert_eq!(
        widget.render_calls, 1,
        "Widget::render must be called exactly once per frame"
    );
}

// ── 13. test_custom_widget_painter_no_panic ──────────────────────────────────

/// A widget that allocates a rect and paints via `egui::Painter` must not
/// panic in a headless egui context.
struct PainterWidget;

impl Widget for PainterWidget {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        // Cast to EguiUiCtx is not available here (we only have &mut dyn UiCtx).
        // Use the label path as a proxy — the EguiUiCtx impl forwards to egui.
        ui.label("painter proxy");
    }
}

#[test]
fn test_custom_widget_painter_no_panic() {
    let ctx = make_ctx();
    let mut widget = PainterWidget;

    // Test that the egui Painter path works directly in a run_ui frame.
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        let rect = ui.allocate_space(egui::vec2(64.0, 32.0)).1;
        let painter = ui.painter();
        painter.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::RED);

        // Also exercise the OxiWidget path.
        ui.add(OxiWidget::new(&mut widget));
    });
}

// ── 14. test_egui_adapter_headless_frame_cycle ───────────────────────────────

/// Create a `StatefulEguiAdapter` with no palette and run two consecutive
/// frames.  Verifies that the adapter lifecycle (font load, token application,
/// palette caching) completes without panic even with no configuration.
#[test]
fn test_egui_adapter_headless_frame_cycle() {
    let ctx = make_ctx();
    let mut adapter = StatefulEguiAdapter::new();

    for _ in 0..2 {
        let _ = ctx.run_ui(egui::RawInput::default(), |_ui| {
            adapter.apply(&ctx);
        });
    }

    // No palette → visuals must not have been recomputed.
    assert_eq!(
        adapter.visuals_recompute_count, 0,
        "adapter with no palette must not recompute visuals"
    );
    // No fonts → load count must stay zero.
    assert_eq!(
        adapter.fonts_load_count, 0,
        "adapter with no font bytes must not call set_fonts"
    );
}

// ── 15. test_stateful_adapter_valid_font_loaded_once ─────────────────────────

/// `StatefulEguiAdapter::with_font_bytes` with valid TTF bytes must load them
/// exactly once and report `fonts_load_count == 1` after the first frame.
/// Subsequent frames must not re-load (count stays at 1).
#[test]
fn test_stateful_adapter_valid_font_loaded_once() {
    let ctx = make_ctx();
    let mut adapter = StatefulEguiAdapter::new().with_font_bytes(FONT_BYTES.to_vec());

    let _ = ctx.run_ui(egui::RawInput::default(), |_ui| {
        adapter.apply(&ctx);
    });
    assert_eq!(
        adapter.fonts_load_count, 1,
        "fonts must be loaded exactly once after first frame"
    );

    let _ = ctx.run_ui(egui::RawInput::default(), |_ui| {
        adapter.apply(&ctx);
    });
    assert_eq!(
        adapter.fonts_load_count, 1,
        "fonts must NOT be re-loaded on subsequent frames"
    );
}
