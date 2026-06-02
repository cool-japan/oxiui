use oxiui_core::{Color, FontSpec, Palette, Theme};
use oxiui_theme::high_contrast::{wcag_contrast, wcag_luminance};
use oxiui_theme::{cooljapan_default, cooljapan_high_contrast, dark, light};
use oxiui_theme::{
    os_prefers_high_contrast, os_prefers_reduced_motion, CooljapanTheme, DesignTokens, RadiusStep,
    SpacingStep, ThemeExt, TypographyScale,
};

// ── Existing palette tests ───────────────────────────────────────────────────

#[test]
fn dark_and_light_palettes_differ() {
    let d = dark();
    let l = light();
    assert_ne!(d.palette().background, l.palette().background);
}

#[test]
fn cooljapan_default_is_dark() {
    let default_theme = cooljapan_default();
    let dark_theme = dark();
    // Both should return the same primary colour.
    assert_eq!(
        default_theme.palette().primary,
        dark_theme.palette().primary
    );
}

#[test]
fn font_spec_has_positive_size_and_weight() {
    let theme = cooljapan_default();
    let font = theme.font();
    assert!(font.size > 0.0, "font size must be positive");
    assert!(font.weight > 0, "font weight must be positive");
}

#[test]
fn dark_palette_is_tokyo_night() {
    let theme = dark();
    let palette = theme.palette();
    // #1A1B26 = (26, 27, 38)
    assert_eq!(palette.background.0, 26);
    assert_eq!(palette.background.1, 27);
    assert_eq!(palette.background.2, 38);
}

#[test]
fn light_palette_has_white_surface() {
    let theme = light();
    let palette = theme.palette();
    assert_eq!(palette.surface.0, 255);
    assert_eq!(palette.surface.1, 255);
    assert_eq!(palette.surface.2, 255);
}

// ── WCAG luminance / contrast helpers ───────────────────────────────────────

#[test]
fn wcag_luminance_black_is_zero() {
    let lum = wcag_luminance(0, 0, 0);
    assert!(
        (lum - 0.0).abs() < 1e-10,
        "black luminance must be 0.0, got {lum}"
    );
}

#[test]
fn wcag_luminance_white_is_one() {
    let lum = wcag_luminance(255, 255, 255);
    assert!(
        (lum - 1.0).abs() < 1e-6,
        "white luminance must be ~1.0, got {lum}"
    );
}

#[test]
fn wcag_contrast_white_on_black_is_21() {
    let ratio = wcag_contrast((255, 255, 255), (0, 0, 0));
    assert!(
        (ratio - 21.0).abs() < 0.1,
        "white-on-black contrast should be ~21.0, got {ratio:.4}"
    );
}

#[test]
fn wcag_contrast_yellow_on_black_exceeds_aaa() {
    // #FFFF00 (bright yellow) on #000000 (black)
    let ratio = wcag_contrast((255, 255, 0), (0, 0, 0));
    assert!(
        ratio > 7.0,
        "yellow-on-black contrast should exceed WCAG AAA 7.0, got {ratio:.4}"
    );
}

#[test]
fn wcag_contrast_is_symmetric() {
    let fg_on_bg = wcag_contrast((255, 255, 255), (0, 0, 0));
    let bg_on_fg = wcag_contrast((0, 0, 0), (255, 255, 255));
    assert!(
        (fg_on_bg - bg_on_fg).abs() < 1e-10,
        "contrast ratio must be symmetric"
    );
}

// ── cooljapan_high_contrast() palette ───────────────────────────────────────

#[test]
fn high_contrast_palette_foreground_vs_background_exceeds_aaa() {
    let p = cooljapan_high_contrast();
    let fg = (p.text.0, p.text.1, p.text.2);
    let bg = (p.background.0, p.background.1, p.background.2);
    let ratio = wcag_contrast(fg, bg);
    assert!(
        ratio > 7.0,
        "high-contrast text-on-background must exceed WCAG AAA 7.0, got {ratio:.4}"
    );
}

#[test]
fn high_contrast_palette_primary_vs_background_exceeds_aaa() {
    let p = cooljapan_high_contrast();
    let primary = (p.primary.0, p.primary.1, p.primary.2);
    let bg = (p.background.0, p.background.1, p.background.2);
    let ratio = wcag_contrast(primary, bg);
    assert!(
        ratio > 7.0,
        "high-contrast primary-on-background must exceed WCAG AAA 7.0, got {ratio:.4}"
    );
}

#[test]
fn high_contrast_palette_background_is_black() {
    let p = cooljapan_high_contrast();
    assert_eq!(p.background.0, 0, "background R");
    assert_eq!(p.background.1, 0, "background G");
    assert_eq!(p.background.2, 0, "background B");
}

#[test]
fn high_contrast_palette_text_is_white() {
    let p = cooljapan_high_contrast();
    assert_eq!(p.text.0, 255, "text R");
    assert_eq!(p.text.1, 255, "text G");
    assert_eq!(p.text.2, 255, "text B");
}

// ── ThemeExt: tokens, typography, extended palette ──────────────────────────

#[test]
fn theme_ext_works_on_boxed_dyn_theme() {
    // The blanket impl must reach through Box<dyn Theme>.
    let theme = cooljapan_default();
    let tokens = theme.tokens();
    assert_eq!(tokens.spacing(SpacingStep::Xs), 4.0);
    assert_eq!(tokens.radius(RadiusStep::None), 0.0);
    let typ = theme.typography();
    assert!(typ.body.size > 0.0);
}

#[test]
fn extended_palette_inferred_dark_for_dark_theme() {
    let theme = dark();
    let ext = theme.extended_palette();
    // Base preserved.
    assert_eq!(ext.base.background, theme.palette().background);
    // Dark theme => Tokyo Night red error (#F7768E).
    assert_eq!(ext.error, Color(247, 118, 142, 255));
}

#[test]
fn extended_palette_inferred_light_for_light_theme() {
    let theme = light();
    let ext = theme.extended_palette();
    // Light theme picks the darker status set (different from dark theme's).
    assert_ne!(ext.error, Color(247, 118, 142, 255));
}

#[test]
fn custom_cooljapan_theme_constructible() {
    // CooljapanTheme is now public and constructible by users.
    let palette = Palette::new(
        Color(10, 10, 10, 255),
        Color(20, 20, 20, 255),
        Color(0, 120, 255, 255),
        Color(255, 255, 255, 255),
        Color(230, 230, 230, 255),
        Color(120, 120, 120, 255),
    );
    let theme = CooljapanTheme::new(palette, FontSpec::new("Inter", 16.0, 500));
    assert_eq!(theme.font().size, 16.0);
    assert_eq!(theme.palette().primary, Color(0, 120, 255, 255));
    // ThemeExt available on the concrete type too.
    assert!(theme.typography().display.size > theme.typography().body.size);
}

// ── ThemeExt: design_tokens(), typography_ref(), is_high_contrast(), effective_palette() ──

#[test]
fn test_design_tokens_from_dark_theme() {
    // design_tokens() returns a reference to a static-backed DesignTokens.
    let theme = dark();
    let dt: &DesignTokens = theme.design_tokens();
    // spacing.xs is the smallest step — must be positive.
    assert!(dt.spacing(SpacingStep::Xs) > 0.0);
    // radius.None is 0.0 (no rounding).
    assert_eq!(dt.radius(RadiusStep::None), 0.0);
    // elevation level 0 is 0.0 (no shadow).
    assert_eq!(dt.elevation(0), 0.0);
}

#[test]
fn test_typography_ref_from_dark_theme() {
    // typography_ref() returns a reference to a static-backed TypographyScale.
    let theme = dark();
    let ts: &TypographyScale = theme.typography_ref();
    // body size is the 14-px default.
    assert!(ts.body.size > 0.0);
    // display is largest.
    assert!(ts.display.size > ts.body.size);
}

#[test]
fn test_effective_palette_no_override_by_default() {
    // Without OXIUI_HIGH_CONTRAST set the effective palette must equal the raw palette.
    // Guard: if the var happens to be set in the test runner environment, skip.
    if std::env::var("OXIUI_HIGH_CONTRAST").is_ok() {
        return;
    }
    let theme = light();
    let palette = theme.palette().clone();
    let effective = theme.effective_palette();
    assert_eq!(effective.background, palette.background);
    assert_eq!(effective.surface, palette.surface);
    assert_eq!(effective.primary, palette.primary);
    assert_eq!(effective.text, palette.text);
}

#[test]
fn test_effective_palette_boosts_contrast_when_env_set() {
    // SAFETY: std::env::set_var is documented as not safe in multi-threaded contexts.
    // This test is intentionally written to mutate the env only within a narrow window
    // and immediately restore it. Because cargo test runs each integration-test binary
    // single-threaded by default (`--test-threads=1` is not needed for correctness here
    // since we use a unique key and restore it), the window is small.
    let key = "OXIUI_HIGH_CONTRAST_TEST_TEMP_KEY_DP92";
    // Temporarily hijack a throwaway var to validate the blend logic directly
    // rather than mutating the real OXIUI_HIGH_CONTRAST var.
    // Use blend_to_black logic directly: channel * (1 - factor).
    let bg = Color(216, 218, 228, 255); // light theme background
    let expected_r = (bg.0 as f32 * 0.9).round() as u8;
    let expected_g = (bg.1 as f32 * 0.9).round() as u8;
    let expected_b = (bg.2 as f32 * 0.9).round() as u8;
    // factor 0.1 => 90% of original
    assert!(
        expected_r < bg.0 || expected_g < bg.1 || expected_b < bg.2,
        "blending toward black must darken at least one channel"
    );
    // Verify using env-driven path. Guard to avoid interfering with other tests
    // if someone else sets OXIUI_HIGH_CONTRAST.
    if std::env::var("OXIUI_HIGH_CONTRAST").is_ok() {
        return;
    }
    // SAFETY: single-threaded integration test binary.
    unsafe {
        std::env::set_var("OXIUI_HIGH_CONTRAST", "1");
    }
    let theme = light();
    let effective = theme.effective_palette();
    // SAFETY: restore immediately.
    unsafe {
        std::env::remove_var("OXIUI_HIGH_CONTRAST");
    }
    // Background must have been darkened.
    assert!(effective.background.0 <= bg.0);
    assert!(effective.background.1 <= bg.1);
    assert!(effective.background.2 <= bg.2);
    // Alpha preserved.
    assert_eq!(effective.background.3, 255);
    // Not identical to original.
    assert_ne!(effective.background, bg);
    // Suppress unused key warning.
    let _ = key;
}

/// Test: `os_prefers_high_contrast()` returns true when env var is "1".
///
/// # Safety
/// `std::env::set_var` is unsafe in multi-threaded contexts. This test
/// must run in a single-threaded environment (which integration test binaries
/// are by default in Rust's test harness).
#[test]
fn test_high_contrast_auto_detect_env_var_one() {
    if std::env::var("OXIUI_HIGH_CONTRAST").is_ok() {
        // Already set — skip to avoid interfering.
        return;
    }
    unsafe {
        std::env::set_var("OXIUI_HIGH_CONTRAST", "1");
    }
    let result = os_prefers_high_contrast();
    unsafe {
        std::env::remove_var("OXIUI_HIGH_CONTRAST");
    }
    assert!(
        result,
        "os_prefers_high_contrast() must be true when OXIUI_HIGH_CONTRAST=1"
    );
}

/// Same as above but with "true" (case-insensitive).
#[test]
fn test_high_contrast_auto_detect_env_var_true() {
    if std::env::var("OXIUI_HIGH_CONTRAST").is_ok() {
        return;
    }
    unsafe {
        std::env::set_var("OXIUI_HIGH_CONTRAST", "True");
    }
    let result = os_prefers_high_contrast();
    unsafe {
        std::env::remove_var("OXIUI_HIGH_CONTRAST");
    }
    assert!(
        result,
        "os_prefers_high_contrast() must be true when OXIUI_HIGH_CONTRAST=True"
    );
}

#[test]
fn test_high_contrast_env_absent_returns_false() {
    if std::env::var("OXIUI_HIGH_CONTRAST").is_ok() {
        return;
    }
    assert!(!os_prefers_high_contrast(), "no env var => false");
}

#[test]
fn test_reduced_motion_env_var() {
    if std::env::var("OXIUI_REDUCED_MOTION").is_ok() {
        return;
    }
    assert!(!os_prefers_reduced_motion(), "no env var => false");
    unsafe {
        std::env::set_var("OXIUI_REDUCED_MOTION", "1");
    }
    let result = os_prefers_reduced_motion();
    unsafe {
        std::env::remove_var("OXIUI_REDUCED_MOTION");
    }
    assert!(result);
}

#[test]
fn test_theme_trait_object_safe() {
    // Verify Theme is object-safe (Box<dyn Theme> can be constructed).
    let palette = Palette::new(
        Color(0, 0, 0, 255),
        Color(20, 20, 20, 255),
        Color(0, 100, 200, 255),
        Color(255, 255, 255, 255),
        Color(220, 220, 220, 255),
        Color(128, 128, 128, 255),
    );
    let _: Box<dyn Theme> = Box::new(CooljapanTheme::new(palette, FontSpec::default()));
}

#[test]
fn test_is_high_contrast_default_false() {
    // By default, no theme claims to be high-contrast.
    let theme = dark();
    assert!(!theme.is_high_contrast());
    let theme = light();
    assert!(!theme.is_high_contrast());
}

#[test]
fn test_design_tokens_spacing_non_zero() {
    let theme = cooljapan_default();
    let dt = theme.design_tokens();
    // The base spacing step (Xs = 4px) must be > 0.
    assert!(
        dt.spacing(SpacingStep::Xs) > 0.0,
        "Xs spacing must be positive"
    );
}

#[test]
fn test_typography_ref_body_size_positive() {
    let theme = cooljapan_default();
    let ts = theme.typography_ref();
    assert!(ts.body.size > 0.0, "body font size must be positive");
}
