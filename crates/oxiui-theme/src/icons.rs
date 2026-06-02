//! Icon theme trait and built-in icon set with hand-authored SVG path data.
//!
//! No SVG parsing dependency is used.  All path-data strings are `'static`
//! byte string constants embedded at compile time.  Renderers receive the raw
//! SVG `d`-attribute string and are responsible for stroking / filling it
//! according to the requested variant.

// ── Public types ───────────────────────────────────────────────────────────────

/// Drawing variant for an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconVariant {
    /// Outline (stroke) style.
    Outline,
    /// Filled (filled-path) style.
    Filled,
    /// Rounded stroke style (may alias `Outline` in the built-in set).
    Rounded,
}

/// Logical icon name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconName {
    /// Close / dismiss (X mark).
    Close,
    /// Hamburger menu (three horizontal lines).
    Menu,
    /// Arrow pointing right.
    ArrowRight,
    /// Arrow pointing left.
    ArrowLeft,
    /// Arrow pointing up.
    ArrowUp,
    /// Arrow pointing down.
    ArrowDown,
    /// Check / tick mark.
    Check,
    /// Magnifying-glass / search.
    Search,
}

/// A source of SVG path-data strings for named icons.
///
/// Implementations return the SVG `d` attribute value for the given icon,
/// variant, and size.  If a combination is unsupported, `None` is returned and
/// callers should fall back to a default or skip rendering.
pub trait IconSet: Send + Sync {
    /// Return the SVG path-data string (the `d` attribute) for the icon.
    ///
    /// Returns `None` when the implementation does not support the requested
    /// combination of icon, variant, and size.
    fn path_data(&self, icon: IconName, variant: IconVariant, size: u32) -> Option<&'static str>;
}

// ── Built-in icon set ──────────────────────────────────────────────────────────

/// Built-in icon set with hand-authored SVG path strings.
///
/// Supports sizes 16, 20, 24, 32.  `Rounded` aliases `Outline` for all icons.
pub struct BuiltinIcons;

impl BuiltinIcons {
    /// Create a new instance of the built-in icon set.
    pub fn new() -> Self {
        Self
    }
}

impl Default for BuiltinIcons {
    fn default() -> Self {
        Self::new()
    }
}

impl IconSet for BuiltinIcons {
    fn path_data(&self, icon: IconName, variant: IconVariant, size: u32) -> Option<&'static str> {
        match (icon, variant, size) {
            // ── Close (X) ─────────────────────────────────────────────────────
            (IconName::Close, IconVariant::Outline | IconVariant::Rounded, 16) => {
                Some("M3 3L13 13M13 3L3 13")
            }
            (IconName::Close, IconVariant::Outline | IconVariant::Rounded, 20) => {
                Some("M4 4L16 16M16 4L4 16")
            }
            (IconName::Close, IconVariant::Outline | IconVariant::Rounded, 24) => {
                Some("M5 5L19 19M19 5L5 19")
            }
            (IconName::Close, IconVariant::Outline | IconVariant::Rounded, 32) => {
                Some("M6 6L26 26M26 6L6 26")
            }
            (IconName::Close, IconVariant::Filled, 16) => Some("M3 3L13 13M13 3L3 13"),
            (IconName::Close, IconVariant::Filled, 20) => Some("M4 4L16 16M16 4L4 16"),
            (IconName::Close, IconVariant::Filled, 24) => Some("M5 5L19 19M19 5L5 19"),
            (IconName::Close, IconVariant::Filled, 32) => Some("M6 6L26 26M26 6L6 26"),

            // ── Menu (hamburger) ──────────────────────────────────────────────
            (IconName::Menu, _, 16) => Some("M2 5H14M2 8H14M2 11H14"),
            (IconName::Menu, _, 20) => Some("M3 6H17M3 10H17M3 14H17"),
            (IconName::Menu, _, 24) => Some("M4 7H20M4 12H20M4 17H20"),
            (IconName::Menu, _, 32) => Some("M5 9H27M5 16H27M5 23H27"),

            // ── Arrow Right ───────────────────────────────────────────────────
            (IconName::ArrowRight, IconVariant::Outline | IconVariant::Rounded, 16) => {
                Some("M4 8H12M9 5L12 8L9 11")
            }
            (IconName::ArrowRight, _, 20) => Some("M5 10H15M11 6L15 10L11 14"),
            (IconName::ArrowRight, _, 24) => Some("M6 12H18M13 7L18 12L13 17"),
            (IconName::ArrowRight, _, 32) => Some("M8 16H24M17 9L24 16L17 23"),
            (IconName::ArrowRight, IconVariant::Filled, 16) => Some("M4 8H12M9 5L12 8L9 11"),

            // ── Arrow Left ────────────────────────────────────────────────────
            (IconName::ArrowLeft, IconVariant::Outline | IconVariant::Rounded, 16) => {
                Some("M12 8H4M7 5L4 8L7 11")
            }
            (IconName::ArrowLeft, _, 20) => Some("M15 10H5M9 6L5 10L9 14"),
            (IconName::ArrowLeft, _, 24) => Some("M18 12H6M11 7L6 12L11 17"),
            (IconName::ArrowLeft, _, 32) => Some("M24 16H8M15 9L8 16L15 23"),
            (IconName::ArrowLeft, IconVariant::Filled, 16) => Some("M12 8H4M7 5L4 8L7 11"),

            // ── Arrow Up ──────────────────────────────────────────────────────
            (IconName::ArrowUp, _, 16) => Some("M8 12V4M5 7L8 4L11 7"),
            (IconName::ArrowUp, _, 20) => Some("M10 15V5M6 9L10 5L14 9"),
            (IconName::ArrowUp, _, 24) => Some("M12 19V5M7 10L12 5L17 10"),
            (IconName::ArrowUp, _, 32) => Some("M16 26V6M9 13L16 6L23 13"),

            // ── Arrow Down ────────────────────────────────────────────────────
            (IconName::ArrowDown, _, 16) => Some("M8 4V12M5 9L8 12L11 9"),
            (IconName::ArrowDown, _, 20) => Some("M10 5V15M6 11L10 15L14 11"),
            (IconName::ArrowDown, _, 24) => Some("M12 5V19M7 14L12 19L17 14"),
            (IconName::ArrowDown, _, 32) => Some("M16 6V26M9 19L16 26L23 19"),

            // ── Check ─────────────────────────────────────────────────────────
            (IconName::Check, _, 16) => Some("M2 8L6 12L14 4"),
            (IconName::Check, _, 20) => Some("M3 10L8 15L17 5"),
            (IconName::Check, _, 24) => Some("M4 12L9 18L20 6"),
            (IconName::Check, _, 32) => Some("M5 16L12 24L27 8"),

            // ── Search (magnifying glass) ─────────────────────────────────────
            (IconName::Search, _, 16) => Some("M7 7m-4 0a4 4 0 1 0 8 0a4 4 0 1 0-8 0M11 11L14 14"),
            (IconName::Search, _, 20) => {
                Some("M9 9m-5 0a5 5 0 1 0 10 0a5 5 0 1 0-10 0M14 14L18 18")
            }
            (IconName::Search, _, 24) => {
                Some("M11 11m-6 0a6 6 0 1 0 12 0a6 6 0 1 0-12 0M17 17L21 21")
            }
            (IconName::Search, _, 32) => {
                Some("M14 14m-8 0a8 8 0 1 0 16 0a8 8 0 1 0-16 0M22 22L28 28")
            }

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_all_sizes_outline() {
        let icons = BuiltinIcons::new();
        for size in [16u32, 20, 24, 32] {
            assert!(
                icons
                    .path_data(IconName::Close, IconVariant::Outline, size)
                    .is_some(),
                "Close outline missing for size {size}"
            );
        }
    }

    #[test]
    fn close_all_sizes_filled() {
        let icons = BuiltinIcons::new();
        for size in [16u32, 20, 24, 32] {
            assert!(
                icons
                    .path_data(IconName::Close, IconVariant::Filled, size)
                    .is_some(),
                "Close filled missing for size {size}"
            );
        }
    }

    #[test]
    fn all_icons_have_24px_outline() {
        let icons = BuiltinIcons::new();
        let all = [
            IconName::Close,
            IconName::Menu,
            IconName::ArrowRight,
            IconName::ArrowLeft,
            IconName::ArrowUp,
            IconName::ArrowDown,
            IconName::Check,
            IconName::Search,
        ];
        for icon in all {
            assert!(
                icons.path_data(icon, IconVariant::Outline, 24).is_some(),
                "{icon:?} missing 24px outline"
            );
        }
    }

    #[test]
    fn rounded_aliases_outline_for_close() {
        let icons = BuiltinIcons::new();
        let outline = icons.path_data(IconName::Close, IconVariant::Outline, 24);
        let rounded = icons.path_data(IconName::Close, IconVariant::Rounded, 24);
        assert_eq!(outline, rounded);
    }

    #[test]
    fn unknown_size_returns_none() {
        let icons = BuiltinIcons::new();
        assert!(icons
            .path_data(IconName::Close, IconVariant::Outline, 48)
            .is_none());
    }
}
