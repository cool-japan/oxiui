/// Design-token integration tests for `oxiui-egui`.
///
/// Covers:
///   - `tokens_to_egui_style` produces a style that differs from egui defaults
///     in at least one measurable field (button_padding).
///   - Typography in `TypographyScale` is reflected in the egui `TextStyle::Body`
///     and `TextStyle::Heading` font sizes.
///   - `StatefulEguiAdapter::with_design_tokens` applies without panic and
///     returns a valid style on the context.
use egui::TextStyle;
use oxiui_egui::tokens_to_egui_style;
use oxiui_theme::{DesignTokens, TypographyScale};

// ── helper: run a headless egui frame ─────────────────────────────────────────

fn headless_run<F: FnMut(&egui::Context)>(mut f: F) {
    let ctx = egui::Context::default();
    // `run_ui` runs the closure inside the egui frame loop.
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        f(ui.ctx());
    });
}

// ── test 1: non-default style ─────────────────────────────────────────────────

/// `tokens_to_egui_style` must produce a style that differs from
/// `egui::Style::default()` in at least one field.
///
/// We assert on `spacing.button_padding` because the default egui value is
/// `vec2(4.0, 1.0)` (from egui 0.34 source), while our mapping sets it to
/// `vec2(spacing_sm, spacing_xs / 2.0)` = `vec2(8.0, 2.0)` with the default
/// token scale — clearly different.
#[test]
fn test_tokens_to_egui_style_non_default() {
    let tokens = DesignTokens::default();
    let typography = TypographyScale::default();

    let style = tokens_to_egui_style(&tokens, &typography);
    let default_style = egui::Style::default();

    // button_padding.x should differ: default is 4.0, token-mapped is 8.0 (Sm).
    assert_ne!(
        style.spacing.button_padding.x, default_style.spacing.button_padding.x,
        "button_padding.x should be overridden from the design token Sm spacing"
    );

    // item_spacing should also differ from default (default is vec2(8.0, 3.0),
    // ours is vec2(Sm=8.0, Xs=4.0) — the y component changes).
    assert_ne!(
        style.spacing.item_spacing.y, default_style.spacing.item_spacing.y,
        "item_spacing.y should be overridden by token Xs spacing"
    );
}

// ── test 2: typography mapped into text styles ────────────────────────────────

/// The body size from `TypographyScale` must be reflected in the egui
/// `TextStyle::Body` font size.
#[test]
fn test_tokens_typography_mapped() {
    let tokens = DesignTokens::default();
    let typography = TypographyScale::default();

    // Verify the default values we are testing against.
    assert_eq!(
        typography.body.size, 14.0,
        "TypographyScale default body is 14.0 px"
    );
    assert_eq!(
        typography.headline.size, 24.0,
        "TypographyScale default headline is 24.0 px"
    );
    assert_eq!(
        typography.caption.size, 12.0,
        "TypographyScale default caption is 12.0 px"
    );

    let style = tokens_to_egui_style(&tokens, &typography);

    // Body text style must carry the body size.
    let body_font = style
        .text_styles
        .get(&TextStyle::Body)
        .expect("TextStyle::Body must be present in the style");
    assert_eq!(
        body_font.size, typography.body.size,
        "egui TextStyle::Body size must match TypographyScale::body.size"
    );

    // Heading text style must carry the headline size.
    let heading_font = style
        .text_styles
        .get(&TextStyle::Heading)
        .expect("TextStyle::Heading must be present in the style");
    assert_eq!(
        heading_font.size, typography.headline.size,
        "egui TextStyle::Heading size must match TypographyScale::headline.size"
    );

    // Small text style must carry the caption size.
    let small_font = style
        .text_styles
        .get(&TextStyle::Small)
        .expect("TextStyle::Small must be present in the style");
    assert_eq!(
        small_font.size, typography.caption.size,
        "egui TextStyle::Small size must match TypographyScale::caption.size"
    );
}

// ── test 3: border radius mapped ─────────────────────────────────────────────

/// Corner-radius tokens are applied to widget states and window/menu radius.
#[test]
fn test_tokens_corner_radius_mapped() {
    let tokens = DesignTokens::default();
    let typography = TypographyScale::default();

    // Default token Sm radius = 2.0 (rounds to 2u8), Md radius = 4.0.
    let expected_sm = 2u8;
    let expected_md = 4u8;

    let style = tokens_to_egui_style(&tokens, &typography);

    assert_eq!(
        style.visuals.widgets.noninteractive.corner_radius,
        egui::CornerRadius::same(expected_sm),
        "noninteractive corner_radius should map to RadiusStep::Sm"
    );
    assert_eq!(
        style.visuals.widgets.inactive.corner_radius,
        egui::CornerRadius::same(expected_sm),
        "inactive corner_radius should map to RadiusStep::Sm"
    );
    assert_eq!(
        style.visuals.widgets.active.corner_radius,
        egui::CornerRadius::same(expected_md),
        "active corner_radius should map to RadiusStep::Md"
    );
    assert_eq!(
        style.visuals.menu_corner_radius,
        egui::CornerRadius::same(expected_md),
        "menu_corner_radius should map to RadiusStep::Md"
    );
    assert_eq!(
        style.visuals.window_corner_radius,
        egui::CornerRadius::same(expected_md),
        "window_corner_radius should map to RadiusStep::Md"
    );
}

// ── test 4: StatefulEguiAdapter with design tokens ────────────────────────────

/// Creating a `StatefulEguiAdapter` with design tokens and applying it to a
/// headless egui context must not panic, and the context must have an updated
/// style (body font size differs from egui default 13.0 px → token-mapped 14.0 px).
#[test]
fn test_egui_integration_design_tokens_flow() {
    use oxiui_core::{Color, Palette};
    use oxiui_egui::StatefulEguiAdapter;

    let palette = Palette {
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        text: Color(192, 202, 245, 255),
        muted: Color(86, 95, 137, 255),
    };

    let tokens = DesignTokens::default();
    let typography = TypographyScale::default();

    let mut adapter = StatefulEguiAdapter::new()
        .with_palette(palette)
        .with_design_tokens(tokens, typography);

    headless_run(|ctx| {
        // Must not panic.
        adapter.apply(ctx);

        // The body font size should now be 14.0 (from TypographyScale default),
        // not the egui default of 13.0.
        let body_size = ctx
            .global_style()
            .text_styles
            .get(&TextStyle::Body)
            .map(|f| f.size);
        assert_eq!(
            body_size,
            Some(14.0),
            "Body font size must be 14.0 after design token application"
        );
    });
}

// ── test 5: StatefulEguiAdapter tokens applied only once ─────────────────────

/// `tokens_applied` guard: applying the adapter a second time must not re-apply
/// the style (the adapter exits early after the first application).
///
/// We verify this indirectly by confirming the adapter does not panic when
/// `apply` is called across two frames, and that `visuals_recompute_count` is 1.
#[test]
fn test_stateful_adapter_tokens_applied_once() {
    use oxiui_core::{Color, Palette};
    use oxiui_egui::StatefulEguiAdapter;

    let palette = Palette {
        background: Color(26, 27, 38, 255),
        surface: Color(36, 40, 59, 255),
        primary: Color(122, 162, 247, 255),
        on_primary: Color(26, 27, 38, 255),
        text: Color(192, 202, 245, 255),
        muted: Color(86, 95, 137, 255),
    };

    let mut adapter = StatefulEguiAdapter::new()
        .with_palette(palette)
        .with_design_tokens(DesignTokens::default(), TypographyScale::default());

    let ctx = egui::Context::default();

    // Frame 1.
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| adapter.apply(ui.ctx()));
    assert_eq!(
        adapter.visuals_recompute_count, 1,
        "first frame should recompute once"
    );

    // Frame 2 — tokens already applied; no further recompute.
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| adapter.apply(ui.ctx()));
    // Count stays at 1 (tokens path exits early, palette-only path is skipped).
    assert_eq!(
        adapter.visuals_recompute_count, 1,
        "second frame must not recompute"
    );
}
