//! Fluent [`PaletteBuilder`] with WCAG contrast validation.
//!
//! ```rust
//! use oxiui_core::{Color, Palette};
//! use oxiui_theme::builder::{PaletteBuilder, WcagLevel};
//!
//! let result = PaletteBuilder::new()
//!     .background(Color(0, 0, 0, 255))
//!     .surface(Color(10, 10, 26, 255))
//!     .text_primary(Color(255, 255, 255, 255))
//!     .text_secondary(Color(200, 200, 200, 255))
//!     .primary(Color(255, 255, 0, 255))
//!     .on_primary(Color(0, 0, 0, 255))
//!     .validate();
//!
//! assert!(result.is_aa_compliant);
//! ```

use crate::high_contrast::wcag_contrast;
use crate::CooljapanTheme;
use oxiui_core::{Color, FontSpec, Palette};

/// WCAG conformance level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WcagLevel {
    /// Minimum accessibility — contrast ratio ≥ 4.5:1 for normal text.
    AA,
    /// Enhanced accessibility — contrast ratio ≥ 7.0:1 for normal text.
    AAA,
}

/// A contrast warning for a foreground/background colour pair.
#[derive(Clone, Debug, PartialEq)]
pub struct ContrastWarning {
    /// Names of the foreground and background roles.
    pub pair: (&'static str, &'static str),
    /// Actual contrast ratio.
    pub ratio: f64,
    /// Minimum required contrast for the failing level.
    pub required: f64,
    /// The WCAG level that this pair fails.
    pub level: WcagLevel,
}

/// The outcome of [`PaletteBuilder::validate`].
#[derive(Clone, Debug)]
pub struct ValidationResult {
    /// All contrast warnings (pairs that fail AA or AAA).
    pub warnings: Vec<ContrastWarning>,
    /// `true` if every checked pair meets ≥ 4.5:1 (WCAG AA).
    pub is_aa_compliant: bool,
    /// `true` if every checked pair meets ≥ 7.0:1 (WCAG AAA).
    pub is_aaa_compliant: bool,
}

/// Fluent builder for a [`Palette`] with WCAG contrast validation.
///
/// All fields are optional; [`build`](PaletteBuilder::build) falls back to
/// safe defaults for any field not provided so that partial palettes compile.
#[derive(Clone, Debug, Default)]
pub struct PaletteBuilder {
    background: Option<Color>,
    surface: Option<Color>,
    text_primary: Option<Color>,
    text_secondary: Option<Color>,
    primary: Option<Color>,
    on_primary: Option<Color>,
}

impl PaletteBuilder {
    /// Create a new builder with all fields unset.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the background colour.
    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }

    /// Set the surface colour (cards, dialogs).
    pub fn surface(mut self, c: Color) -> Self {
        self.surface = Some(c);
        self
    }

    /// Set the primary text colour.
    pub fn text_primary(mut self, c: Color) -> Self {
        self.text_primary = Some(c);
        self
    }

    /// Set the secondary / muted text colour.
    pub fn text_secondary(mut self, c: Color) -> Self {
        self.text_secondary = Some(c);
        self
    }

    /// Set the primary brand / accent colour.
    pub fn primary(mut self, c: Color) -> Self {
        self.primary = Some(c);
        self
    }

    /// Set the "on-primary" text colour (drawn on top of `primary`).
    pub fn on_primary(mut self, c: Color) -> Self {
        self.on_primary = Some(c);
        self
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn resolved_background(&self) -> Color {
        self.background.unwrap_or(Color(255, 255, 255, 255))
    }
    fn resolved_surface(&self) -> Color {
        self.surface.unwrap_or(Color(255, 255, 255, 255))
    }
    fn resolved_text_primary(&self) -> Color {
        self.text_primary.unwrap_or(Color(0, 0, 0, 255))
    }
    fn resolved_text_secondary(&self) -> Color {
        self.text_secondary.unwrap_or(Color(60, 60, 60, 255))
    }
    fn resolved_primary(&self) -> Color {
        self.primary.unwrap_or(Color(0, 0, 200, 255))
    }
    fn resolved_on_primary(&self) -> Color {
        self.on_primary.unwrap_or(Color(255, 255, 255, 255))
    }

    // ── Validation ────────────────────────────────────────────────────────

    /// Validate all foreground/background pairs and return a [`ValidationResult`].
    pub fn validate(&self) -> ValidationResult {
        let bg = self.resolved_background();
        let surface = self.resolved_surface();
        let text = self.resolved_text_primary();
        let muted = self.resolved_text_secondary();
        let primary = self.resolved_primary();
        let on_primary = self.resolved_on_primary();

        // Pairs to check: (fg, bg, fg_name, bg_name)
        let pairs: &[(Color, Color, &'static str, &'static str)] = &[
            (text, bg, "text_primary", "background"),
            (muted, bg, "text_secondary", "background"),
            (text, surface, "text_primary", "surface"),
            (muted, surface, "text_secondary", "surface"),
            (on_primary, primary, "on_primary", "primary"),
        ];

        let mut warnings = Vec::new();
        for &(fg, back, fg_name, bg_name) in pairs {
            let ratio = wcag_contrast((fg.0, fg.1, fg.2), (back.0, back.1, back.2));
            if ratio < 4.5 {
                warnings.push(ContrastWarning {
                    pair: (fg_name, bg_name),
                    ratio,
                    required: 4.5,
                    level: WcagLevel::AA,
                });
            } else if ratio < 7.0 {
                warnings.push(ContrastWarning {
                    pair: (fg_name, bg_name),
                    ratio,
                    required: 7.0,
                    level: WcagLevel::AAA,
                });
            }
        }

        let is_aa_compliant = warnings.iter().all(|w| w.level != WcagLevel::AA);
        let is_aaa_compliant = warnings.is_empty();
        ValidationResult {
            warnings,
            is_aa_compliant,
            is_aaa_compliant,
        }
    }

    /// Assemble a [`CooljapanTheme`] from the builder's colours.
    ///
    /// Returns `Err` if any foreground/background pair fails WCAG AA (< 4.5:1).
    pub fn build(self) -> Result<CooljapanTheme, Vec<ContrastWarning>> {
        let result = self.validate();
        // Collect only AA failures (ratio < 4.5) as hard errors.
        let aa_failures: Vec<ContrastWarning> = result
            .warnings
            .into_iter()
            .filter(|w| w.level == WcagLevel::AA)
            .collect();
        if !aa_failures.is_empty() {
            return Err(aa_failures);
        }
        let palette = Palette {
            background: self.resolved_background(),
            surface: self.resolved_surface(),
            primary: self.resolved_primary(),
            on_primary: self.resolved_on_primary(),
            text: self.resolved_text_primary(),
            muted: self.resolved_text_secondary(),
        };
        Ok(CooljapanTheme::new(
            palette,
            FontSpec::new("Inter", 14.0, 400),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::Color;

    fn high_contrast_builder() -> PaletteBuilder {
        PaletteBuilder::new()
            .background(Color(0, 0, 0, 255))
            .surface(Color(10, 10, 26, 255))
            .text_primary(Color(255, 255, 255, 255))
            .text_secondary(Color(200, 200, 200, 255))
            .primary(Color(255, 255, 0, 255))
            .on_primary(Color(0, 0, 0, 255))
    }

    #[test]
    fn builder_valid_palette_builds() {
        let result = high_contrast_builder().build();
        assert!(
            result.is_ok(),
            "high-contrast builder should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn builder_aaa_flag() {
        let result = high_contrast_builder().validate();
        assert!(
            result.is_aaa_compliant,
            "all pairs should be AAA; warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn builder_low_contrast_warns() {
        // Light grey on white — very low contrast.
        let result = PaletteBuilder::new()
            .background(Color(255, 255, 255, 255))
            .text_primary(Color(200, 200, 200, 255)) // near-white text
            .validate();
        assert!(
            !result.warnings.is_empty(),
            "should warn about low contrast"
        );
    }

    #[test]
    fn builder_aa_failure_returns_err() {
        // Near-identical colours: near-white text on white background.
        let result = PaletteBuilder::new()
            .background(Color(255, 255, 255, 255))
            .text_primary(Color(240, 240, 240, 255))
            .build();
        assert!(result.is_err(), "near-white on white should fail AA build");
    }

    #[test]
    fn builder_default_is_accessible() {
        // Default colours (black text on white bg) must pass AA.
        let result = PaletteBuilder::new().validate();
        assert!(result.is_aa_compliant);
    }
}
