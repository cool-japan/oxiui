//! `oxiui-theme` integration for `oxiui-table`.
//!
//! Provides [`TableTheme`] — a colour token set derived from
//! [`oxiui_theme::DesignTokens`] and [`oxiui_core::Palette`] that encodes the
//! visual appearance of the table without coupling the core table logic to
//! any specific renderer.
//!
//! Renderers (egui, iced, soft, wgpu) can call [`TableTheme::from_palette`]
//! or [`TableTheme::from_tokens`] to obtain a backend-agnostic colour
//! description, then map the `[u8; 4]` RGBA values to their native colour
//! types.
//!
//! # Usage
//!
//! ```rust
//! use oxiui_table::theme_integration::TableTheme;
//!
//! let theme = TableTheme::default();
//! // Header background: semi-transparent primary colour.
//! let [r, g, b, a] = theme.header_bg;
//! assert!(a > 0);
//! ```
//!
//! # Feature flag
//!
//! The [`TableTheme::from_palette`] and [`TableTheme::from_tokens`]
//! constructors are only available when the `theme-table` feature is enabled
//! (which gates `oxiui-theme`).  The struct itself and the `Default` impl are
//! always compiled so that downstream code does not need conditional
//! compilation at the usage site.

#[cfg(feature = "theme-table")]
use oxiui_core::Palette;
#[cfg(feature = "theme-table")]
use oxiui_theme::tokens::DesignTokens;

// ── TableTheme ────────────────────────────────────────────────────────────────

/// Colour tokens for the table widget, expressed as RGBA `[u8; 4]` arrays.
///
/// All colours use pre-multiplied-alpha conventions: `a == 0` means
/// fully transparent, `a == 255` means fully opaque.
///
/// The defaults correspond to the COOLJAPAN Tokyo Night dark palette.
#[derive(Clone, Debug, PartialEq)]
pub struct TableTheme {
    /// Background colour of the column header row.
    pub header_bg: [u8; 4],
    /// Foreground (text) colour of the column header row.
    pub header_fg: [u8; 4],
    /// Background colour for data rows (even rows when zebra striping is off).
    pub row_bg: [u8; 4],
    /// Background colour for alternate (odd) rows in zebra-striping mode.
    pub row_stripe_bg: [u8; 4],
    /// Background colour for the currently selected / highlighted row.
    pub selection_bg: [u8; 4],
    /// Foreground (text) colour for the currently selected row.
    pub selection_fg: [u8; 4],
    /// Border / grid-line colour between rows and columns.
    pub border_color: [u8; 4],
    /// Background colour of the cell that holds keyboard focus.
    pub focus_ring_color: [u8; 4],
    /// Default foreground (text) colour for all data cells.
    pub cell_fg: [u8; 4],
    /// Background colour for the footer (aggregate) row, if present.
    pub footer_bg: [u8; 4],
    /// Foreground (text) colour for the footer row.
    pub footer_fg: [u8; 4],
    /// Cell horizontal padding derived from the spacing scale (logical pixels).
    pub cell_padding_x: f32,
    /// Cell vertical padding derived from the spacing scale (logical pixels).
    pub cell_padding_y: f32,
    /// Border radius for the focus ring (logical pixels).
    pub focus_radius: f32,
}

impl Default for TableTheme {
    /// COOLJAPAN Tokyo Night dark defaults.
    ///
    /// These values are hard-coded so the struct is usable without the
    /// `theme-table` feature.  When the feature is enabled, prefer
    /// [`TableTheme::from_palette`] to derive colours from the active theme.
    fn default() -> Self {
        Self {
            // Header: #24283B surface colour, fully opaque text.
            header_bg: [36, 40, 59, 255],
            header_fg: [192, 202, 245, 255], // #C0CAF5
            // Rows: dark background (#1A1B26), slightly lighter surface (#1F2035).
            row_bg: [26, 27, 38, 255],
            row_stripe_bg: [31, 32, 53, 255],
            // Selection: primary #7AA2F7 at 30 % opacity over a dark base.
            selection_bg: [122, 162, 247, 77], // alpha = 0.30 * 255 ≈ 77
            selection_fg: [255, 255, 255, 255],
            // Subtle borders.
            border_color: [86, 95, 137, 128], // muted at 50 % alpha
            // Focus ring: primary colour, 50 % alpha.
            focus_ring_color: [122, 162, 247, 128],
            // Regular cell text.
            cell_fg: [192, 202, 245, 255],
            // Footer: same as header.
            footer_bg: [36, 40, 59, 255],
            footer_fg: [192, 202, 245, 200],
            // Spacing defaults (4-px grid).
            cell_padding_x: 8.0,
            cell_padding_y: 4.0,
            focus_radius: 2.0,
        }
    }
}

impl TableTheme {
    /// Build a `TableTheme` from a [`Palette`] and optional [`DesignTokens`].
    ///
    /// When the `theme-table` feature is **disabled** this constructor is
    /// unavailable; use [`TableTheme::default`] instead.
    #[cfg(feature = "theme-table")]
    pub fn from_palette(palette: &Palette, tokens: Option<&DesignTokens>) -> Self {
        use oxiui_theme::color::darken;

        // Derive zebra-stripe colour by darkening the base background slightly.
        let stripe_color = darken(palette.background, 0.05);

        // Selection background: primary colour at 30 % alpha.
        let sel_a = (0.30_f32 * 255.0_f32).round() as u8;
        let selection_bg = [
            palette.primary.0,
            palette.primary.1,
            palette.primary.2,
            sel_a,
        ];

        // Border: muted colour at 50 % alpha.
        let border_color = [palette.muted.0, palette.muted.1, palette.muted.2, 128];

        // Focus ring: primary colour at 50 % alpha.
        let focus_ring_color = [palette.primary.0, palette.primary.1, palette.primary.2, 128];

        // Footer: surface colour, slightly dimmed text.
        let footer_fg = [palette.text.0, palette.text.1, palette.text.2, 200];

        // Spacing from tokens if provided, otherwise the 4-px grid defaults.
        let (cell_padding_x, cell_padding_y, focus_radius) = if let Some(t) = tokens {
            use oxiui_theme::tokens::{RadiusStep, SpacingStep};
            (
                t.spacing(SpacingStep::Sm), // 8 px
                t.spacing(SpacingStep::Xs), // 4 px
                t.radius(RadiusStep::Sm),   // 2 px
            )
        } else {
            (8.0, 4.0, 2.0)
        };

        Self {
            header_bg: [palette.surface.0, palette.surface.1, palette.surface.2, 255],
            header_fg: [palette.text.0, palette.text.1, palette.text.2, 255],
            row_bg: [
                palette.background.0,
                palette.background.1,
                palette.background.2,
                255,
            ],
            row_stripe_bg: [stripe_color.0, stripe_color.1, stripe_color.2, 255],
            selection_bg,
            selection_fg: [
                palette.on_primary.0,
                palette.on_primary.1,
                palette.on_primary.2,
                255,
            ],
            border_color,
            focus_ring_color,
            cell_fg: [palette.text.0, palette.text.1, palette.text.2, 255],
            footer_bg: [palette.surface.0, palette.surface.1, palette.surface.2, 255],
            footer_fg,
            cell_padding_x,
            cell_padding_y,
            focus_radius,
        }
    }

    /// Build a `TableTheme` from a [`DesignTokens`] alone, falling back to the
    /// COOLJAPAN default palette colours.
    ///
    /// Only available when the `theme-table` feature is enabled.
    #[cfg(feature = "theme-table")]
    pub fn from_tokens(tokens: &DesignTokens) -> Self {
        use oxiui_core::Theme;
        use oxiui_core::{Color, FontSpec, Palette};
        use oxiui_theme::CooljapanTheme;

        // Instantiate the default COOLJAPAN dark theme to obtain its palette.
        let theme = CooljapanTheme::new(
            Palette {
                background: Color(26, 27, 38, 255),
                surface: Color(36, 40, 59, 255),
                primary: Color(122, 162, 247, 255),
                on_primary: Color(26, 27, 38, 255),
                text: Color(192, 202, 245, 255),
                muted: Color(86, 95, 137, 255),
            },
            FontSpec::new("Inter", 14.0, 400),
        );
        Self::from_palette(theme.palette(), Some(tokens))
    }

    /// Whether the colour scheme is perceived as dark (background luminance < 0.5).
    ///
    /// Useful for selecting icon variants or blend modes in renderers.
    pub fn is_dark(&self) -> bool {
        let [r, g, b, _] = self.row_bg;
        // WCAG relative luminance approximation (not gamma-correct but fast).
        let luma =
            0.2126 * (r as f32 / 255.0) + 0.7152 * (g as f32 / 255.0) + 0.0722 * (b as f32 / 255.0);
        luma < 0.5
    }

    /// Apply an alpha blend: mix the theme's `selection_bg` over `row_bg` for
    /// a given `is_selected` flag.
    ///
    /// Returns the resulting RGBA colour for the row background.
    pub fn effective_row_bg(&self, row_index: usize, is_selected: bool, zebra: bool) -> [u8; 4] {
        if is_selected {
            // Blend selection over the base row colour.
            let base = if zebra && row_index % 2 == 1 {
                self.row_stripe_bg
            } else {
                self.row_bg
            };
            alpha_blend(self.selection_bg, base)
        } else if zebra && row_index % 2 == 1 {
            self.row_stripe_bg
        } else {
            self.row_bg
        }
    }
}

/// Alpha-blend `src` (with `src.a` alpha) over `dst` (opaque).
///
/// Result is fully opaque.  Uses integer arithmetic to avoid floating-point.
fn alpha_blend(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let a = src[3] as u32;
    let ia = 255 - a;
    let blend = |s: u8, d: u8| -> u8 {
        let v = a * s as u32 + ia * d as u32;
        ((v + 127) / 255) as u8
    };
    [
        blend(src[0], dst[0]),
        blend(src[1], dst[1]),
        blend(src[2], dst[2]),
        255,
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_dark() {
        assert!(TableTheme::default().is_dark());
    }

    #[test]
    fn default_header_bg_is_surface() {
        let theme = TableTheme::default();
        // Surface = #24283B = (36, 40, 59)
        assert_eq!(theme.header_bg[0], 36);
        assert_eq!(theme.header_bg[1], 40);
        assert_eq!(theme.header_bg[2], 59);
        assert_eq!(theme.header_bg[3], 255);
    }

    #[test]
    fn effective_row_bg_normal() {
        let theme = TableTheme::default();
        let bg = theme.effective_row_bg(0, false, false);
        assert_eq!(bg, theme.row_bg);
    }

    #[test]
    fn effective_row_bg_zebra_odd() {
        let theme = TableTheme::default();
        let bg = theme.effective_row_bg(1, false, true);
        assert_eq!(bg, theme.row_stripe_bg);
    }

    #[test]
    fn effective_row_bg_zebra_even() {
        let theme = TableTheme::default();
        let bg = theme.effective_row_bg(0, false, true);
        assert_eq!(bg, theme.row_bg);
    }

    #[test]
    fn effective_row_bg_selected_is_blended() {
        let theme = TableTheme::default();
        let bg = theme.effective_row_bg(0, true, false);
        // The selection_bg has alpha=77; the result must differ from both the
        // raw row_bg and the raw selection_bg (it's a blend).
        assert_ne!(bg, theme.row_bg);
        // Result must be fully opaque.
        assert_eq!(bg[3], 255);
    }

    #[test]
    fn alpha_blend_fully_transparent_is_dst() {
        let src = [100, 150, 200, 0]; // fully transparent
        let dst = [10, 20, 30, 255];
        let result = alpha_blend(src, dst);
        // Transparent src → dst unchanged (modulo rounding).
        assert!((result[0] as i32 - dst[0] as i32).abs() <= 1);
        assert!((result[1] as i32 - dst[1] as i32).abs() <= 1);
        assert!((result[2] as i32 - dst[2] as i32).abs() <= 1);
    }

    #[test]
    fn alpha_blend_fully_opaque_is_src() {
        let src = [100, 150, 200, 255]; // fully opaque
        let dst = [10, 20, 30, 255];
        let result = alpha_blend(src, dst);
        assert_eq!(result[0], 100);
        assert_eq!(result[1], 150);
        assert_eq!(result[2], 200);
        assert_eq!(result[3], 255);
    }

    #[test]
    fn selection_bg_has_partial_alpha() {
        // The default selection_bg must have partial alpha so blending works.
        let theme = TableTheme::default();
        let a = theme.selection_bg[3];
        assert!(
            a > 0 && a < 255,
            "selection_bg alpha should be partial, got {a}"
        );
    }

    #[test]
    fn cell_padding_positive() {
        let theme = TableTheme::default();
        assert!(theme.cell_padding_x > 0.0);
        assert!(theme.cell_padding_y > 0.0);
    }

    #[test]
    fn focus_radius_non_negative() {
        let theme = TableTheme::default();
        assert!(theme.focus_radius >= 0.0);
    }

    #[cfg(feature = "theme-table")]
    #[test]
    fn from_tokens_returns_valid_theme() {
        use oxiui_theme::tokens::DesignTokens;
        let tokens = DesignTokens::default();
        let theme = TableTheme::from_tokens(&tokens);
        // Cell padding must match the Sm/Xs spacing steps (8.0 / 4.0).
        assert!((theme.cell_padding_x - 8.0).abs() < f32::EPSILON);
        assert!((theme.cell_padding_y - 4.0).abs() < f32::EPSILON);
    }

    #[cfg(feature = "theme-table")]
    #[test]
    fn from_palette_header_bg_is_surface() {
        use oxiui_core::{Color, Palette};
        let palette = Palette {
            background: Color(10, 10, 10, 255),
            surface: Color(30, 30, 30, 255),
            primary: Color(100, 150, 200, 255),
            on_primary: Color(0, 0, 0, 255),
            text: Color(220, 220, 220, 255),
            muted: Color(80, 80, 80, 255),
        };
        let theme = TableTheme::from_palette(&palette, None);
        assert_eq!(theme.header_bg[0], 30);
        assert_eq!(theme.header_bg[1], 30);
        assert_eq!(theme.header_bg[2], 30);
    }

    #[cfg(feature = "theme-table")]
    #[test]
    fn from_palette_selection_has_partial_alpha() {
        use oxiui_core::{Color, Palette};
        let palette = Palette {
            background: Color(10, 10, 10, 255),
            surface: Color(30, 30, 30, 255),
            primary: Color(100, 150, 200, 255),
            on_primary: Color(0, 0, 0, 255),
            text: Color(220, 220, 220, 255),
            muted: Color(80, 80, 80, 255),
        };
        let theme = TableTheme::from_palette(&palette, None);
        let a = theme.selection_bg[3];
        assert!(a > 0 && a < 255, "selection alpha must be partial, got {a}");
    }
}
