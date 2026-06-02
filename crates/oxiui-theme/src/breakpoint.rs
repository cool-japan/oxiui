//! Responsive breakpoints matching common CSS media-query thresholds.
//!
//! The breakpoints follow the Bootstrap 5 / Tailwind CSS convention:
//!
//! | Variant | Viewport width         |
//! |---------|------------------------|
//! | `Xs`    | < 576 px               |
//! | `Sm`    | 576 – 767 px           |
//! | `Md`    | 768 – 991 px           |
//! | `Lg`    | 992 – 1199 px          |
//! | `Xl`    | 1200 – 1535 px         |
//! | `Xxl`   | ≥ 1536 px              |

/// A responsive breakpoint corresponding to a viewport-width range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Breakpoint {
    /// Extra-small: viewport width below 576 logical pixels.
    Xs,
    /// Small: viewport width from 576 to 767 logical pixels.
    Sm,
    /// Medium: viewport width from 768 to 991 logical pixels.
    Md,
    /// Large: viewport width from 992 to 1199 logical pixels.
    Lg,
    /// Extra-large: viewport width from 1200 to 1535 logical pixels.
    Xl,
    /// Extra-extra-large: viewport width at or above 1536 logical pixels.
    Xxl,
}

impl Breakpoint {
    /// Determine the active breakpoint for a given viewport `width` in logical
    /// pixels.
    pub fn for_width(width: f32) -> Self {
        if width < 576.0 {
            Breakpoint::Xs
        } else if width < 768.0 {
            Breakpoint::Sm
        } else if width < 992.0 {
            Breakpoint::Md
        } else if width < 1200.0 {
            Breakpoint::Lg
        } else if width < 1536.0 {
            Breakpoint::Xl
        } else {
            Breakpoint::Xxl
        }
    }

    /// The minimum viewport width (in logical pixels) at which this breakpoint
    /// activates.
    pub fn min_width(self) -> f32 {
        match self {
            Breakpoint::Xs => 0.0,
            Breakpoint::Sm => 576.0,
            Breakpoint::Md => 768.0,
            Breakpoint::Lg => 992.0,
            Breakpoint::Xl => 1200.0,
            Breakpoint::Xxl => 1536.0,
        }
    }

    /// Returns `true` when `width` is at or above this breakpoint's minimum.
    ///
    /// Equivalent to a CSS `@media (min-width: …)` query.
    pub fn matches_width(self, width: f32) -> bool {
        width >= self.min_width()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoint_xs_below_576() {
        assert_eq!(Breakpoint::for_width(400.0), Breakpoint::Xs);
        assert_eq!(Breakpoint::for_width(0.0), Breakpoint::Xs);
    }

    #[test]
    fn breakpoint_sm_at_576() {
        assert_eq!(Breakpoint::for_width(576.0), Breakpoint::Sm);
        assert_eq!(Breakpoint::for_width(767.9), Breakpoint::Sm);
    }

    #[test]
    fn breakpoint_md_768_to_991() {
        assert_eq!(Breakpoint::for_width(900.0), Breakpoint::Md);
    }

    #[test]
    fn breakpoint_xxl_at_1536() {
        assert_eq!(Breakpoint::for_width(1536.0), Breakpoint::Xxl);
        assert_eq!(Breakpoint::for_width(2000.0), Breakpoint::Xxl);
    }

    #[test]
    fn breakpoint_ordering() {
        assert!(Breakpoint::Xs < Breakpoint::Sm);
        assert!(Breakpoint::Sm < Breakpoint::Md);
        assert!(Breakpoint::Md < Breakpoint::Lg);
        assert!(Breakpoint::Lg < Breakpoint::Xl);
        assert!(Breakpoint::Xl < Breakpoint::Xxl);
    }

    #[test]
    fn min_width_xs_is_zero() {
        assert_eq!(Breakpoint::Xs.min_width(), 0.0);
    }

    #[test]
    fn matches_width_true_and_false() {
        assert!(Breakpoint::Md.matches_width(900.0));
        assert!(!Breakpoint::Xl.matches_width(900.0));
    }
}
