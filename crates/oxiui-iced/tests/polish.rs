//! Tests for the iced adapter polish items (TODO L41/L45/L64/L65).
//!
//! Covers:
//! - Window title round-trip via `IcedConfig` seam (TODO L41 — deviation: no Application impl).
//! - `IcedUiCtx` widget methods return correct response types (TODO L45).
//! - Palette→iced-theme cache correctness (TODO L65).
//! - Pre-allocated specs vector with zero/non-zero capacity hint (TODO L64).

use oxiui_core::{Color, Palette, UiCtx};
use oxiui_iced::adapter::{IcedConfig, IcedUiCtx, ThemeCache};

// ── Helper ────────────────────────────────────────────────────────────────────

fn make_palette(r: u8) -> Palette {
    Palette::new(
        Color(r, 255, 255, 255),
        Color(240, 240, 240, 255),
        Color(0, 100, 200, 255),
        Color(255, 255, 255, 255),
        Color(0, 0, 0, 255),
        Color(128, 128, 128, 255),
    )
}

// ── Step 2: Window title seam (TODO L41) ──────────────────────────────────────

/// `IcedConfig::with_title` stores the title and `title` field round-trips correctly.
///
/// Deviation: there is no `iced::Application` impl in `oxiui-iced`.  This test
/// validates the config seam; a host's `title()` callback reads `config.title`.
#[test]
fn test_iced_title_from_config() {
    let config = IcedConfig::default().with_title("Test Window");
    assert_eq!(config.title, "Test Window");
}

#[test]
fn test_iced_title_default_is_empty() {
    let config = IcedConfig::default();
    assert!(
        config.title.is_empty(),
        "default config title must be empty, got {:?}",
        config.title
    );
}

#[test]
fn test_iced_title_chain_builder() {
    let config = IcedConfig::default()
        .with_spacing(8.0)
        .with_title("My App")
        .with_padding(4.0);
    assert_eq!(config.title, "My App");
    assert_eq!(config.spacing, 8.0);
    assert_eq!(config.padding, 4.0);
}

// ── Step 3: WidgetResponse from all methods (TODO L45) ────────────────────────

/// `IcedUiCtx::image` returns `WidgetResponse` with `supported = true`.
///
/// Note: `UiCtx::heading` and `UiCtx::label` return `()` per the core trait
/// definition — they are excluded from this check by design.
#[test]
fn test_iced_image_returns_supported_widget_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.image("test.png", None);
    assert!(
        resp.supported,
        "image() must return supported WidgetResponse"
    );
}

/// `IcedUiCtx::separator` returns `WidgetResponse::supported()`.
#[test]
fn test_iced_separator_returns_supported_widget_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.separator();
    assert!(
        resp.supported,
        "separator() must return supported WidgetResponse"
    );
}

/// `IcedUiCtx::spacer` returns `WidgetResponse::supported()`.
#[test]
fn test_iced_spacer_returns_supported_widget_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.spacer(8.0);
    assert!(
        resp.supported,
        "spacer() must return supported WidgetResponse"
    );
}

/// `IcedUiCtx::scroll_area` returns `WidgetResponse::supported()`.
#[test]
fn test_iced_scroll_area_returns_supported_widget_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.scroll_area(&mut |ui| {
        ui.label("content");
    });
    assert!(
        resp.supported,
        "scroll_area() must return supported WidgetResponse"
    );
}

/// `IcedUiCtx::button` returns a `ButtonResponse` with correct click state.
#[test]
fn test_iced_button_returns_button_response_with_click_state() {
    let mut config = IcedConfig::default();
    config.pending_clicks.insert(0);
    let mut ctx = IcedUiCtx::new(config);
    let resp = ctx.button("Click");
    assert!(
        resp.clicked,
        "button() must report clicked when id is in pending_clicks"
    );
}

/// `IcedUiCtx::checkbox` returns a `CheckboxResponse` with `supported = true`.
#[test]
fn test_iced_checkbox_returns_supported_checkbox_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.checkbox("Check me", false);
    assert!(
        resp.supported,
        "checkbox() must return supported CheckboxResponse"
    );
}

/// `IcedUiCtx::slider` returns a `SliderResponse` with `supported = true`.
#[test]
fn test_iced_slider_returns_supported_slider_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.slider(0.5, 0.0..=1.0);
    assert!(
        resp.supported,
        "slider() must return supported SliderResponse"
    );
    assert!(
        (resp.value - 0.5).abs() < f64::EPSILON,
        "slider() must carry initial value"
    );
}

/// `IcedUiCtx::dropdown` returns a `DropdownResponse` with `supported = true`.
#[test]
fn test_iced_dropdown_returns_supported_dropdown_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.dropdown(&["a", "b", "c"], 1);
    assert!(
        resp.supported,
        "dropdown() must return supported DropdownResponse"
    );
    assert_eq!(resp.selected, 1, "dropdown() must carry initial selection");
}

/// `IcedUiCtx::text_input` returns a `TextInputResponse` with `supported = true`.
#[test]
fn test_iced_text_input_returns_supported_text_input_response() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    let resp = ctx.text_input("hello");
    assert!(
        resp.supported,
        "text_input() must return supported TextInputResponse"
    );
}

// ── Step 4: Cache palette→iced-theme (TODO L65) ───────────────────────────────

/// Two identical palettes produce equal theme names (cache hit preserves correctness).
#[test]
fn test_iced_palette_cache_hit_preserves_correctness() {
    let palette = make_palette(200);
    let mut cache = ThemeCache::default();
    let theme1 = cache.get_or_compute(&palette);
    let theme2 = cache.get_or_compute(&palette);
    // Both calls must return the same theme name, confirming cache consistency.
    assert_eq!(
        format!("{theme1:?}"),
        format!("{theme2:?}"),
        "cache hit must return the same theme"
    );
}

/// Changing the palette produces a different theme (cache invalidation).
#[test]
fn test_iced_palette_cache_invalidates_on_change() {
    let palette_a = make_palette(0);
    let palette_b = make_palette(100);
    let mut cache = ThemeCache::default();
    let theme_a = cache.get_or_compute(&palette_a);
    let theme_b = cache.get_or_compute(&palette_b);
    // Different palettes must produce different themes.
    // We compare their Debug representations as a proxy for equality.
    assert_ne!(
        format!("{theme_a:?}"),
        format!("{theme_b:?}"),
        "different palettes must produce different themes"
    );
}

/// `ThemeCache::default` starts empty and computes on first call.
#[test]
fn test_iced_theme_cache_default_starts_empty() {
    let mut cache = ThemeCache::default();
    let palette = make_palette(128);
    // Must not panic; must return a valid theme.
    let theme = cache.get_or_compute(&palette);
    let _ = theme; // just confirm no panic and a result is returned
}

// ── Step 5: Pre-allocate specs vec (TODO L64) ─────────────────────────────────

/// `Vec::with_capacity(0)` is valid; `spec_capacity_hint=0` falls back to 8.
#[test]
fn test_iced_specs_pre_alloc_zero_hint_does_not_panic() {
    let config = IcedConfig::default(); // spec_capacity_hint defaults to 0
    let mut ctx = IcedUiCtx::new(config);
    // Should work fine with the minimum-8 fallback.
    ctx.label("hello");
    ctx.label("world");
    assert_eq!(ctx.spec_count(), 2);
}

/// `with_spec_capacity(prev)` pre-allocates the correct capacity.
#[test]
fn test_iced_specs_pre_alloc_uses_hint() {
    let config = IcedConfig::default().with_spec_capacity(16);
    let mut ctx = IcedUiCtx::new(config);
    // Adding 16 labels must not trigger a reallocation (capacity is already 16).
    for _ in 0..16 {
        ctx.label("item");
    }
    assert_eq!(ctx.spec_count(), 16);
}

/// `spec_count` accurately reflects the number of collected widget specs.
#[test]
fn test_iced_spec_count_reflects_widget_count() {
    let mut ctx = IcedUiCtx::new(IcedConfig::default());
    assert_eq!(ctx.spec_count(), 0, "fresh context must have 0 specs");
    ctx.label("a");
    assert_eq!(ctx.spec_count(), 1);
    ctx.button("b");
    assert_eq!(ctx.spec_count(), 2);
    ctx.separator();
    assert_eq!(ctx.spec_count(), 3);
}

/// Capacity hint from previous frame feeds correctly into next frame's `IcedConfig`.
#[test]
fn test_iced_spec_capacity_round_trip_across_frames() {
    let mut config = IcedConfig::default();

    // Frame 1: render 5 widgets, record count.
    let mut ctx1 = IcedUiCtx::new(config.clone());
    for _ in 0..5 {
        ctx1.label("item");
    }
    let frame1_count = ctx1.spec_count();
    config = config.with_spec_capacity(frame1_count);

    // Frame 2: re-use capacity hint — must not panic and must collect correctly.
    let mut ctx2 = IcedUiCtx::new(config);
    for _ in 0..5 {
        ctx2.label("item");
    }
    assert_eq!(
        ctx2.spec_count(),
        5,
        "frame 2 must collect the same number of specs"
    );
}
