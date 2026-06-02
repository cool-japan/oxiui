//! CSS-style property inheritance for computed styles.
//!
//! In CSS, some properties are inherited from parent to child by default
//! (notably `color`, `font-size`, and `font-weight`) while others are not
//! (spacing, border, opacity).  This module provides [`resolve`] which applies
//! those rules between a parent and child [`ComputedStyle`].

use crate::stylesheet::{ComputedStyle, CssValue};

/// Resolve a child style against its parent by applying CSS inheritance rules.
///
/// **Inheritable** properties (`color`, `font-size`, `font-weight`): if the
/// child's value is `None` or explicitly `inherit`, the parent's value is
/// copied in.  If the child sets `initial` or `unset`, the property is cleared
/// back to `None`.
///
/// **Non-inheritable** properties (`background-color`, `padding`, `margin`,
/// `border-*`, `opacity`): these are left as-is on the child; the parent's
/// values are ignored.
pub fn resolve(parent: &ComputedStyle, child: &mut ComputedStyle) {
    // ── color (inheritable) ───────────────────────────────────────────────────
    if matches!(child.color, Some(CssValue::Inherit)) || child.color.is_none() {
        child.color = parent.color.clone();
    }
    if matches!(child.color, Some(CssValue::Initial) | Some(CssValue::Unset)) {
        child.color = None;
    }

    // ── font-size (inheritable) ───────────────────────────────────────────────
    if child.font_size.is_none() {
        child.font_size = parent.font_size;
    }

    // ── font-weight (inheritable) ─────────────────────────────────────────────
    if child.font_weight.is_none() {
        child.font_weight = parent.font_weight;
    }

    // background-color, padding, margin, border-*, opacity: NOT inherited.
    // They stay as-is on the child.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stylesheet::CssValue;
    use oxiui_core::Color;

    fn color_val(r: u8, g: u8, b: u8) -> CssValue {
        CssValue::Color(Color(r, g, b, 255))
    }

    #[test]
    fn color_flows_from_parent_when_child_has_none() {
        let parent = ComputedStyle {
            color: Some(color_val(255, 0, 0)),
            ..Default::default()
        };
        let mut child = ComputedStyle::default();
        resolve(&parent, &mut child);
        assert_eq!(child.color, Some(color_val(255, 0, 0)));
    }

    #[test]
    fn color_inherit_keyword_copies_parent() {
        let parent = ComputedStyle {
            color: Some(color_val(0, 255, 0)),
            ..Default::default()
        };
        let mut child = ComputedStyle {
            color: Some(CssValue::Inherit),
            ..Default::default()
        };
        resolve(&parent, &mut child);
        assert_eq!(child.color, Some(color_val(0, 255, 0)));
    }

    #[test]
    fn color_initial_clears_to_none() {
        let parent = ComputedStyle {
            color: Some(color_val(0, 0, 255)),
            ..Default::default()
        };
        let mut child = ComputedStyle {
            color: Some(CssValue::Initial),
            ..Default::default()
        };
        resolve(&parent, &mut child);
        assert!(child.color.is_none());
    }

    #[test]
    fn padding_not_inherited() {
        let parent = ComputedStyle {
            padding: Some(16.0),
            ..Default::default()
        };
        let mut child = ComputedStyle::default();
        resolve(&parent, &mut child);
        assert!(child.padding.is_none(), "padding must not be inherited");
    }

    #[test]
    fn font_size_flows_from_parent() {
        let parent = ComputedStyle {
            font_size: Some(18.0),
            ..Default::default()
        };
        let mut child = ComputedStyle::default();
        resolve(&parent, &mut child);
        assert_eq!(child.font_size, Some(18.0));
    }

    #[test]
    fn font_weight_flows_from_parent() {
        let parent = ComputedStyle {
            font_weight: Some(700.0),
            ..Default::default()
        };
        let mut child = ComputedStyle::default();
        resolve(&parent, &mut child);
        assert_eq!(child.font_weight, Some(700.0));
    }

    #[test]
    fn own_font_size_not_overridden_by_parent() {
        let parent = ComputedStyle {
            font_size: Some(18.0),
            ..Default::default()
        };
        let mut child = ComputedStyle {
            font_size: Some(12.0),
            ..Default::default()
        };
        resolve(&parent, &mut child);
        assert_eq!(child.font_size, Some(12.0));
    }
}
