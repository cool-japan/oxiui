//! COOLJAPAN palette → iced theme conversion.
//!
//! Maps `oxiui_core::Palette` semantic colours to the closest equivalents in
//! `iced::theme::palette::Palette` (iced 0.14).  The mapping is intentionally
//! lossy: OxiUI has 6 semantic slots; iced's palette has 6 as well
//! (background, text, primary, success, warning, danger), but the semantics
//! differ slightly.  Cosmetic slots without a direct equivalent reuse `muted`.
//!
//! Also provides [`DesignTokensAdapter`] to expose `oxiui-theme` design tokens
//! and typography as iced-compatible values (pixel sizes, padding), and the
//! [`palette_and_tokens_to_iced_theme`] convenience wrapper.

use iced::widget::scrollable;
use iced::widget::text_input;
use iced::{Background, Border};
use oxiui_core::{Palette, Theme};
use oxiui_theme::{DesignTokens, RadiusStep, SpacingStep, TypographyScale};

/// Convert an OxiUI [`Palette`] into an `iced::Theme`.
///
/// The resulting theme is a `Theme::Custom` variant containing an
/// `iced::theme::palette::Palette` whose fields are populated from the
/// OxiUI palette using the following mapping:
///
/// | iced field | OxiUI source       |
/// |------------|-------------------|
/// | background | palette.background |
/// | text       | palette.text       |
/// | primary    | palette.primary    |
/// | success    | palette.muted      |
/// | warning    | palette.muted      |
/// | danger     | palette.surface    |
pub fn palette_to_iced_theme(p: &Palette) -> iced::Theme {
    let iced_palette = iced::theme::palette::Palette {
        background: color_to_iced(&p.background),
        text: color_to_iced(&p.text),
        primary: color_to_iced(&p.primary),
        success: color_to_iced(&p.muted),
        warning: color_to_iced(&p.muted),
        danger: color_to_iced(&p.surface),
    };
    iced::Theme::custom("OxiUI COOLJAPAN", iced_palette)
}

/// Convert an OxiUI [`Theme`] trait object into an `iced::Theme`.
///
/// This is an extended version of [`palette_to_iced_theme`] that accepts any
/// `&dyn Theme` (rather than a bare `&Palette`), extracting the palette via
/// [`Theme::palette`] and delegating to the core conversion.
///
/// The `oxiui_core::Palette` has the following semantic fields available for
/// mapping (there are no separate error/warning/success fields in the current
/// palette schema; those iced slots receive the closest approximation):
///
/// | iced field | OxiUI source      | Rationale                         |
/// |------------|-------------------|-----------------------------------|
/// | background | palette.background | direct match                      |
/// | text       | palette.text       | direct match                      |
/// | primary    | palette.primary    | direct match                      |
/// | success    | palette.muted      | no success slot → muted (subdued) |
/// | warning    | palette.muted      | no warning slot → muted (subdued) |
/// | danger     | palette.surface    | no error slot → surface (neutral) |
///
/// When `oxiui_core::Palette` gains error/warning/success fields in a future
/// milestone, update this function's mapping accordingly.
pub fn palette_to_iced_theme_ext(theme: &dyn Theme) -> iced::Theme {
    palette_to_iced_theme(theme.palette())
}

pub(crate) fn color_to_iced(c: &oxiui_core::Color) -> iced::Color {
    let oxiui_core::Color(r, g, b, a) = *c;
    iced::Color::from_rgba8(r, g, b, a as f32 / 255.0)
}

/// Produce an iced `text_input::Style` derived from an OxiUI [`Palette`].
///
/// Sets `border.color` from the palette's `primary` colour and `border.radius`
/// to 2 px. The background, placeholder, value, and selection colours fall
/// back to sensible defaults from the palette.
pub fn text_input_style_from_palette(p: &Palette) -> text_input::Style {
    let background_color = color_to_iced(&p.background);
    let text_color = color_to_iced(&p.text);
    let primary_color = color_to_iced(&p.primary);
    let muted_color = color_to_iced(&p.muted);
    let mut selection = primary_color;
    selection.a = 0.4;

    text_input::Style {
        background: Background::Color(background_color),
        border: Border {
            color: primary_color,
            width: 1.0,
            radius: 2.0.into(),
        },
        icon: muted_color,
        placeholder: muted_color,
        value: text_color,
        selection,
    }
}

/// Produce an iced `scrollable::Style` derived from an OxiUI [`Palette`].
///
/// Sets the scroller background to the palette's `primary` colour and the rail
/// background to the palette's `surface` colour.
pub fn scrollable_style_from_palette(p: &Palette) -> scrollable::Style {
    let primary_color = color_to_iced(&p.primary);
    let surface_color = color_to_iced(&p.surface);

    let rail = scrollable::Rail {
        background: Some(Background::Color(surface_color)),
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: Background::Color(primary_color),
            border: Border::default(),
        },
    };

    scrollable::Style {
        container: iced::widget::container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(surface_color),
            border: Border::default(),
            shadow: iced::Shadow::default(),
            icon: primary_color,
        },
    }
}

/// Produce `text_input_style_from_palette` using a `&dyn Theme` trait object.
pub fn text_input_style_from_theme(theme: &dyn Theme) -> text_input::Style {
    text_input_style_from_palette(theme.palette())
}

/// Produce `scrollable_style_from_palette` using a `&dyn Theme` trait object.
pub fn scrollable_style_from_theme(theme: &dyn Theme) -> scrollable::Style {
    scrollable_style_from_palette(theme.palette())
}

// ── DesignTokens integration ──────────────────────────────────────────────────

/// Convert an OxiUI [`Palette`] into an `iced::Theme`, optionally informed by
/// [`DesignTokens`] and [`TypographyScale`].
///
/// # Limitation (iced 0.14)
///
/// `iced::Theme::Custom` wraps only a colour palette — it has no slots for
/// border radius, spacing, or typography. Those values cannot be folded into
/// the returned `iced::Theme` at this level; they are exposed through
/// [`DesignTokensAdapter`] for per-widget use instead.  The `_tokens` and
/// `_typography` parameters are accepted for API symmetry and future extension
/// but do not alter the produced theme in iced 0.14.
///
/// # Deviation note
///
/// Full "respect tokens" integration requires threading a [`DesignTokensAdapter`]
/// into individual widget render sites (e.g. `text_input_style_from_palette`
/// border radius, `build_one` heading/body font sizes). That per-site wiring is
/// a separate follow-up; this function provides the public seam.
pub fn palette_and_tokens_to_iced_theme(
    palette: &Palette,
    _tokens: Option<&DesignTokens>,
    _typography: Option<&TypographyScale>,
) -> iced::Theme {
    // iced 0.14 Theme::Custom holds only colours; tokens cannot be embedded.
    palette_to_iced_theme(palette)
}

/// Applies [`DesignTokens`] and [`TypographyScale`] to produce iced-compatible
/// style values for use in per-widget style helpers.
///
/// iced 0.14 has no global style-override hook — styles are set per-widget
/// (e.g. `button::style`, `text_input::style`). `DesignTokensAdapter` exposes
/// the token values as iced primitives so callers can apply them at the widget
/// call site without repeating the token-field mapping.
///
/// # Example
///
/// ```rust
/// use oxiui_iced::DesignTokensAdapter;
/// use oxiui_theme::{DesignTokens, TypographyScale};
///
/// let adapter = DesignTokensAdapter::from_tokens(
///     &DesignTokens::default(),
///     &TypographyScale::default(),
/// );
/// assert!(adapter.body_font_size > 0.0);
/// let _padding = adapter.standard_padding();
/// let _body_sz = adapter.body_text_size();
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignTokensAdapter {
    /// Medium border radius in logical pixels (mapped from `RadiusStep::Md`).
    pub border_radius: f32,
    /// Body font size in logical pixels (from `TypographyScale::body.size`).
    pub body_font_size: f32,
    /// Headline font size in logical pixels (from `TypographyScale::headline.size`).
    pub headline_font_size: f32,
    /// Base spacing in logical pixels (mapped from `SpacingStep::Sm`, 8 px by default).
    pub base_spacing: f32,
}

impl DesignTokensAdapter {
    /// Build an adapter from references to a [`DesignTokens`] and a
    /// [`TypographyScale`].
    ///
    /// Field mapping:
    /// - `border_radius` ← `tokens.radius(RadiusStep::Md)` (4.0 px by default)
    /// - `body_font_size` ← `typography.body.size` (14.0 px by default)
    /// - `headline_font_size` ← `typography.headline.size` (24.0 px by default)
    /// - `base_spacing` ← `tokens.spacing(SpacingStep::Sm)` (8.0 px by default)
    pub fn from_tokens(tokens: &DesignTokens, typography: &TypographyScale) -> Self {
        Self {
            border_radius: tokens.radius(RadiusStep::Md),
            body_font_size: typography.body.size,
            headline_font_size: typography.headline.size,
            base_spacing: tokens.spacing(SpacingStep::Sm),
        }
    }

    /// Returns an iced text size for body text.
    pub fn body_text_size(&self) -> iced::Pixels {
        iced::Pixels(self.body_font_size)
    }

    /// Returns an iced text size for headlines.
    pub fn headline_text_size(&self) -> iced::Pixels {
        iced::Pixels(self.headline_font_size)
    }

    /// Returns an iced [`iced::Padding`] with uniform padding equal to
    /// `base_spacing` on all sides.
    pub fn standard_padding(&self) -> iced::Padding {
        iced::Padding::from(self.base_spacing)
    }
}
