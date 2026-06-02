//! Extended semantic palette built on top of the core [`Palette`].
//!
//! The core [`oxiui_core::Palette`] is intentionally minimal (and used as a
//! struct literal across the workspace, so its fields are frozen). This module
//! layers the additional semantic roles a real design system needs —
//! status colours (`error`/`warning`/`success`/`info`), surface/outline/shadow,
//! and on-surface text — without modifying the core type.

use oxiui_core::{Color, Palette};

/// A palette extended with status, surface, and on-* semantic colours.
#[derive(Clone, Debug)]
pub struct ExtendedPalette {
    /// The underlying core palette (background/surface/primary/text/…).
    pub base: Palette,
    /// Error / destructive state colour.
    pub error: Color,
    /// Warning / caution state colour.
    pub warning: Color,
    /// Success / confirmation state colour.
    pub success: Color,
    /// Informational state colour.
    pub info: Color,
    /// A secondary surface tone (e.g. nested cards).
    pub surface_variant: Color,
    /// Outline / divider / border colour.
    pub outline: Color,
    /// Shadow colour (semi-transparent).
    pub shadow: Color,
    /// Text/icon colour drawn on top of [`surface`](Palette::surface).
    pub on_surface: Color,
    /// Text/icon colour drawn on top of [`background`](Palette::background).
    pub on_background: Color,
}

impl ExtendedPalette {
    /// Derive an extended palette from a base [`Palette`].
    ///
    /// `dark` selects status-colour brightness tuned for dark vs light surfaces.
    /// `on_surface` / `on_background` default to the base text colour.
    pub fn derive(base: Palette, dark: bool) -> Self {
        let (error, warning, success, info) = if dark {
            (
                Color(247, 118, 142, 255), // #F7768E Tokyo Night red
                Color(224, 175, 104, 255), // #E0AF68 yellow/orange
                Color(158, 206, 106, 255), // #9ECE6A green
                Color(125, 207, 255, 255), // #7DCFFF cyan
            )
        } else {
            (
                Color(196, 50, 70, 255),
                Color(176, 124, 40, 255),
                Color(86, 148, 40, 255),
                Color(40, 120, 180, 255),
            )
        };
        let outline = base.muted;
        let on_surface = base.text;
        let on_background = base.text;
        let shadow = Color(0, 0, 0, if dark { 160 } else { 64 });
        // A surface variant: nudge the surface toward the background a little.
        let surface_variant = crate::color::mix(base.surface, base.background);
        Self {
            base,
            error,
            warning,
            success,
            info,
            surface_variant,
            outline,
            shadow,
            on_surface,
            on_background,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::wcag_contrast;

    #[test]
    fn derive_preserves_base_and_adds_status() {
        let base = oxiui_core::Palette {
            background: Color(26, 27, 38, 255),
            surface: Color(36, 40, 59, 255),
            primary: Color(122, 162, 247, 255),
            on_primary: Color(26, 27, 38, 255),
            text: Color(192, 202, 245, 255),
            muted: Color(86, 95, 137, 255),
        };
        let ext = ExtendedPalette::derive(base, true);
        assert_eq!(ext.base.background, Color(26, 27, 38, 255));
        // Status colours are distinct from each other.
        assert_ne!(ext.error, ext.success);
        assert_ne!(ext.warning, ext.info);
        // on_surface defaults to text.
        assert_eq!(ext.on_surface, ext.base.text);
    }

    #[test]
    fn dark_status_colors_readable_on_dark_bg() {
        let base = oxiui_core::Palette {
            background: Color(26, 27, 38, 255),
            surface: Color(36, 40, 59, 255),
            primary: Color(122, 162, 247, 255),
            on_primary: Color(26, 27, 38, 255),
            text: Color(192, 202, 245, 255),
            muted: Color(86, 95, 137, 255),
        };
        let ext = ExtendedPalette::derive(base, true);
        // Status colours on the dark background should at least meet AA (3.0)
        // for large/graphical elements.
        let bg = (26, 27, 38);
        for c in [ext.error, ext.warning, ext.success, ext.info] {
            let ratio = wcag_contrast((c.0, c.1, c.2), bg);
            assert!(ratio >= 3.0, "status colour contrast {ratio} too low");
        }
    }
}
