#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! COOLJAPAN dark/light themes — Tokyo Night palette.
//!
//! # Quick start
//! ```
//! let theme = oxiui_theme::cooljapan_default();
//! assert_eq!(theme.palette().background.0, 26); // Tokyo Night dark #1A1B26
//! ```

use oxiui_core::{Color, FontSpec, Palette, Theme};

pub mod high_contrast;
pub use high_contrast::{cooljapan_high_contrast, cooljapan_high_contrast_light};

pub mod anim_tokens;
pub mod breakpoint;
pub mod builder;
pub mod color;
pub mod compile;
pub mod gallery;
pub mod icons;
pub mod inheritance;
pub mod lazy_palette;
pub mod manager;
pub mod overlay;
pub mod palette_ext;
pub mod serial;
pub mod spec;
pub mod style_cache;
pub mod stylesheet;
pub mod tokens;
pub mod typography;

pub use anim_tokens::{
    fade_in, scale_up, slide_in, AnimationKeyframe, AnimationSpec, EasingKind, FillMode,
    IterationCount, TransitionSpec,
};
pub use breakpoint::Breakpoint;
pub use builder::{ContrastWarning, PaletteBuilder, ValidationResult, WcagLevel};
pub use compile::CompiledStyleSheet;
pub use gallery::{
    make_catppuccin_latte, make_catppuccin_mocha, make_dracula, make_material_dark,
    make_material_light, make_nord_dark, make_nord_light, make_solarized_dark,
    make_solarized_light,
};
pub use icons::{BuiltinIcons, IconName, IconSet, IconVariant};
pub use inheritance::resolve as resolve_inheritance;
pub use lazy_palette::LazyPaletteVariants;
pub use manager::{ThemeListener, ThemeManager};
pub use overlay::{overlay, PartialTheme};
pub use palette_ext::ExtendedPalette;
pub use serial::{deserialize_theme, serialize_theme, ThemeSnapshot};
pub use spec::{
    elevation_shadow, elevation_shadows, elevation_to_shadow, BorderSpec, BorderSpecs, BorderStyle,
    ShadowSpec,
};
pub use style_cache::StyleCache;
pub use stylesheet::{
    ComputedStyle, CssValue, ParseDiagnostic, ParseResult, Rule, Selector, SelectorPart,
    Specificity, StyleSheet,
};
pub use tokens::{DesignTokens, RadiusStep, SpacingStep};
pub use typography::{TextStyleToken, TypographyScale};

/// COOLJAPAN theme implementing [`Theme`]: a colour [`Palette`] plus a [`FontSpec`].
///
/// Construct with [`cooljapan_default`], [`dark`], or [`light`], or build a
/// custom one directly via [`CooljapanTheme::new`].
#[derive(Clone, Debug)]
pub struct CooljapanTheme {
    palette: Palette,
    font: FontSpec,
}

impl CooljapanTheme {
    /// Construct a theme from an explicit palette and font.
    pub fn new(palette: Palette, font: FontSpec) -> Self {
        Self { palette, font }
    }
}

impl Theme for CooljapanTheme {
    fn palette(&self) -> &Palette {
        &self.palette
    }
    fn font(&self) -> &FontSpec {
        &self.font
    }
}

/// Extension trait adding design tokens, typography, and the extended palette
/// to any [`Theme`].
///
/// Implemented for every `Theme` via a blanket impl, returning the COOLJAPAN
/// defaults. Concrete themes may override these methods for custom token sets.
pub trait ThemeExt: Theme {
    /// The theme's design-token scale (spacing / radius / elevation / opacity).
    ///
    /// Returns by value for flexibility; use [`ThemeExt::design_tokens`] for a
    /// reference-returning variant backed by a process-lifetime static.
    fn tokens(&self) -> DesignTokens {
        DesignTokens::default()
    }

    /// Returns a reference to the [`DesignTokens`] for this theme.
    ///
    /// The default implementation stores the default tokens in a
    /// `OnceLock`-backed static so that the reference lifetime is `'static`.
    /// Override this method in concrete theme structs that own a `DesignTokens`
    /// field to return a reference to that field instead.
    fn design_tokens(&self) -> &DesignTokens {
        static DEFAULT: std::sync::OnceLock<DesignTokens> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(DesignTokens::default)
    }

    /// The theme's typographic scale (returned by value).
    ///
    /// Use [`ThemeExt::typography_ref`] for a reference-returning variant.
    fn typography(&self) -> TypographyScale {
        TypographyScale::default()
    }

    /// Returns a reference to the [`TypographyScale`] for this theme.
    ///
    /// Backed by a `OnceLock`-bound static in the blanket impl. Override in
    /// concrete structs that carry a `TypographyScale` field.
    fn typography_ref(&self) -> &TypographyScale {
        static DEFAULT: std::sync::OnceLock<TypographyScale> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(TypographyScale::default)
    }

    /// Returns `true` if this theme is designed for high-contrast display.
    ///
    /// The default returns `false`. Override in high-contrast theme variants.
    fn is_high_contrast(&self) -> bool {
        false
    }

    /// Returns the effective [`Palette`], with contrast boosted if the OS or
    /// user has requested high-contrast mode via the `OXIUI_HIGH_CONTRAST`
    /// environment variable.
    ///
    /// When the env var is set to `"1"` or `"true"` (case-insensitive) and the
    /// theme is not already a high-contrast theme, a simple contrast-boost is
    /// applied: the background and surface colours are blended slightly toward
    /// black to deepen dark tones.
    fn effective_palette(&self) -> Palette {
        let prefs_high_contrast = os_prefers_high_contrast();
        if prefs_high_contrast && !self.is_high_contrast() {
            let mut p = self.palette().clone();
            p.background = blend_to_black(p.background, 0.1);
            p.surface = blend_to_black(p.surface, 0.05);
            p
        } else {
            self.palette().clone()
        }
    }

    /// The extended semantic palette, derived from [`Theme::palette`].
    ///
    /// `dark` is inferred from the background luminance: backgrounds darker than
    /// mid-grey are treated as dark themes for status-colour selection.
    fn extended_palette(&self) -> ExtendedPalette {
        let p = self.palette();
        let bg = p.background;
        let luma = color::wcag_luminance(bg.0, bg.1, bg.2);
        ExtendedPalette::derive(p.clone(), luma < 0.5)
    }
}

impl<T: Theme + ?Sized> ThemeExt for T {}

// ── OS accessibility helpers ────────────────────────────────────────────────

/// Returns `true` if the OS (or user preference env var) requests high-contrast
/// mode.
///
/// Reads the `OXIUI_HIGH_CONTRAST` environment variable. Accepted truthy values
/// are `"1"` and `"true"` (case-insensitive). All other values — including
/// absent variable — return `false`.
pub fn os_prefers_high_contrast() -> bool {
    std::env::var("OXIUI_HIGH_CONTRAST")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Returns `true` if the OS (or user preference env var) requests reduced
/// motion (fewer animations / transitions).
///
/// Reads the `OXIUI_REDUCED_MOTION` environment variable. Accepted truthy
/// values are `"1"` and `"true"` (case-insensitive).
pub fn os_prefers_reduced_motion() -> bool {
    std::env::var("OXIUI_REDUCED_MOTION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

// ── Private palette-blend helpers ───────────────────────────────────────────

/// Blends `color` toward pure black by `factor` (0.0 = no change, 1.0 = black).
///
/// Each channel is reduced proportionally: `channel * (1.0 - factor)`.
/// The alpha channel is preserved unchanged.
fn blend_to_black(color: oxiui_core::Color, factor: f32) -> oxiui_core::Color {
    let factor = factor.clamp(0.0, 1.0);
    let scale = 1.0 - factor;
    oxiui_core::Color(
        (color.0 as f32 * scale).round() as u8,
        (color.1 as f32 * scale).round() as u8,
        (color.2 as f32 * scale).round() as u8,
        color.3,
    )
}

fn make_dark() -> Box<dyn Theme> {
    Box::new(CooljapanTheme::new(
        Palette {
            background: Color(26, 27, 38, 255), // #1A1B26
            surface: Color(36, 40, 59, 255),    // #24283B
            primary: Color(122, 162, 247, 255), // #7AA2F7
            on_primary: Color(26, 27, 38, 255), // #1A1B26
            text: Color(192, 202, 245, 255),    // #C0CAF5
            muted: Color(86, 95, 137, 255),     // #565F89
        },
        FontSpec::new("Inter", 14.0, 400),
    ))
}

fn make_light() -> Box<dyn Theme> {
    Box::new(CooljapanTheme::new(
        Palette {
            background: Color(216, 218, 228, 255), // #D8DAE4
            surface: Color(255, 255, 255, 255),    // #FFFFFF
            primary: Color(68, 100, 200, 255),     // #4464C8
            on_primary: Color(255, 255, 255, 255),
            text: Color(30, 35, 60, 255), // #1E233C
            muted: Color(120, 130, 155, 255),
        },
        FontSpec::new("Inter", 14.0, 400),
    ))
}

/// Returns the COOLJAPAN default theme (dark / Tokyo Night).
pub fn cooljapan_default() -> Box<dyn Theme> {
    make_dark()
}

/// Returns the COOLJAPAN dark theme (Tokyo Night).
pub fn dark() -> Box<dyn Theme> {
    make_dark()
}

/// Returns the COOLJAPAN light theme.
pub fn light() -> Box<dyn Theme> {
    make_light()
}
