//! Spacing and decoration styling primitives.
//!
//! [`Padding`] and [`Margin`] are newtype wrappers over [`Insets`] that keep the
//! two concepts distinct at the type level (a value meant for inner padding can
//! never be silently used as an outer margin). [`Border`] couples a set of
//! [`Insets`] (the per-side border widths) with a [`Color`] and a
//! [`BorderStyle`].
//!
//! These are additive styling types consumed by [`WidgetExt`](crate::WidgetExt)
//! combinators and by adapters that honour design tokens.

use crate::geometry::{Insets, Rect};
use crate::Color;

/// Inner spacing between a widget's border box and its content.
///
/// A transparent newtype over [`Insets`]; the wrapper exists purely to prevent
/// confusing padding with [`Margin`] at call sites.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding(pub Insets);

impl Padding {
    /// Zero padding on all sides.
    pub const ZERO: Padding = Padding(Insets::ZERO);

    /// Per-side padding.
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self(Insets::new(top, right, bottom, left))
    }

    /// The same padding on all four sides.
    pub const fn all(v: f32) -> Self {
        Self(Insets::all(v))
    }

    /// Symmetric padding: `vertical` on top/bottom, `horizontal` on left/right.
    pub const fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self(Insets::symmetric(vertical, horizontal))
    }

    /// Borrow the underlying [`Insets`].
    pub const fn insets(self) -> Insets {
        self.0
    }

    /// Apply this padding inward to `rect`, yielding the content rectangle.
    pub fn shrink(self, rect: Rect) -> Rect {
        rect.deflate(self.0)
    }
}

impl From<Insets> for Padding {
    fn from(insets: Insets) -> Self {
        Self(insets)
    }
}

impl From<Padding> for Insets {
    fn from(padding: Padding) -> Self {
        padding.0
    }
}

/// Outer spacing between a widget's margin box and its siblings.
///
/// A transparent newtype over [`Insets`]; distinct from [`Padding`] by type.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Margin(pub Insets);

impl Margin {
    /// Zero margin on all sides.
    pub const ZERO: Margin = Margin(Insets::ZERO);

    /// Per-side margin.
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self(Insets::new(top, right, bottom, left))
    }

    /// The same margin on all four sides.
    pub const fn all(v: f32) -> Self {
        Self(Insets::all(v))
    }

    /// Symmetric margin: `vertical` on top/bottom, `horizontal` on left/right.
    pub const fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self(Insets::symmetric(vertical, horizontal))
    }

    /// Borrow the underlying [`Insets`].
    pub const fn insets(self) -> Insets {
        self.0
    }

    /// Apply this margin outward to `rect`, yielding the margin box.
    pub fn grow(self, rect: Rect) -> Rect {
        rect.inflate(self.0)
    }
}

impl From<Insets> for Margin {
    fn from(insets: Insets) -> Self {
        Self(insets)
    }
}

impl From<Margin> for Insets {
    fn from(margin: Margin) -> Self {
        margin.0
    }
}

/// How a [`Border`] is rendered along its edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BorderStyle {
    /// A continuous solid line.
    #[default]
    Solid,
    /// A series of dashes.
    Dashed,
    /// A series of dots.
    Dotted,
    /// Two parallel solid lines.
    Double,
    /// No visible border (still occupies its width for layout).
    None,
}

/// A widget border: per-side widths, a colour, and a line style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    /// Per-side border widths (logical pixels).
    pub insets: Insets,
    /// Border colour.
    pub color: Color,
    /// Line style.
    pub style: BorderStyle,
}

impl Border {
    /// A uniform-width solid border in `color`.
    pub fn solid(width: f32, color: Color) -> Self {
        Self {
            insets: Insets::all(width),
            color,
            style: BorderStyle::Solid,
        }
    }

    /// A border with explicit per-side widths.
    pub fn new(insets: Insets, color: Color, style: BorderStyle) -> Self {
        Self {
            insets,
            color,
            style,
        }
    }

    /// Builder: replace the line style.
    pub fn with_style(mut self, style: BorderStyle) -> Self {
        self.style = style;
        self
    }

    /// Builder: replace the colour.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Returns `true` if the border occupies no layout space or is invisible.
    pub fn is_none(&self) -> bool {
        self.style == BorderStyle::None
            || (self.insets.top <= 0.0
                && self.insets.right <= 0.0
                && self.insets.bottom <= 0.0
                && self.insets.left <= 0.0)
    }

    /// The content rectangle after subtracting the border widths from `rect`.
    pub fn content_rect(&self, rect: Rect) -> Rect {
        rect.deflate(self.insets)
    }
}

impl Default for Border {
    /// A zero-width transparent solid border (effectively no border).
    fn default() -> Self {
        Self {
            insets: Insets::ZERO,
            color: Color(0, 0, 0, 0),
            style: BorderStyle::None,
        }
    }
}

/// The shape the OS cursor should take while over a widget.
///
/// Mirrors the common CSS/`winit` cursor set. Adapters map these onto their
/// platform cursor enums; unknown shapes fall back to [`CursorShape::Default`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CursorShape {
    /// The platform default arrow.
    #[default]
    Default,
    /// A pointing hand (links / clickable affordances).
    Pointer,
    /// An I-beam for editable text.
    Text,
    /// A crosshair for precision selection.
    Crosshair,
    /// A "move" four-way arrow.
    Move,
    /// A "not allowed" indicator.
    NotAllowed,
    /// A spinning / busy "wait" cursor.
    Wait,
    /// A progress cursor (busy but still interactive).
    Progress,
    /// An open grab hand.
    Grab,
    /// A closed grabbing hand.
    Grabbing,
    /// Horizontal resize (east-west).
    ResizeEw,
    /// Vertical resize (north-south).
    ResizeNs,
    /// Diagonal resize (north-east / south-west).
    ResizeNesw,
    /// Diagonal resize (north-west / south-east).
    ResizeNwse,
    /// The cursor is hidden.
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_shrink_matches_deflate() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        let p = Padding::all(10.0);
        assert_eq!(p.shrink(r), r.deflate(Insets::all(10.0)));
        assert_eq!(p.insets(), Insets::all(10.0));
    }

    #[test]
    fn margin_grow_matches_inflate() {
        let r = Rect::new(10.0, 10.0, 50.0, 50.0);
        let m = Margin::symmetric(4.0, 8.0);
        assert_eq!(m.grow(r), r.inflate(Insets::symmetric(4.0, 8.0)));
    }

    #[test]
    fn padding_margin_conversions() {
        let i = Insets::new(1.0, 2.0, 3.0, 4.0);
        let p: Padding = i.into();
        let back: Insets = p.into();
        assert_eq!(back, i);
        let m: Margin = i.into();
        assert_eq!(Insets::from(m), i);
    }

    #[test]
    fn border_solid_and_content_rect() {
        let b = Border::solid(2.0, Color(255, 0, 0, 255));
        assert_eq!(b.style, BorderStyle::Solid);
        assert_eq!(b.insets, Insets::all(2.0));
        let content = b.content_rect(Rect::new(0.0, 0.0, 20.0, 20.0));
        assert_eq!(content, Rect::new(2.0, 2.0, 16.0, 16.0));
        assert!(!b.is_none());
    }

    #[test]
    fn border_default_is_none() {
        let b = Border::default();
        assert!(b.is_none());
        assert_eq!(b.style, BorderStyle::None);
        // An explicit None style is also "none" even with width.
        let styled = Border::solid(3.0, Color(0, 0, 0, 255)).with_style(BorderStyle::None);
        assert!(styled.is_none());
    }

    #[test]
    fn cursor_shape_default() {
        assert_eq!(CursorShape::default(), CursorShape::Default);
    }
}
