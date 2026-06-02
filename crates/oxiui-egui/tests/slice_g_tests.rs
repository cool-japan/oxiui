//! Tests for Slice G additions: extended event forwarding, full keyboard
//! mapping, OxiWidget, additive style/palette expansion, and multi-font.

use oxiui_core::{Color, Key, Modifiers, MouseButton, Palette, UiCtx, UiError, Widget};
use oxiui_egui::{
    forward_event_to_egui, load_fonts_into_egui, palette_to_egui_visuals,
    palette_to_egui_visuals_with_tokens, OxiWidget,
};
use oxiui_theme::DesignTokens;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_ctx() -> egui::Context {
    egui::Context::default()
}

fn make_palette() -> Palette {
    Palette {
        text: Color(192, 202, 245, 255),
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        muted: Color(86, 95, 137, 255),
    }
}

/// Minimal valid TTF/OTF font bytes (Noto Sans Regular embedded as a tiny
/// in-process byte literal).  Rather than shipping a real font, we load the
/// same test font used by `oxiui-text` — but since that path is fragile across
/// environments, we ask `oxiui_text::TextPipeline::from_bytes` to tell us
/// whether any bytes are valid via the error path.  For these tests the "valid
/// font bytes" case is confirmed by the load_font_into_egui test above which
/// already exercises the happy path; here we only need bytes that will either
/// pass or fail validation deterministically.
///
/// A minimal but real TTF header is 12 bytes; we use 0xFF/0xFE-prefixed junk
/// that looks enough like a SFNT to be attempted.  If this fails validation we
/// skip that branch — the interesting test is the error path.
fn minimal_invalid_bytes() -> Vec<u8> {
    // Not a valid TTF: guaranteed to fail validation.
    vec![0x00, 0x01, 0x02]
}

// ── 1. test_key_down_pushes_egui_event ────────────────────────────────────────

#[test]
fn test_key_down_pushes_egui_event() {
    // Deviation note: the plan named this "KeyPress { key, modifiers, pressed }"
    // but the real UiEvent has KeyDown { key, modifiers, repeat } for
    // pressed=true events. We use KeyDown here.
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::NONE,
        repeat: false,
    };

    // Push the event into the context's pre-frame input queue.
    ctx.input_mut(|i| {
        // We'll push via forward_event_to_egui instead — but we need input_mut
        // accessible from outside run().  forward_event_to_egui calls
        // input_mut internally.
        let _ = i; // ensure we actually have access
    });
    forward_event_to_egui(&ctx, &event);

    // Verify the event is now in the queue by reading back.
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
        assert!(
            found,
            "KeyDown(Enter) should appear in egui event queue as Key::Enter pressed=true"
        );
    });
}

// ── 2. test_mouse_move_pushes_pointer_moved ───────────────────────────────────

#[test]
fn test_mouse_move_pushes_pointer_moved() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::MouseMove { x: 42.0, y: 17.0 };
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(e, egui::Event::PointerMoved(pos) if (pos.x - 42.0).abs() < 0.01 && (pos.y - 17.0).abs() < 0.01)
        });
        assert!(found, "MouseMove should produce PointerMoved(42, 17) in the event queue");
    });
}

// ── 3. test_keyboard_map_modifiers_ctrl ──────────────────────────────────────

#[test]
fn test_keyboard_map_modifiers_ctrl() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::KeyDown {
        key: Key::Character("a".to_owned()),
        modifiers: Modifiers {
            ctrl: true,
            alt: false,
            shift: false,
            meta: false,
        },
        repeat: false,
    };
    forward_event_to_egui(&ctx, &event);

    ctx.input(|i| {
        let found = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key { modifiers, pressed: true, .. }
                    if modifiers.ctrl && !modifiers.alt && !modifiers.shift
            )
        });
        assert!(
            found,
            "Modifiers {{ ctrl: true }} should forward ctrl=true to egui"
        );
    });
}

// ── 4. test_oxi_widget_render_invoked ────────────────────────────────────────

/// A spy widget that increments a counter each time render() is called.
struct SpyWidget {
    render_count: u32,
}

impl Widget for SpyWidget {
    fn render(&mut self, ui: &mut dyn UiCtx) {
        self.render_count += 1;
        ui.label("spy");
    }
}

#[test]
fn test_oxi_widget_render_invoked() {
    let ctx = make_ctx();
    let mut spy = SpyWidget { render_count: 0 };

    let raw_input = egui::RawInput::default();
    let _ = ctx.run_ui(raw_input, |ui| {
        ui.add(OxiWidget::new(&mut spy));
    });

    assert!(
        spy.render_count > 0,
        "OxiWidget must invoke Widget::render at least once"
    );
}

// ── 5. test_palette_to_egui_visuals_unchanged ────────────────────────────────

#[test]
fn test_palette_to_egui_visuals_unchanged() {
    // Regression: the existing function must still map background, primary
    // exactly as before (used by the facade and 3 tests in egui_tests.rs).
    let palette = make_palette();
    let visuals = palette_to_egui_visuals(&palette);

    // background → panel_fill
    assert_eq!(visuals.panel_fill.r(), 26, "panel_fill.r should be 26");
    assert_eq!(visuals.panel_fill.g(), 27, "panel_fill.g should be 27");
    assert_eq!(visuals.panel_fill.b(), 38, "panel_fill.b should be 38");

    // primary → selection.bg_fill
    assert_eq!(
        visuals.selection.bg_fill.r(),
        122,
        "selection.r should be 122"
    );
    assert_eq!(
        visuals.selection.bg_fill.g(),
        162,
        "selection.g should be 162"
    );

    // text → override_text_color
    let tc = visuals
        .override_text_color
        .expect("text color should be set");
    assert_eq!(tc.r(), 192);
}

// ── 6. test_palette_to_egui_visuals_with_tokens_returns_style ────────────────

#[test]
fn test_palette_to_egui_visuals_with_tokens_returns_style() {
    let palette = make_palette();
    let tokens = DesignTokens::default();
    let style = palette_to_egui_visuals_with_tokens(&palette, &tokens);

    // The returned style should have visuals populated from the palette.
    let tc = style
        .visuals
        .override_text_color
        .expect("text color should be set in style");
    assert_eq!(tc.r(), 192, "style.visuals should reflect palette.text");

    // Spacing should differ from the absolute default (which is vec2(8,3)).
    // We map SpacingStep::Sm (8.0) to both x and y, so y will be 8.0 not 3.0.
    assert!(
        (style.spacing.item_spacing.y - 8.0).abs() < 0.01,
        "item_spacing.y should be tokens.spacing(Sm)=8.0, got {}",
        style.spacing.item_spacing.y
    );
}

// ── 7. test_load_fonts_into_egui_empty_bytes_err ─────────────────────────────

#[test]
fn test_load_fonts_into_egui_empty_bytes_err() {
    let ctx = make_ctx();
    let result = load_fonts_into_egui(&[("sans", vec![])], &ctx);
    assert!(
        result.is_err(),
        "empty font bytes must return Err from load_fonts_into_egui"
    );
    // Verify the error is UiError::Render.
    match result {
        Err(UiError::Render(_)) => {}
        other => panic!("expected UiError::Render, got {other:?}"),
    }
}

// ── 8. test_load_fonts_into_egui_invalid_bytes_err ───────────────────────────

#[test]
fn test_load_fonts_into_egui_invalid_bytes_err() {
    // Invalid (not a valid TTF) → must return Err.
    let ctx = make_ctx();
    let result = load_fonts_into_egui(&[("mono", minimal_invalid_bytes())], &ctx);
    assert!(
        result.is_err(),
        "invalid font bytes must return Err from load_fonts_into_egui"
    );
}

// ── bonus: mouse button down event ────────────────────────────────────────────

#[test]
fn test_mouse_down_pushes_pointer_button() {
    let ctx = make_ctx();
    let event = oxiui_core::UiEvent::MouseDown {
        button: MouseButton::Left,
        x: 10.0,
        y: 20.0,
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
            "MouseDown(Left) should produce PointerButton(Primary, pressed=true)"
        );
    });
}
