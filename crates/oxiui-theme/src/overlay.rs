//! Theme composition via [`PartialTheme`] overlays.
//!
//! [`overlay`] merges a [`PartialTheme`] (all fields optional) into a base
//! [`CooljapanTheme`], returning a new theme where every `Some` field in
//! `overrides` supersedes the corresponding field in `base`.
//!
//! ```rust
//! use oxiui_core::{Color, Theme};
//! use oxiui_theme::{CooljapanTheme, dark};
//! use oxiui_theme::overlay::{overlay, PartialTheme};
//!
//! let base = oxiui_theme::dark();
//! let red_bg = PartialTheme { background: Some(Color(200, 0, 0, 255)), ..PartialTheme::default() };
//! let new_theme = overlay(base.as_ref(), &red_bg);
//! assert_eq!(new_theme.palette().background, Color(200, 0, 0, 255));
//! ```

use crate::CooljapanTheme;
use oxiui_core::{Color, FontSpec, Palette, Theme};

/// A partial palette/font override — every field is optional.
///
/// Fields set to `Some(…)` replace the corresponding field in the base theme.
/// `None` fields keep the base value unchanged.
#[derive(Clone, Debug, Default)]
pub struct PartialTheme {
    /// Override for the background colour.
    pub background: Option<Color>,
    /// Override for the surface colour.
    pub surface: Option<Color>,
    /// Override for the primary brand colour.
    pub primary: Option<Color>,
    /// Override for the on-primary colour.
    pub on_primary: Option<Color>,
    /// Override for the primary text colour.
    pub text_primary: Option<Color>,
    /// Override for the muted / secondary text colour.
    pub text_secondary: Option<Color>,
    /// Override for the font family name.
    pub font_family: Option<String>,
    /// Override for the font size in logical pixels.
    pub font_size: Option<f32>,
    /// Override for the font weight.
    pub font_weight: Option<u16>,
}

/// Merge `overrides` into `base`, returning a new [`CooljapanTheme`].
///
/// Any `Some` field in `overrides` replaces the corresponding field in `base`.
/// All `None` fields keep the value from `base`.
pub fn overlay(base: &dyn Theme, overrides: &PartialTheme) -> CooljapanTheme {
    let bp = base.palette();
    let bf = base.font();

    let palette = Palette {
        background: overrides.background.unwrap_or(bp.background),
        surface: overrides.surface.unwrap_or(bp.surface),
        primary: overrides.primary.unwrap_or(bp.primary),
        on_primary: overrides.on_primary.unwrap_or(bp.on_primary),
        text: overrides.text_primary.unwrap_or(bp.text),
        muted: overrides.text_secondary.unwrap_or(bp.muted),
    };
    let font = FontSpec::new(
        overrides.font_family.as_deref().unwrap_or(&bf.family),
        overrides.font_size.unwrap_or(bf.size),
        overrides.font_weight.unwrap_or(bf.weight),
    );
    CooljapanTheme::new(palette, font)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::Theme;

    fn base_theme() -> Box<dyn Theme> {
        crate::dark()
    }

    #[test]
    fn overlay_override_precedence() {
        let base = base_theme();
        let red = Color(200, 0, 0, 255);
        let overrides = PartialTheme {
            background: Some(red),
            ..PartialTheme::default()
        };
        let result = overlay(base.as_ref(), &overrides);
        assert_eq!(
            result.palette().background,
            red,
            "overlay must apply the override background"
        );
    }

    #[test]
    fn overlay_none_fields_keep_base() {
        let base = base_theme();
        let original_bg = base.palette().background;
        let overrides = PartialTheme::default(); // all None
        let result = overlay(base.as_ref(), &overrides);
        assert_eq!(
            result.palette().background,
            original_bg,
            "empty overlay must preserve base"
        );
        assert_eq!(result.palette().surface, base.palette().surface);
        assert_eq!(result.palette().text, base.palette().text);
    }

    #[test]
    fn overlay_multiple_fields() {
        let base = base_theme();
        let new_text = Color(0, 255, 0, 255);
        let new_primary = Color(0, 0, 255, 255);
        let overrides = PartialTheme {
            text_primary: Some(new_text),
            primary: Some(new_primary),
            ..PartialTheme::default()
        };
        let result = overlay(base.as_ref(), &overrides);
        assert_eq!(result.palette().text, new_text);
        assert_eq!(result.palette().primary, new_primary);
        // Unset fields must still come from base.
        assert_eq!(result.palette().background, base.palette().background);
    }
}
