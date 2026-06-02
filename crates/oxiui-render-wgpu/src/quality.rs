//! Configurable render-quality settings.
//!
//! Quality presets are provided for convenience; callers may also construct
//! a [`RenderQuality`] manually for fine-grained control.

// ── Shadow quality ─────────────────────────────────────────────────────────────

/// Controls the fidelity of box-shadow rendering.
#[derive(Clone, Debug, PartialEq)]
pub enum ShadowQuality {
    /// Shadows are not rendered.
    Off,
    /// Low-quality (fast) shadow approximation.
    Low,
    /// High-quality (expensive) shadow blur.
    High,
}

// ── Text quality ──────────────────────────────────────────────────────────────

/// Controls the anti-aliasing strategy used for text rendering.
#[derive(Clone, Debug, PartialEq)]
pub enum TextQuality {
    /// Greyscale anti-aliasing (single-channel coverage).
    Grayscale,
    /// Subpixel anti-aliasing (RGB stripe).
    Subpixel,
    /// Signed-distance-field glyph rendering (resolution-independent).
    Sdf,
}

// ── RenderQuality ─────────────────────────────────────────────────────────────

/// Aggregated quality settings for the wgpu render pipeline.
///
/// Use one of the preset constructors ([`low`], [`balanced`], [`high`]) or
/// construct directly for custom tuning.
///
/// [`low`]: RenderQuality::low
/// [`balanced`]: RenderQuality::balanced
/// [`high`]: RenderQuality::high
#[derive(Clone, Debug)]
pub struct RenderQuality {
    /// MSAA sample count.  Must be a power of two; 1 disables MSAA.
    pub msaa: u32,
    /// Shadow rendering quality.
    pub shadow: ShadowQuality,
    /// Text rendering quality.
    pub text: TextQuality,
}

impl RenderQuality {
    /// Low-quality preset: no MSAA, no shadows, greyscale text.
    pub fn low() -> Self {
        Self {
            msaa: 1,
            shadow: ShadowQuality::Off,
            text: TextQuality::Grayscale,
        }
    }

    /// Balanced preset: 4× MSAA, low-quality shadows, SDF text.
    pub fn balanced() -> Self {
        Self {
            msaa: 4,
            shadow: ShadowQuality::Low,
            text: TextQuality::Sdf,
        }
    }

    /// High-quality preset: 8× MSAA, high-quality shadows, SDF text.
    pub fn high() -> Self {
        Self {
            msaa: 8,
            shadow: ShadowQuality::High,
            text: TextQuality::Sdf,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_presets_correct_values() {
        let low = RenderQuality::low();
        assert_eq!(low.msaa, 1);
        assert_eq!(low.shadow, ShadowQuality::Off);
        assert_eq!(low.text, TextQuality::Grayscale);

        let balanced = RenderQuality::balanced();
        assert_eq!(balanced.msaa, 4);
        assert_eq!(balanced.shadow, ShadowQuality::Low);
        assert_eq!(balanced.text, TextQuality::Sdf);

        let high = RenderQuality::high();
        assert_eq!(high.msaa, 8);
        assert_eq!(high.shadow, ShadowQuality::High);
        assert_eq!(high.text, TextQuality::Sdf);
    }
}
