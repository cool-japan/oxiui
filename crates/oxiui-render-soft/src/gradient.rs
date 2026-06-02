//! Linear and radial gradient fills with multiple colour stops (sRGB interpolation).

use crate::clip::ClipRect;
use crate::framebuffer::{pack_rgba, Framebuffer};
use oxiui_core::Color;

/// A single gradient colour stop at normalized `offset` in `[0, 1]`.
#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    /// Position along the gradient axis, `0.0` = start, `1.0` = end.
    pub offset: f32,
    /// Colour at this offset.
    pub color: Color,
}

impl GradientStop {
    /// Construct a stop.
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

/// A linear gradient defined by an axis (`start` → `end`) and colour stops.
#[derive(Clone, Debug)]
pub struct LinearGradient {
    /// Axis start point (logical pixels).
    pub start: (f32, f32),
    /// Axis end point (logical pixels).
    pub end: (f32, f32),
    /// Colour stops, sorted by ascending `offset`.
    pub stops: Vec<GradientStop>,
}

impl LinearGradient {
    /// Construct a two-stop gradient from `from` at `start` to `to` at `end`.
    pub fn two_stop(start: (f32, f32), end: (f32, f32), from: Color, to: Color) -> Self {
        Self {
            start,
            end,
            stops: vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
        }
    }

    /// Construct from arbitrary stops; they are sorted by offset internally.
    pub fn new(start: (f32, f32), end: (f32, f32), mut stops: Vec<GradientStop>) -> Self {
        stops.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        Self { start, end, stops }
    }

    /// Sample the gradient colour at normalized position `t` in `[0, 1]`.
    pub fn sample(&self, t: f32) -> Color {
        if self.stops.is_empty() {
            return Color(0, 0, 0, 0);
        }
        let t = t.clamp(0.0, 1.0);
        // Before first / after last stop.
        if t <= self.stops[0].offset {
            return self.stops[0].color;
        }
        let last = self.stops.len() - 1;
        if t >= self.stops[last].offset {
            return self.stops[last].color;
        }
        // Find the bracketing pair.
        for w in self.stops.windows(2) {
            let a = &w[0];
            let b = &w[1];
            if t >= a.offset && t <= b.offset {
                let span = (b.offset - a.offset).max(f32::EPSILON);
                let local = (t - a.offset) / span;
                return lerp_color(&a.color, &b.color, local);
            }
        }
        self.stops[last].color
    }

    /// Fill the rectangle `[x, x+w) × [y, y+h)` of `fb` with this gradient,
    /// clipped to `clip`. Each pixel is projected onto the gradient axis.
    pub fn fill_rect(&self, fb: &mut Framebuffer, clip: &ClipRect, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let (sx, sy) = self.start;
        let (ex, ey) = self.end;
        let axis_x = ex - sx;
        let axis_y = ey - sy;
        let len_sq = axis_x * axis_x + axis_y * axis_y;
        let x0 = (x.floor() as i64).max(clip.x0).max(0);
        let y0 = (y.floor() as i64).max(clip.y0).max(0);
        let x1 = ((x + w).ceil() as i64).min(clip.x1);
        let y1 = ((y + h).ceil() as i64).min(clip.y1);
        for py in y0..y1 {
            for px in x0..x1 {
                let t = if len_sq <= f32::EPSILON {
                    0.0
                } else {
                    let rx = px as f32 + 0.5 - sx;
                    let ry = py as f32 + 0.5 - sy;
                    (rx * axis_x + ry * axis_y) / len_sq
                };
                let c = self.sample(t);
                let Color(r, g, b, a) = c;
                fb.blend(px as u32, py as u32, pack_rgba(r, g, b, a));
            }
        }
    }
}

/// Linearly interpolate two colours in sRGB component space at `t` in `[0, 1]`.
pub fn lerp_color(a: &Color, b: &Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 {
        let v = x as f32 + (y as f32 - x as f32) * t;
        v.round().clamp(0.0, 255.0) as u8
    };
    Color(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2), mix(a.3, b.3))
}

// ---------------------------------------------------------------------------
// Radial gradient
// ---------------------------------------------------------------------------

/// A radial gradient defined by a centre point, a radius, and colour stops.
///
/// The gradient transitions from `stops[0]` at the centre to `stops[-1]` at
/// the edge of the circle. Beyond the radius the last stop colour is used.
#[derive(Clone, Debug)]
pub struct RadialGradient {
    /// Centre of the gradient circle (logical pixels).
    pub center: (f32, f32),
    /// Radius of the gradient circle (logical pixels).
    pub radius: f32,
    /// Colour stops, sorted by ascending `offset` (`0.0` = centre, `1.0` = edge).
    pub stops: Vec<GradientStop>,
}

impl RadialGradient {
    /// Construct a two-stop radial gradient from `inner` at the centre to
    /// `outer` at the edge.
    pub fn two_stop(center: (f32, f32), radius: f32, inner: Color, outer: Color) -> Self {
        Self {
            center,
            radius,
            stops: vec![GradientStop::new(0.0, inner), GradientStop::new(1.0, outer)],
        }
    }

    /// Construct from arbitrary stops; they are sorted by offset internally.
    pub fn new(center: (f32, f32), radius: f32, mut stops: Vec<GradientStop>) -> Self {
        stops.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        Self {
            center,
            radius,
            stops,
        }
    }

    /// Sample the gradient colour at pixel `(px, py)`.
    ///
    /// The normalised position `t` is the Euclidean distance from `center`
    /// divided by `radius`, clamped to `[0, 1]`.
    pub fn color_at(&self, px: f32, py: f32) -> Color {
        if self.stops.is_empty() {
            return Color(0, 0, 0, 0);
        }
        let r = self.radius.max(f32::EPSILON);
        let dx = px - self.center.0;
        let dy = py - self.center.1;
        let dist = (dx * dx + dy * dy).sqrt();
        let t = (dist / r).clamp(0.0, 1.0);
        self.sample(t)
    }

    /// Sample the gradient at normalised position `t` in `[0, 1]`.
    pub fn sample(&self, t: f32) -> Color {
        if self.stops.is_empty() {
            return Color(0, 0, 0, 0);
        }
        let t = t.clamp(0.0, 1.0);
        if t <= self.stops[0].offset {
            return self.stops[0].color;
        }
        let last = self.stops.len() - 1;
        if t >= self.stops[last].offset {
            return self.stops[last].color;
        }
        for w in self.stops.windows(2) {
            let a = &w[0];
            let b = &w[1];
            if t >= a.offset && t <= b.offset {
                let span = (b.offset - a.offset).max(f32::EPSILON);
                let local = (t - a.offset) / span;
                return lerp_color(&a.color, &b.color, local);
            }
        }
        self.stops[last].color
    }

    /// Fill the rectangle `[x, x+w) × [y, y+h)` of `fb` with this radial
    /// gradient, clipped to `clip`.
    pub fn fill_rect(&self, fb: &mut Framebuffer, clip: &ClipRect, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let x0 = (x.floor() as i64).max(clip.x0).max(0);
        let y0 = (y.floor() as i64).max(clip.y0).max(0);
        let x1 = ((x + w).ceil() as i64).min(clip.x1);
        let y1 = ((y + h).ceil() as i64).min(clip.y1);
        for py in y0..y1 {
            for px in x0..x1 {
                let c = self.color_at(px as f32 + 0.5, py as f32 + 0.5);
                let Color(r, g, b, a) = c;
                fb.blend(px as u32, py as u32, pack_rgba(r, g, b, a));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_endpoints_and_midpoint() {
        let red = Color(255, 0, 0, 255);
        let blue = Color(0, 0, 255, 255);
        assert_eq!(lerp_color(&red, &blue, 0.0), red);
        assert_eq!(lerp_color(&red, &blue, 1.0), blue);
        let mid = lerp_color(&red, &blue, 0.5);
        // Midpoint between red and blue is purple-ish.
        assert!((120..=135).contains(&mid.0));
        assert!((120..=135).contains(&mid.2));
        assert_eq!(mid.1, 0);
    }

    #[test]
    fn sample_two_stop() {
        let g = LinearGradient::two_stop(
            (0.0, 0.0),
            (10.0, 0.0),
            Color(255, 0, 0, 255),
            Color(0, 0, 255, 255),
        );
        assert_eq!(g.sample(0.0), Color(255, 0, 0, 255));
        assert_eq!(g.sample(1.0), Color(0, 0, 255, 255));
        let m = g.sample(0.5);
        assert!((120..=135).contains(&m.0));
    }

    #[test]
    fn fill_rect_horizontal_midpoint_is_purple() {
        let mut fb = Framebuffer::with_fill(10, 1, Color(0, 0, 0, 255));
        let clip = ClipRect::full(10, 1);
        let g = LinearGradient::two_stop(
            (0.0, 0.0),
            (10.0, 0.0),
            Color(255, 0, 0, 255),
            Color(0, 0, 255, 255),
        );
        g.fill_rect(&mut fb, &clip, 0.0, 0.0, 10.0, 1.0);
        // Left edge near red.
        let (lr, _, _, _) = fb.get_rgba(0, 0).expect("left");
        assert!(lr > 200);
        // Right edge near blue.
        let (_, _, rb, _) = fb.get_rgba(9, 0).expect("right");
        assert!(rb > 200);
        // Middle is a blend of both channels.
        let (mr, _, mb, _) = fb.get_rgba(5, 0).expect("mid");
        assert!(mr > 0 && mb > 0);
    }

    #[test]
    fn multi_stop_sorted() {
        let g = LinearGradient::new(
            (0.0, 0.0),
            (1.0, 0.0),
            vec![
                GradientStop::new(1.0, Color(0, 0, 255, 255)),
                GradientStop::new(0.0, Color(255, 0, 0, 255)),
                GradientStop::new(0.5, Color(0, 255, 0, 255)),
            ],
        );
        // Sorted, so sample at 0.5 is the green stop.
        assert_eq!(g.sample(0.5), Color(0, 255, 0, 255));
    }

    #[test]
    fn radial_gradient_midpoint() {
        // Center = red (t=0), edge = blue (t=1), midpoint ≈ purple.
        let g = RadialGradient::two_stop(
            (5.0, 5.0),
            10.0,
            Color(255, 0, 0, 255),
            Color(0, 0, 255, 255),
        );
        // At centre → red.
        let c_center = g.color_at(5.0, 5.0);
        assert_eq!(c_center, Color(255, 0, 0, 255));
        // At radius distance → blue.
        let c_edge = g.color_at(15.0, 5.0); // 10px right of center
        assert_eq!(c_edge, Color(0, 0, 255, 255));
        // At half-radius → mid purple.
        let c_mid = g.color_at(10.0, 5.0); // 5px right of center
        assert!((100..=160).contains(&c_mid.0), "r={}", c_mid.0);
        assert!((100..=160).contains(&c_mid.2), "b={}", c_mid.2);
    }

    #[test]
    fn radial_gradient_fill_rect() {
        let mut fb = Framebuffer::with_fill(20, 20, Color(0, 0, 0, 255));
        let clip = ClipRect::full(20, 20);
        let g = RadialGradient::two_stop(
            (10.0, 10.0),
            10.0,
            Color(255, 0, 0, 255),
            Color(0, 0, 255, 255),
        );
        g.fill_rect(&mut fb, &clip, 0.0, 0.0, 20.0, 20.0);
        // Centre pixel should be near red.
        let (r, _, b, _) = fb.get_rgba(10, 10).expect("centre");
        assert!(r > b, "centre should be red-dominant (r={r}, b={b})");
        // Corner pixel (far from centre) should be near blue.
        let (r2, _, b2, _) = fb.get_rgba(0, 0).expect("corner");
        assert!(b2 >= r2, "corner should be blue-dominant (r={r2}, b={b2})");
    }
}
