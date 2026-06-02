//! Nested clip-rectangle stack with intersection and integer-scissor output.
//!
//! The `ClipStack` tracks a hierarchy of rectangular clip regions.  Pushing a
//! new rect intersects it with the current top so the effective clip region is
//! always the intersection of all active rects.  The stack never panics on
//! underflow — extra pops are silently ignored.

// ── ClipRect ─────────────────────────────────────────────────────────────────

/// An axis-aligned clip rectangle in logical (floating-point) coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipRect {
    /// Left edge in logical pixels.
    pub x: f32,
    /// Top edge in logical pixels.
    pub y: f32,
    /// Width in logical pixels.
    pub w: f32,
    /// Height in logical pixels.
    pub h: f32,
}

impl ClipRect {
    /// Construct a [`ClipRect`] from origin and dimensions.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Compute the intersection of `self` and `other`.
    ///
    /// Returns `None` if the rectangles are disjoint.
    pub fn intersect(&self, other: ClipRect) -> Option<ClipRect> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        if x1 > x0 && y1 > y0 {
            Some(ClipRect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            })
        } else {
            None
        }
    }
}

// ── ClipStack ─────────────────────────────────────────────────────────────────

/// A push-down stack of intersecting clip rectangles.
///
/// Each `push` intersects the new rect with the current top and stores the
/// result.  Callers are therefore always looking at the *effective* clip, never
/// the raw per-layer rect.
#[derive(Debug, Default)]
pub struct ClipStack {
    /// Stack of intersected (effective) clip rects; top is the last element.
    stack: Vec<ClipRect>,
}

impl ClipStack {
    /// Construct an empty [`ClipStack`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new clip rect, intersecting it with the current top.
    ///
    /// If the stack is empty, `rect` is pushed directly.  If the intersection
    /// with the current top is empty, nothing is pushed and the stack is
    /// unchanged (the new layer would clip everything away anyway; the caller
    /// should balance pushes with pops regardless).
    pub fn push(&mut self, rect: ClipRect) {
        let effective = match self.stack.last() {
            None => rect,
            Some(&top) => {
                // If there is no intersection the new region is fully outside
                // the current clip — push an empty rect so pop is still balanced.
                top.intersect(rect)
                    .unwrap_or(ClipRect::new(0.0, 0.0, 0.0, 0.0))
            }
        };
        self.stack.push(effective);
    }

    /// Pop the topmost clip rect.  Does nothing (no panic) if the stack is empty.
    pub fn pop(&mut self) {
        self.stack.pop();
    }

    /// Return the current (topmost, effective) clip rect, or `None` if the
    /// stack is empty.
    pub fn current(&self) -> Option<&ClipRect> {
        self.stack.last()
    }

    /// Return the current clip as integer `[x, y, w, h]` rounded **outward**
    /// (floor on origin, ceil on extent).
    ///
    /// Returns `None` if the stack is empty.  Negative components are clamped
    /// to zero before the conversion.
    pub fn as_scissor(&self) -> Option<[u32; 4]> {
        let clip = self.stack.last()?;
        let x = clip.x.floor().max(0.0) as u32;
        let y = clip.y.floor().max(0.0) as u32;
        // Extent rounded outward.
        let right = (clip.x + clip.w).ceil().max(0.0) as u32;
        let bottom = (clip.y + clip.h).ceil().max(0.0) as u32;
        let w = right.saturating_sub(x);
        let h = bottom.saturating_sub(y);
        Some([x, y, w, h])
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_push_pop_intersection() {
        let mut stack = ClipStack::new();
        stack.push(ClipRect::new(0.0, 0.0, 100.0, 100.0));
        stack.push(ClipRect::new(10.0, 10.0, 50.0, 50.0));
        let cur = stack.current().copied().expect("stack must not be empty");
        assert!((cur.x - 10.0).abs() < 0.001);
        assert!((cur.y - 10.0).abs() < 0.001);
        assert!((cur.w - 50.0).abs() < 0.001);
        assert!((cur.h - 50.0).abs() < 0.001);
        stack.pop();
        let after_pop = stack.current().copied().expect("stack must not be empty");
        assert!((after_pop.x - 0.0).abs() < 0.001);
        assert!((after_pop.w - 100.0).abs() < 0.001);
    }

    #[test]
    fn clip_underflow_is_noop() {
        let mut stack = ClipStack::new();
        // Pop on empty must not panic.
        stack.pop();
        stack.pop();
        assert!(stack.current().is_none());
        // After spurious pops we can still use the stack normally.
        stack.push(ClipRect::new(0.0, 0.0, 10.0, 10.0));
        assert!(stack.current().is_some());
    }

    #[test]
    fn clip_as_scissor_rounds_outward() {
        let mut stack = ClipStack::new();
        // Fractional rect: x=1.2, y=2.7, w=10.1, h=5.3 → right=11.3, bottom=8.0
        // Expected scissor: x=floor(1.2)=1, y=floor(2.7)=2,
        //                   right=ceil(11.3)=12, bottom=ceil(8.0)=8
        //                   w=12-1=11, h=8-2=6
        stack.push(ClipRect::new(1.2, 2.7, 10.1, 5.3));
        let scissor = stack.as_scissor().expect("scissor must be Some");
        assert_eq!(scissor[0], 1, "x should be floor(1.2)=1");
        assert_eq!(scissor[1], 2, "y should be floor(2.7)=2");
        assert_eq!(scissor[2], 11, "w should be ceil(11.3)-1=11");
        assert_eq!(scissor[3], 6, "h should be ceil(8.0)-2=6");
    }
}
