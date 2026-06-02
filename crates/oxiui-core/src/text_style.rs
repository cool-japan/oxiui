//! Text rendering style for labels and headings.
//!
//! [`TextStyle`] is an opinionated, builder-pattern helper that captures
//! typography intent — size, weight, italic, colour, decorations — without
//! depending on any shaping library. Adapters consume it however they see fit.

/// Text rendering style for labels and headings.
///
/// All fields have sensible defaults (400-weight, upright, theme colour). Use
/// the builder methods to produce variant styles without repeating boilerplate.
///
/// # Example
/// ```
/// use oxiui_core::TextStyle;
///
/// let style = TextStyle::default()
///     .with_size(18.0)
///     .with_weight(600)
///     .with_color([0, 0, 0, 255]);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font size in points (`None` = use the theme default).
    pub font_size: Option<f32>,
    /// Font weight: 100 = thin, 400 = regular, 700 = bold, 900 = black.
    pub font_weight: u16,
    /// Whether the text is rendered with an italic face.
    pub italic: bool,
    /// Text colour in RGBA bytes (`None` = use the theme default).
    pub color: Option<[u8; 4]>,
    /// Line height multiplier (`None` = platform/theme default of ~1.2).
    pub line_height: Option<f32>,
    /// Additional horizontal space between glyphs, in pixels (0.0 = default).
    pub letter_spacing: f32,
    /// Draw a line beneath the text.
    pub underline: bool,
    /// Draw a line through the middle of the text.
    pub strikethrough: bool,
}

impl Default for TextStyle {
    /// Returns a regular-weight, upright style with all overrides unset.
    fn default() -> Self {
        Self {
            font_size: None,
            font_weight: 400,
            italic: false,
            color: None,
            line_height: None,
            letter_spacing: 0.0,
            underline: false,
            strikethrough: false,
        }
    }
}

impl TextStyle {
    /// A bold preset: weight 700, all other fields at their defaults.
    pub fn bold() -> Self {
        Self {
            font_weight: 700,
            ..Self::default()
        }
    }

    /// An italic preset: italic enabled, all other fields at their defaults.
    pub fn italic() -> Self {
        Self {
            italic: true,
            ..Self::default()
        }
    }

    /// A heading preset: 24 pt, bold (weight 700).
    pub fn heading() -> Self {
        Self {
            font_size: Some(24.0),
            font_weight: 700,
            ..Self::default()
        }
    }

    /// A body text preset: equivalent to `default()`.
    pub fn body() -> Self {
        Self::default()
    }

    /// A caption preset: 11 pt, regular weight.
    pub fn caption() -> Self {
        Self {
            font_size: Some(11.0),
            ..Self::default()
        }
    }

    /// Builder: override the font size in points.
    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Builder: override the font weight (100–900).
    pub fn with_weight(mut self, weight: u16) -> Self {
        self.font_weight = weight;
        self
    }

    /// Builder: set an explicit RGBA colour override.
    pub fn with_color(mut self, rgba: [u8; 4]) -> Self {
        self.color = Some(rgba);
        self
    }

    /// Builder: set the line height multiplier.
    pub fn with_line_height(mut self, multiplier: f32) -> Self {
        self.line_height = Some(multiplier);
        self
    }

    /// Builder: set the additional letter spacing in pixels.
    pub fn with_letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Builder: enable or disable the underline decoration.
    pub fn with_underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }

    /// Builder: enable or disable the strikethrough decoration.
    pub fn with_strikethrough(mut self, strikethrough: bool) -> Self {
        self.strikethrough = strikethrough;
        self
    }
}
