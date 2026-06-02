//! 2-D geometry primitives used across the OxiUI stack.
//!
//! All coordinates are `f32` logical pixels with the origin at the top-left,
//! `x` increasing rightward and `y` increasing downward.

use core::ops::{Add, Mul, Sub};

/// A 2-D point in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

impl Point {
    /// The origin point `(0, 0)`.
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    /// Construct a [`Point`] from explicit coordinates.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to `other`.
    pub fn distance(self, other: Point) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// A 2-D size (width × height) in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Width in logical pixels (non-negative by convention).
    pub width: f32,
    /// Height in logical pixels (non-negative by convention).
    pub height: f32,
}

impl Size {
    /// A zero-area size.
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    /// Construct a [`Size`] from explicit dimensions.
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Returns the area (`width * height`).
    pub fn area(self) -> f32 {
        self.width * self.height
    }

    /// Returns `true` if either dimension is zero or negative.
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Clamp both dimensions into `[min, max]` component-wise.
    pub fn clamp(self, min: Size, max: Size) -> Size {
        Size::new(
            self.width.clamp(min.width, max.width),
            self.height.clamp(min.height, max.height),
        )
    }
}

impl Mul<f32> for Size {
    type Output = Size;
    fn mul(self, rhs: f32) -> Size {
        Size::new(self.width * rhs, self.height * rhs)
    }
}

/// Per-side insets (padding or margin) in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Top inset.
    pub top: f32,
    /// Right inset.
    pub right: f32,
    /// Bottom inset.
    pub bottom: f32,
    /// Left inset.
    pub left: f32,
}

impl Insets {
    /// Zero on all sides.
    pub const ZERO: Insets = Insets {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    /// Construct per-side insets.
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// The same inset on all four sides.
    pub const fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// Symmetric insets: `vertical` on top/bottom, `horizontal` on left/right.
    pub const fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Total horizontal inset (`left + right`).
    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    /// Total vertical inset (`top + bottom`).
    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

/// An axis-aligned rectangle defined by its top-left [`Point`] and [`Size`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Top-left corner.
    pub origin: Point,
    /// Width and height.
    pub size: Size,
}

impl Rect {
    /// A zero rectangle at the origin.
    pub const ZERO: Rect = Rect {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    /// Construct a rectangle from explicit `x`, `y`, `width`, `height`.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    /// Construct a rectangle from an [`origin`](Rect::origin) and [`size`](Rect::size).
    pub const fn from_origin_size(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// Left edge (`origin.x`).
    pub fn left(&self) -> f32 {
        self.origin.x
    }

    /// Top edge (`origin.y`).
    pub fn top(&self) -> f32 {
        self.origin.y
    }

    /// Right edge (`origin.x + width`).
    pub fn right(&self) -> f32 {
        self.origin.x + self.size.width
    }

    /// Bottom edge (`origin.y + height`).
    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.height
    }

    /// Width in logical pixels.
    pub fn width(&self) -> f32 {
        self.size.width
    }

    /// Height in logical pixels.
    pub fn height(&self) -> f32 {
        self.size.height
    }

    /// Geometric centre of the rectangle.
    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width * 0.5,
            self.origin.y + self.size.height * 0.5,
        )
    }

    /// Returns `true` if `p` lies within the rectangle (inclusive of the
    /// top/left edges, exclusive of the bottom/right edges).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.left() && p.x < self.right() && p.y >= self.top() && p.y < self.bottom()
    }

    /// Returns `true` if this rectangle shares any interior area with `other`.
    pub fn intersects(&self, other: &Rect) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    /// Returns the overlapping rectangle, or `None` if the two do not overlap.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right > left && bottom > top {
            Some(Rect::new(left, top, right - left, bottom - top))
        } else {
            None
        }
    }

    /// Returns the smallest rectangle enclosing both `self` and `other`.
    pub fn union(&self, other: &Rect) -> Rect {
        let left = self.left().min(other.left());
        let top = self.top().min(other.top());
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Rect::new(left, top, right - left, bottom - top)
    }

    /// Shrink the rectangle inward by `insets` (clamped to non-negative size).
    pub fn deflate(&self, insets: Insets) -> Rect {
        let w = (self.size.width - insets.horizontal()).max(0.0);
        let h = (self.size.height - insets.vertical()).max(0.0);
        Rect::new(self.left() + insets.left, self.top() + insets.top, w, h)
    }

    /// Grow the rectangle outward by `insets`.
    pub fn inflate(&self, insets: Insets) -> Rect {
        Rect::new(
            self.left() - insets.left,
            self.top() - insets.top,
            self.size.width + insets.horizontal(),
            self.size.height + insets.vertical(),
        )
    }

    /// Returns `true` if width or height is zero or negative.
    pub fn is_empty(&self) -> bool {
        self.size.is_empty()
    }
}

/// Box layout constraints: a `[min, max]` range for width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    /// Minimum allowed size.
    pub min: Size,
    /// Maximum allowed size (use [`f32::INFINITY`] for unbounded).
    pub max: Size,
}

impl Constraints {
    /// Construct constraints from a min and max size.
    pub const fn new(min: Size, max: Size) -> Self {
        Self { min, max }
    }

    /// Tight constraints that force exactly `size`.
    pub fn tight(size: Size) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    /// Loose constraints: `min` is zero, `max` is the given size.
    pub fn loose(max: Size) -> Self {
        Self {
            min: Size::ZERO,
            max,
        }
    }

    /// Unbounded constraints (zero min, infinite max).
    pub fn unbounded() -> Self {
        Self {
            min: Size::ZERO,
            max: Size::new(f32::INFINITY, f32::INFINITY),
        }
    }

    /// Clamp `size` into this constraint range.
    pub fn constrain(&self, size: Size) -> Size {
        size.clamp(self.min, self.max)
    }

    /// Returns `true` if only one size satisfies both width and height bounds.
    pub fn is_tight(&self) -> bool {
        self.min.width == self.max.width && self.min.height == self.max.height
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self::unbounded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_and_edges() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.right(), 110.0);
        assert_eq!(r.bottom(), 70.0);
        assert!(r.contains(Point::new(10.0, 20.0)));
        assert!(r.contains(Point::new(109.9, 69.9)));
        assert!(!r.contains(Point::new(110.0, 70.0))); // exclusive on far edges
        assert!(!r.contains(Point::new(9.9, 20.0)));
        assert_eq!(r.center(), Point::new(60.0, 45.0));
    }

    #[test]
    fn rect_intersection_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert!(a.intersects(&b));
        let i = a.intersection(&b).expect("rects overlap");
        assert_eq!(i, Rect::new(5.0, 5.0, 5.0, 5.0));
        let u = a.union(&b);
        assert_eq!(u, Rect::new(0.0, 0.0, 15.0, 15.0));

        let c = Rect::new(100.0, 100.0, 5.0, 5.0);
        assert!(!a.intersects(&c));
        assert!(a.intersection(&c).is_none());
    }

    #[test]
    fn rect_deflate_inflate() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        let d = r.deflate(Insets::all(10.0));
        assert_eq!(d, Rect::new(10.0, 10.0, 80.0, 80.0));
        let i = d.inflate(Insets::all(10.0));
        assert_eq!(i, r);

        // Deflate beyond size clamps to zero, never negative.
        let tiny = Rect::new(0.0, 0.0, 5.0, 5.0);
        let clamped = tiny.deflate(Insets::all(10.0));
        assert_eq!(clamped.width(), 0.0);
        assert_eq!(clamped.height(), 0.0);
    }

    #[test]
    fn constraints_constrain() {
        let c = Constraints::new(Size::new(10.0, 10.0), Size::new(100.0, 100.0));
        assert_eq!(c.constrain(Size::new(5.0, 200.0)), Size::new(10.0, 100.0));
        assert!(Constraints::tight(Size::new(50.0, 50.0)).is_tight());
        assert!(!c.is_tight());
    }

    #[test]
    fn point_arithmetic() {
        let p = Point::new(3.0, 4.0) + Point::new(1.0, 1.0);
        assert_eq!(p, Point::new(4.0, 5.0));
        assert_eq!((p - Point::new(4.0, 5.0)), Point::ZERO);
        assert_eq!(Point::ZERO.distance(Point::new(3.0, 4.0)), 5.0);
    }

    #[test]
    fn insets_totals() {
        let i = Insets::symmetric(8.0, 16.0);
        assert_eq!(i.vertical(), 16.0);
        assert_eq!(i.horizontal(), 32.0);
    }
}
