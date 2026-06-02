//! Rectangular clip-region stack.
//!
//! Clip rectangles use integer pixel bounds `[x0, x1) × [y0, y1)`. Pushing a
//! new clip intersects it with the current clip, so nested clips never expand
//! the drawable region.

/// An integer-bounded clip rectangle: `[x0, x1) × [y0, y1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRect {
    /// Inclusive left bound.
    pub x0: i64,
    /// Inclusive top bound.
    pub y0: i64,
    /// Exclusive right bound.
    pub x1: i64,
    /// Exclusive bottom bound.
    pub y1: i64,
}

impl ClipRect {
    /// A clip covering the whole framebuffer `[0, width) × [0, height)`.
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: width as i64,
            y1: height as i64,
        }
    }

    /// Construct from a position and size (negative sizes yield an empty clip).
    pub fn from_rect(x: i64, y: i64, w: i64, h: i64) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + w.max(0),
            y1: y + h.max(0),
        }
    }

    /// Returns `true` if the clip has zero or negative area.
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    /// Intersect with `other`, returning the overlapping clip.
    pub fn intersect(&self, other: &ClipRect) -> ClipRect {
        ClipRect {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }

    /// Returns `true` if pixel `(x, y)` lies within the clip.
    pub fn contains(&self, x: i64, y: i64) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

/// A stack of nested clip rectangles.
///
/// The effective clip is always the intersection of every pushed rectangle,
/// maintained incrementally as [`current`](ClipStack::current).
#[derive(Clone, Debug)]
pub struct ClipStack {
    stack: Vec<ClipRect>,
}

impl ClipStack {
    /// Create a stack whose base clip covers the whole framebuffer.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            stack: vec![ClipRect::full(width, height)],
        }
    }

    /// The current effective clip (intersection of all pushed rects).
    pub fn current(&self) -> ClipRect {
        // The base is always present; `last` is the running intersection.
        *self.stack.last().unwrap_or(&ClipRect {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
        })
    }

    /// Push `clip`, intersecting it with the current effective clip.
    pub fn push(&mut self, clip: ClipRect) {
        let next = self.current().intersect(&clip);
        self.stack.push(next);
    }

    /// Pop the most recently pushed clip. The base clip is never popped.
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Number of clips on the stack (including the base).
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_clips_intersect() {
        let mut s = ClipStack::new(100, 100);
        assert_eq!(s.current(), ClipRect::full(100, 100));
        s.push(ClipRect::from_rect(10, 10, 50, 50)); // [10,60) x [10,60)
        s.push(ClipRect::from_rect(40, 40, 50, 50)); // [40,90) x [40,90)
        let c = s.current();
        assert_eq!(
            c,
            ClipRect {
                x0: 40,
                y0: 40,
                x1: 60,
                y1: 60
            }
        );
        s.pop();
        assert_eq!(
            s.current(),
            ClipRect {
                x0: 10,
                y0: 10,
                x1: 60,
                y1: 60
            }
        );
    }

    #[test]
    fn base_clip_never_popped() {
        let mut s = ClipStack::new(10, 10);
        s.pop();
        s.pop();
        assert_eq!(s.depth(), 1);
        assert_eq!(s.current(), ClipRect::full(10, 10));
    }

    #[test]
    fn contains_and_empty() {
        let c = ClipRect::from_rect(0, 0, 5, 5);
        assert!(c.contains(0, 0));
        assert!(c.contains(4, 4));
        assert!(!c.contains(5, 5));
        assert!(ClipRect::from_rect(0, 0, 0, 5).is_empty());
        let disjoint =
            ClipRect::from_rect(0, 0, 5, 5).intersect(&ClipRect::from_rect(10, 10, 5, 5));
        assert!(disjoint.is_empty());
    }
}
