//! Design tokens: spacing, border-radius, elevation, and opacity scales.
//!
//! These provide a consistent, theme-wide vocabulary of sizes so that widgets
//! reference semantic steps (`spacing.md`) rather than magic numbers.

/// A named step within the spacing scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacingStep {
    /// Extra-small spacing (tightest).
    Xs,
    /// Small spacing.
    Sm,
    /// Medium spacing.
    Md,
    /// Large spacing.
    Lg,
    /// Extra-large spacing.
    Xl,
    /// Double extra-large spacing.
    Xxl,
    /// Triple extra-large spacing (loosest).
    Xxxl,
}

/// A named step within the border-radius scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadiusStep {
    /// No rounding (sharp corners).
    None,
    /// Small radius.
    Sm,
    /// Medium radius.
    Md,
    /// Large radius.
    Lg,
    /// Extra-large radius.
    Xl,
    /// Fully rounded (pill / circle).
    Full,
}

/// The full set of design tokens for a theme.
///
/// Arrays are indexed by their corresponding step enum's discriminant order.
#[derive(Clone, Debug, PartialEq)]
pub struct DesignTokens {
    /// Spacing scale in logical pixels (7 steps, `Xs`..`Xxxl`).
    pub spacing: [f32; 7],
    /// Border-radius scale in logical pixels (6 steps, `None`..`Full`).
    pub radius: [f32; 6],
    /// Elevation blur radii in logical pixels (6 levels, 0..=5).
    pub elevation: [f32; 6],
    /// Opacity levels in `[0, 1]` (5 steps: disabled..opaque).
    pub opacity: [f32; 5],
}

impl DesignTokens {
    /// Spacing value (logical px) for a named step.
    pub fn spacing(&self, step: SpacingStep) -> f32 {
        let i = match step {
            SpacingStep::Xs => 0,
            SpacingStep::Sm => 1,
            SpacingStep::Md => 2,
            SpacingStep::Lg => 3,
            SpacingStep::Xl => 4,
            SpacingStep::Xxl => 5,
            SpacingStep::Xxxl => 6,
        };
        self.spacing[i]
    }

    /// Border-radius value (logical px) for a named step.
    pub fn radius(&self, step: RadiusStep) -> f32 {
        let i = match step {
            RadiusStep::None => 0,
            RadiusStep::Sm => 1,
            RadiusStep::Md => 2,
            RadiusStep::Lg => 3,
            RadiusStep::Xl => 4,
            RadiusStep::Full => 5,
        };
        self.radius[i]
    }

    /// Elevation blur radius (logical px) for level `0..=5` (clamped).
    pub fn elevation(&self, level: usize) -> f32 {
        self.elevation[level.min(self.elevation.len() - 1)]
    }
}

impl Default for DesignTokens {
    /// The COOLJAPAN default scale: 4-px-based spacing, conventional radii.
    fn default() -> Self {
        Self {
            // 4 / 8 / 12 / 16 / 24 / 32 / 48 — all multiples of 4.
            spacing: [4.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0],
            // none / sm / md / lg / xl / full.
            radius: [0.0, 2.0, 4.0, 8.0, 16.0, 9999.0],
            // 0..5 shadow blur.
            elevation: [0.0, 1.0, 3.0, 6.0, 12.0, 24.0],
            // disabled / muted / secondary / high / opaque.
            opacity: [0.38, 0.60, 0.74, 0.87, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_is_multiple_of_four() {
        let t = DesignTokens::default();
        for v in t.spacing {
            assert_eq!(v % 4.0, 0.0, "spacing {v} not a multiple of 4");
        }
    }

    #[test]
    fn spacing_is_monotonic() {
        let t = DesignTokens::default();
        for w in t.spacing.windows(2) {
            assert!(w[1] > w[0], "spacing scale must increase");
        }
    }

    #[test]
    fn radius_non_negative_and_named_lookup() {
        let t = DesignTokens::default();
        for v in t.radius {
            assert!(v >= 0.0);
        }
        assert_eq!(t.radius(RadiusStep::None), 0.0);
        assert!(t.radius(RadiusStep::Full) > t.radius(RadiusStep::Lg));
    }

    #[test]
    fn named_spacing_lookup() {
        let t = DesignTokens::default();
        assert_eq!(t.spacing(SpacingStep::Xs), 4.0);
        assert_eq!(t.spacing(SpacingStep::Md), 12.0);
        assert_eq!(t.spacing(SpacingStep::Xxxl), 48.0);
    }

    #[test]
    fn elevation_clamps() {
        let t = DesignTokens::default();
        assert_eq!(t.elevation(0), 0.0);
        assert_eq!(t.elevation(5), 24.0);
        // Out-of-range clamps to the last level.
        assert_eq!(t.elevation(99), 24.0);
    }

    #[test]
    fn opacity_in_range_and_monotonic() {
        let t = DesignTokens::default();
        for v in t.opacity {
            assert!((0.0..=1.0).contains(&v));
        }
        for w in t.opacity.windows(2) {
            assert!(w[1] > w[0]);
        }
    }
}
