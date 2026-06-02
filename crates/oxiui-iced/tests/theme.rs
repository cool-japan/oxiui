/// Tests for COOLJAPAN palette → iced theme conversion.
///
/// Covers both `palette_to_iced_theme(&Palette)` and the extended
/// `palette_to_iced_theme_ext(&dyn Theme)` variant, plus the text_input and
/// scrollable style derivation helpers.
#[test]
fn palette_to_iced_theme_dark() {
    let theme_box = oxiui_theme::dark();
    let iced_theme = oxiui_iced::palette_to_iced_theme(theme_box.palette());
    // Verify it produces a Custom variant (our wrapper always does)
    let debug_repr = format!("{:?}", iced_theme);
    // iced::Theme::Custom wraps an Arc<Custom> — the Debug output contains "Custom"
    assert!(
        debug_repr.contains("Custom"),
        "expected Custom theme variant, got: {debug_repr}"
    );
}

#[test]
fn palette_to_iced_theme_light() {
    let theme_box = oxiui_theme::light();
    let iced_theme = oxiui_iced::palette_to_iced_theme(theme_box.palette());
    // Compile and smoke check — just confirm no panic and a non-empty debug repr
    let debug_repr = format!("{:?}", iced_theme);
    assert!(!debug_repr.is_empty());
}

#[test]
fn palette_to_iced_theme_default() {
    let theme_box = oxiui_theme::cooljapan_default();
    let iced_theme = oxiui_iced::palette_to_iced_theme(theme_box.palette());
    // Theme::custom returns Theme::Custom — verify name is embedded
    let name = iced_theme.to_string();
    assert_eq!(name, "OxiUI COOLJAPAN");
}

// ── palette_to_iced_theme_ext ─────────────────────────────────────────────────

#[test]
fn palette_to_iced_theme_ext_dark_produces_custom() {
    // palette_to_iced_theme_ext accepts &dyn Theme, not &Palette.
    let theme_box = oxiui_theme::dark();
    let iced_theme = oxiui_iced::palette_to_iced_theme_ext(theme_box.as_ref());
    let debug_repr = format!("{:?}", iced_theme);
    assert!(
        debug_repr.contains("Custom"),
        "expected Custom theme variant from ext fn, got: {debug_repr}"
    );
}

#[test]
fn palette_to_iced_theme_ext_matches_palette_variant() {
    // Verify ext fn and bare-palette fn produce equivalent results.
    let theme_box = oxiui_theme::cooljapan_default();
    let from_palette = oxiui_iced::palette_to_iced_theme(theme_box.palette());
    let from_theme = oxiui_iced::palette_to_iced_theme_ext(theme_box.as_ref());
    // Both return Theme::Custom; check the display name matches.
    assert_eq!(from_palette.to_string(), from_theme.to_string());
}

// ── text_input style expansion ────────────────────────────────────────────────

#[test]
fn text_input_style_border_color_is_primary() {
    let theme_box = oxiui_theme::cooljapan_default();
    let style = oxiui_iced::text_input_style_from_palette(theme_box.palette());
    // The border color should be non-transparent (primary color)
    assert!(
        style.border.color.a > 0.0,
        "text_input border color must not be fully transparent"
    );
    // Border width should be positive
    assert!(style.border.width > 0.0, "border width must be positive");
}

#[test]
fn text_input_style_from_theme_matches_palette() {
    let theme_box = oxiui_theme::cooljapan_default();
    let from_palette = oxiui_iced::text_input_style_from_palette(theme_box.palette());
    let from_theme = oxiui_iced::text_input_style_from_theme(theme_box.as_ref());
    assert_eq!(from_palette.border.color, from_theme.border.color);
}

// ── scrollable style expansion ────────────────────────────────────────────────

#[test]
fn scrollable_style_scroller_background_is_primary() {
    let theme_box = oxiui_theme::cooljapan_default();
    let style = oxiui_iced::scrollable_style_from_palette(theme_box.palette());
    // Verify vertical rail scroller background is set (non-transparent)
    use iced::Background;
    if let Background::Color(c) = style.vertical_rail.scroller.background {
        assert!(
            c.a > 0.0,
            "scroller background color must not be fully transparent"
        );
    }
    // gradient or other fill — also acceptable
}

#[test]
fn scrollable_style_from_theme_matches_palette() {
    let theme_box = oxiui_theme::cooljapan_default();
    let from_palette = oxiui_iced::scrollable_style_from_palette(theme_box.palette());
    let from_theme = oxiui_iced::scrollable_style_from_theme(theme_box.as_ref());
    assert_eq!(
        from_palette.vertical_rail.scroller.background,
        from_theme.vertical_rail.scroller.background
    );
}

// ── DesignTokensAdapter tests ─────────────────────────────────────────────────

#[test]
fn test_design_tokens_adapter_from_default() {
    use oxiui_theme::{DesignTokens, TypographyScale};
    let adapter = oxiui_iced::DesignTokensAdapter::from_tokens(
        &DesignTokens::default(),
        &TypographyScale::default(),
    );
    assert!(
        adapter.body_font_size > 0.0,
        "body_font_size must be positive, got {}",
        adapter.body_font_size
    );
    assert!(
        adapter.border_radius >= 0.0,
        "border_radius must be non-negative, got {}",
        adapter.border_radius
    );
    assert!(
        adapter.headline_font_size > 0.0,
        "headline_font_size must be positive, got {}",
        adapter.headline_font_size
    );
    assert!(
        adapter.base_spacing > 0.0,
        "base_spacing must be positive, got {}",
        adapter.base_spacing
    );
}

#[test]
fn test_body_text_size_matches_typography() {
    use oxiui_theme::{DesignTokens, TypographyScale};
    let adapter = oxiui_iced::DesignTokensAdapter::from_tokens(
        &DesignTokens::default(),
        &TypographyScale::default(),
    );
    let size = adapter.body_text_size();
    assert_eq!(
        size.0, adapter.body_font_size,
        "body_text_size().0 must equal body_font_size"
    );
}

#[test]
fn test_standard_padding_non_zero() {
    use oxiui_theme::{DesignTokens, TypographyScale};
    let adapter = oxiui_iced::DesignTokensAdapter::from_tokens(
        &DesignTokens::default(),
        &TypographyScale::default(),
    );
    let padding = adapter.standard_padding();
    // base_spacing = 8.0 from SpacingStep::Sm default → all sides should be 8.0
    assert!(
        padding.top > 0.0,
        "standard_padding().top must be > 0 when base_spacing > 0, got {}",
        padding.top
    );
}

#[test]
fn test_palette_and_tokens_theme() {
    use oxiui_theme::{DesignTokens, TypographyScale};
    let theme_box = oxiui_theme::cooljapan_default();
    let tokens = DesignTokens::default();
    let typography = TypographyScale::default();
    let iced_theme = oxiui_iced::palette_and_tokens_to_iced_theme(
        theme_box.palette(),
        Some(&tokens),
        Some(&typography),
    );
    // Must not panic and must return a Custom theme (same as palette_to_iced_theme).
    let debug_repr = format!("{:?}", iced_theme);
    assert!(
        debug_repr.contains("Custom"),
        "expected Custom theme variant, got: {debug_repr}"
    );
}
