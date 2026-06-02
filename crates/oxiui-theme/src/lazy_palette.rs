//! Lazy computation of hover / pressed / disabled colour variants.
//!
//! [`LazyPaletteVariants`] derives the three interaction-state colours from a
//! base [`Color`] on first access using [`std::cell::OnceCell`], so the
//! potentially-costly HSL conversion is paid only if the variant is actually
//! requested.

use std::cell::OnceCell;

use oxiui_core::Color;

use crate::color::{darken, desaturate, lighten};

/// Hover, pressed, and disabled colour variants derived lazily from a base colour.
///
/// Each variant is computed at most once per instance.  After the first call to
/// [`hover`](Self::hover), [`pressed`](Self::pressed), or
/// [`disabled`](Self::disabled), the result is cached and returned directly on
/// subsequent calls without repeating the colour manipulation.
///
/// # Example
/// ```rust
/// use oxiui_core::Color;
/// use oxiui_theme::LazyPaletteVariants;
///
/// let base = Color(122, 162, 247, 255); // Tokyo Night #7AA2F7
/// let variants = LazyPaletteVariants::new(base);
///
/// // Hover is lighter than base.
/// let hover = variants.hover();
/// assert!(hover.0 >= base.0 || hover.1 >= base.1 || hover.2 >= base.2);
/// ```
pub struct LazyPaletteVariants {
    /// The base colour from which all variants are derived.
    pub base: Color,
    hover: OnceCell<Color>,
    pressed: OnceCell<Color>,
    disabled: OnceCell<Color>,
}

impl LazyPaletteVariants {
    /// Create a new instance from `base`.  No variants are computed yet.
    pub fn new(base: Color) -> Self {
        Self {
            base,
            hover: OnceCell::new(),
            pressed: OnceCell::new(),
            disabled: OnceCell::new(),
        }
    }

    /// Return the hover-state colour — lighter than `base` by 10 %.
    ///
    /// Computed once and cached.
    pub fn hover(&self) -> &Color {
        self.hover.get_or_init(|| lighten(self.base, 0.1))
    }

    /// Return the pressed-state colour — darker than `base` by 10 %.
    ///
    /// Computed once and cached.
    pub fn pressed(&self) -> &Color {
        self.pressed.get_or_init(|| darken(self.base, 0.1))
    }

    /// Return the disabled-state colour — desaturated by 50 %.
    ///
    /// Computed once and cached.
    pub fn disabled(&self) -> &Color {
        self.disabled.get_or_init(|| desaturate(self.base, 0.5))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_lazy_palette_hover_is_lighter() {
        let base = Color(100, 120, 200, 255);
        let variants = LazyPaletteVariants::new(base);
        let hover = *variants.hover();
        // lighten(c, 0.1) blends toward white; at least one RGB channel must be
        // greater than or equal (none should decrease).
        assert!(
            hover.0 >= base.0 && hover.1 >= base.1 && hover.2 >= base.2,
            "hover must be lighter: base={base:?}, hover={hover:?}"
        );
    }

    #[test]
    fn test_lazy_palette_pressed_is_darker() {
        let base = Color(100, 120, 200, 255);
        let variants = LazyPaletteVariants::new(base);
        let pressed = *variants.pressed();
        // darken blends toward black; all RGB channels must be ≤ base.
        assert!(
            pressed.0 <= base.0 && pressed.1 <= base.1 && pressed.2 <= base.2,
            "pressed must be darker: base={base:?}, pressed={pressed:?}"
        );
    }

    #[test]
    fn test_lazy_palette_disabled_differs_from_base() {
        let base = Color(200, 100, 50, 255); // clearly saturated colour
        let variants = LazyPaletteVariants::new(base);
        let disabled = *variants.disabled();
        // A fully saturated colour desaturated by 50% must differ from base.
        assert_ne!(
            disabled, base,
            "disabled must differ from a saturated base colour"
        );
    }

    /// Verify that the `hover` closure is not recomputed on the second call.
    ///
    /// We cannot directly instrument the `OnceCell` closure, so we use a
    /// standalone `OnceCell<u32>` with an `AtomicUsize` counter as a proxy to
    /// confirm the once-only semantics, then verify that calling `hover()` twice
    /// on `LazyPaletteVariants` returns the identical value.
    #[test]
    fn test_lazy_palette_variant_computed_once() {
        // Standalone once-cell proxy to confirm once-only semantics.
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let cell: OnceCell<u32> = OnceCell::new();
        let _ = cell.get_or_init(|| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            42
        });
        let _ = cell.get_or_init(|| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            99
        });
        assert_eq!(
            COUNTER.load(Ordering::SeqCst),
            1,
            "OnceCell must call the init closure exactly once"
        );

        // Now confirm the LazyPaletteVariants instance returns the same &Color on both calls.
        let base = Color(100, 150, 200, 255);
        let variants = LazyPaletteVariants::new(base);
        let first: Color = *variants.hover();
        let second: Color = *variants.hover();
        assert_eq!(
            first, second,
            "hover() must return the same value on repeated calls"
        );
    }
}
