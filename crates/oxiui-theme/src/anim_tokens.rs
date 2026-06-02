//! Animation and transition tokens for OxiUI themes.
//!
//! Provides [`TransitionSpec`] for CSS-like single-property transitions and
//! [`AnimationSpec`] for multi-keyframe animations. Standard presets
//! ([`fade_in`], [`slide_in`], [`scale_up`]) encode common UI motion patterns.

use std::collections::HashMap;

/// The easing function for a transition or animation.
#[derive(Clone, Debug, PartialEq)]
pub enum EasingKind {
    /// Constant velocity.
    Linear,
    /// Starts slow, ends fast.
    EaseIn,
    /// Starts fast, ends slow.
    EaseOut,
    /// Slow at both ends (most natural for UI).
    EaseInOut,
    /// Cubic Bézier — `(x1, y1, x2, y2)` control points.
    CubicBezier(f32, f32, f32, f32),
}

/// A CSS-like transition specification for a single property.
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionSpec {
    /// Total transition duration in milliseconds.
    pub duration_ms: u64,
    /// Delay before the transition starts, in milliseconds.
    pub delay_ms: u64,
    /// Easing function to apply over the transition duration.
    pub easing: EasingKind,
}

/// A single keyframe in an animation.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationKeyframe {
    /// Position within the animation (0.0 = start, 1.0 = end).
    pub offset: f32,
    /// Property name → value pairs at this keyframe (CSS-like tokens).
    pub props: HashMap<String, String>,
}

/// Fill mode for an animation (what values apply before/after the animation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    /// No fill: default values apply before and after.
    None,
    /// After the animation ends, hold the final keyframe values.
    Forwards,
    /// Before the animation starts (during `delay`), apply the first keyframe.
    Backwards,
    /// Combination of `Forwards` and `Backwards`.
    Both,
}

/// How many times an animation repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationCount {
    /// A finite number of repetitions.
    Count(u32),
    /// Loops indefinitely.
    Infinite,
}

/// A multi-keyframe animation specification.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationSpec {
    /// Ordered keyframes (should be sorted by `offset`).
    pub keyframes: Vec<AnimationKeyframe>,
    /// Total single-iteration duration in milliseconds.
    pub duration_ms: u64,
    /// Fill behaviour before and after the animation.
    pub fill_mode: FillMode,
    /// Number of times the animation plays.
    pub iteration_count: IterationCount,
}

// ── Standard presets ─────────────────────────────────────────────────────────

/// Fade-in transition: 150 ms, `ease-in-out` opacity ramp.
pub fn fade_in() -> TransitionSpec {
    TransitionSpec {
        duration_ms: 150,
        delay_ms: 0,
        easing: EasingKind::EaseInOut,
    }
}

/// Slide-in transition: 200 ms, `ease-out` transform (entering elements).
pub fn slide_in() -> TransitionSpec {
    TransitionSpec {
        duration_ms: 200,
        delay_ms: 0,
        easing: EasingKind::EaseOut,
    }
}

/// Scale-up transition: 150 ms, `ease-in-out` scale (popovers / tooltips).
pub fn scale_up() -> TransitionSpec {
    TransitionSpec {
        duration_ms: 150,
        delay_ms: 0,
        easing: EasingKind::EaseInOut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_in_preset_present() {
        let t = fade_in();
        assert!(t.duration_ms > 0, "fade_in must have a positive duration");
        assert_eq!(t.easing, EasingKind::EaseInOut);
    }

    #[test]
    fn slide_in_preset_present() {
        let t = slide_in();
        assert!(t.duration_ms > 0, "slide_in must have a positive duration");
        assert_eq!(t.easing, EasingKind::EaseOut);
    }

    #[test]
    fn scale_up_preset_present() {
        let t = scale_up();
        assert!(t.duration_ms > 0, "scale_up must have a positive duration");
        assert_eq!(t.easing, EasingKind::EaseInOut);
    }

    #[test]
    fn easing_kind_cubic_bezier_stores_values() {
        let e = EasingKind::CubicBezier(0.25, 0.1, 0.25, 1.0);
        if let EasingKind::CubicBezier(x1, y1, x2, y2) = e {
            assert_eq!(x1, 0.25);
            assert_eq!(y1, 0.1);
            assert_eq!(x2, 0.25);
            assert_eq!(y2, 1.0);
        } else {
            panic!("unexpected variant");
        }
    }

    #[test]
    fn animation_spec_builds() {
        let mut props = HashMap::new();
        props.insert("opacity".to_string(), "0".to_string());
        let spec = AnimationSpec {
            keyframes: vec![
                AnimationKeyframe {
                    offset: 0.0,
                    props: props.clone(),
                },
                AnimationKeyframe {
                    offset: 1.0,
                    props: {
                        let mut p = HashMap::new();
                        p.insert("opacity".to_string(), "1".to_string());
                        p
                    },
                },
            ],
            duration_ms: 300,
            fill_mode: FillMode::Forwards,
            iteration_count: IterationCount::Count(1),
        };
        assert_eq!(spec.keyframes.len(), 2);
        assert_eq!(spec.duration_ms, 300);
    }

    #[test]
    fn iteration_count_infinite() {
        let ic = IterationCount::Infinite;
        assert_eq!(ic, IterationCount::Infinite);
    }
}
