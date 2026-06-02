//! Active-Edge-Table (AET) scanline rasteriser for polygon and triangle fill.
//!
//! Supports sub-pixel vertical-supersample coverage anti-aliasing, even-odd
//! fill rule, and non-zero winding fill rule. Both are computed by the same
//! AET machinery — the rule only changes how the accumulated winding count is
//! interpreted per scanline span.

use crate::framebuffer::Framebuffer;
use oxiui_core::Color;

/// How self-intersecting or overlapping sub-paths are filled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FillRule {
    /// Standard even-odd rule: regions enclosed by an odd number of boundary
    /// crossings are filled; even crossings produce holes.
    EvenOdd,
    /// Non-zero winding rule: a point is inside if the signed crossing count is
    /// non-zero.
    #[default]
    NonZero,
}

// ---------------------------------------------------------------------------
// Internal AET machinery
// ---------------------------------------------------------------------------

/// A single edge for the active-edge-table algorithm, stored in float for
/// sub-pixel accuracy.
#[derive(Clone, Debug)]
pub(crate) struct Edge {
    /// Current X position at the current scanline's top.
    x: f32,
    /// X increment per scanline (dx/dy).
    dx: f32,
    /// Scanline at which this edge ends (exclusive).
    y_max: i32,
    /// Winding contribution: +1 for upward, -1 for downward (used by
    /// non-zero winding rule).
    winding: i32,
}

// ---------------------------------------------------------------------------
// Rasterizer scratch buffers — reused across fills to avoid per-polygon alloc
// ---------------------------------------------------------------------------

/// Scratch buffers owned by a rasterizer, reused across polygon fills.
///
/// Call [`RasterizerScratch::clear`] at the start of each fill instead of
/// creating fresh `Vec`s.
pub struct RasterizerScratch {
    /// Global edge table (y_start, Edge) pairs for the current polygon.
    pub(crate) global_edges: Vec<(i32, Edge)>,
    /// Active edge table for the current scanline.
    pub(crate) active: Vec<Edge>,
}

impl RasterizerScratch {
    /// Allocate empty scratch buffers.
    pub fn new() -> Self {
        Self {
            global_edges: Vec::new(),
            active: Vec::new(),
        }
    }

    /// Clear both buffers for the next polygon fill (no heap deallocation).
    pub fn clear(&mut self) {
        self.global_edges.clear();
        self.active.clear();
    }
}

impl Default for RasterizerScratch {
    fn default() -> Self {
        Self::new()
    }
}

/// A scanline rasterizer that owns reusable scratch buffers so per-polygon
/// heap allocation is avoided.
///
/// Use [`Rasterizer::fill_polygon`] / [`Rasterizer::fill_polygon_clipped`]
/// in performance-critical loops. The free-standing functions in this module
/// delegate here and are kept for API stability.
pub struct Rasterizer {
    scratch: RasterizerScratch,
}

impl Rasterizer {
    /// Create a new rasterizer with empty scratch buffers.
    pub fn new() -> Self {
        Self {
            scratch: RasterizerScratch::new(),
        }
    }

    /// Fill a polygon into `fb`, reusing internal scratch buffers.
    pub fn fill_polygon(
        &mut self,
        fb: &mut Framebuffer,
        points: &[(f32, f32)],
        color: Color,
        fill_rule: FillRule,
        aa: bool,
    ) {
        fill_polygon_with_scratch(fb, points, color, fill_rule, aa, &mut self.scratch);
    }

    /// Fill a polygon into `fb`, clipped to `clip`, reusing internal scratch.
    pub fn fill_polygon_clipped(
        &mut self,
        fb: &mut Framebuffer,
        points: &[(f32, f32)],
        color: Color,
        fill_rule: FillRule,
        aa: bool,
        clip: crate::clip::ClipRect,
    ) {
        fill_polygon_clipped_with_scratch(
            fb,
            points,
            color,
            fill_rule,
            aa,
            clip,
            &mut self.scratch,
        );
    }
}

impl Default for Rasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the global edge table from a polygon vertex list, appending into `out`.
///
/// Each consecutive pair of vertices forms an edge; the last vertex connects
/// back to the first to close the polygon. Horizontal edges (dy == 0) are
/// skipped. `out` is *not* cleared by this function — callers must clear it
/// before calling if a fresh table is needed.
fn build_edges_into(points: &[(f32, f32)], out: &mut Vec<(i32, Edge)>) {
    let n = points.len();
    if n < 2 {
        return;
    }
    out.reserve(n);
    for i in 0..n {
        let (x0, y0) = points[i];
        let (x1, y1) = points[(i + 1) % n];
        if (y0 - y1).abs() < f32::EPSILON {
            // Horizontal — skip.
            continue;
        }
        let (top_y, bot_y, top_x, bot_x, winding) = if y0 < y1 {
            (y0, y1, x0, x1, 1i32)
        } else {
            (y1, y0, x1, x0, -1i32)
        };
        let dy = bot_y - top_y;
        let dx = (bot_x - top_x) / dy;
        let y_start = top_y.ceil() as i32;
        // Sub-pixel correction: move x to the y_start scanline centre.
        let x_at_start = top_x + dx * (y_start as f32 - top_y);
        let y_max = bot_y.ceil() as i32;
        out.push((
            y_start,
            Edge {
                x: x_at_start,
                dx,
                y_max,
                winding,
            },
        ));
    }
}

/// Compute the alpha (coverage) for a single pixel column within a span,
/// given supersample coverage fraction.
///
/// This is a simple closed-form computation: sample 8 sub-rows per pixel
/// to get fractional coverage at the span boundary. The interior receives 1.0.
#[inline]
fn span_coverage(left: f32, right: f32, px: f32) -> f32 {
    // Pixel occupies [px, px+1).
    let pl = px;
    let pr = px + 1.0;
    if right <= pl || left >= pr {
        return 0.0;
    }
    let cl = left.max(pl);
    let cr = right.min(pr);
    (cr - cl).clamp(0.0, 1.0)
}

/// Core fill implementation that reuses caller-provided scratch buffers.
fn fill_polygon_with_scratch(
    fb: &mut Framebuffer,
    points: &[(f32, f32)],
    color: Color,
    fill_rule: FillRule,
    aa: bool,
    scratch: &mut RasterizerScratch,
) {
    if points.len() < 3 {
        return;
    }

    // Bounding box.
    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for &(_, y) in points {
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    let y_start = min_y.floor() as i32;
    let y_end = max_y.ceil() as i32;

    // Build global edge table into scratch buffer.
    scratch.clear();
    build_edges_into(points, &mut scratch.global_edges);

    // Sort global edge table by y_start ascending, then by x.
    scratch.global_edges.sort_by(|a, b| {
        a.0.cmp(&b.0).then(
            a.1.x
                .partial_cmp(&b.1.x)
                .unwrap_or(core::cmp::Ordering::Equal),
        )
    });

    let mut gi = 0usize;

    for y in y_start..y_end {
        // Add edges whose y_start == y.
        while gi < scratch.global_edges.len() && scratch.global_edges[gi].0 == y {
            scratch.active.push(scratch.global_edges[gi].1.clone());
            gi += 1;
        }

        // Remove expired edges.
        scratch.active.retain(|e| e.y_max > y);

        // Sort active edges by current x.
        scratch
            .active
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(core::cmp::Ordering::Equal));

        // Fill spans using the chosen fill rule.
        fill_spans(
            fb,
            &scratch.active,
            y as u32,
            y as f32,
            color,
            fill_rule,
            aa,
        );

        // Advance active edges.
        for e in &mut scratch.active {
            e.x += e.dx;
        }
    }
}

/// Fill a polygon (closed contour) defined by `points` into `fb`.
///
/// `points` is interpreted as an ordered list of (x, y) vertices; the polygon
/// is implicitly closed (last → first). Uses a vertical-supersample AET for
/// sub-pixel AA.
///
/// Does nothing if there are fewer than 3 points.
///
/// For tight loops over many polygons, prefer [`Rasterizer::fill_polygon`]
/// which reuses scratch buffers across calls.
pub fn fill_polygon(
    fb: &mut Framebuffer,
    points: &[(f32, f32)],
    color: Color,
    fill_rule: FillRule,
    aa: bool,
) {
    let mut scratch = RasterizerScratch::new();
    fill_polygon_with_scratch(fb, points, color, fill_rule, aa, &mut scratch);
}

/// Process active edges for one scanline using the chosen fill rule.
fn fill_spans(
    fb: &mut Framebuffer,
    active: &[Edge],
    y: u32,
    _y_float: f32,
    color: Color,
    fill_rule: FillRule,
    aa: bool,
) {
    if y >= fb.height() || active.is_empty() {
        return;
    }

    match fill_rule {
        FillRule::EvenOdd => fill_even_odd(fb, active, y, color, aa),
        FillRule::NonZero => fill_non_zero(fb, active, y, color, aa),
    }
}

/// Even-odd rule: pairs of edges delimit filled spans.
fn fill_even_odd(fb: &mut Framebuffer, active: &[Edge], y: u32, color: Color, aa: bool) {
    let mut i = 0;
    while i + 1 < active.len() {
        let left = active[i].x;
        let right = active[i + 1].x;
        if right > left {
            paint_span(fb, left, right, y, color, aa);
        }
        i += 2;
    }
}

/// Non-zero winding rule: accumulate winding; fill while winding != 0.
fn fill_non_zero(fb: &mut Framebuffer, active: &[Edge], y: u32, color: Color, aa: bool) {
    let mut winding = 0i32;
    let mut fill_start: Option<f32> = None;

    for edge in active {
        let prev_winding = winding;
        winding += edge.winding;

        if prev_winding == 0 && winding != 0 {
            // Entering a filled region.
            fill_start = Some(edge.x);
        } else if prev_winding != 0 && winding == 0 {
            // Leaving a filled region.
            if let Some(start) = fill_start.take() {
                let end = edge.x;
                if end > start {
                    paint_span(fb, start, end, y, color, aa);
                }
            }
        }
    }
}

/// Paint a horizontal span `[left, right)` on row `y`, with optional edge AA.
fn paint_span(fb: &mut Framebuffer, left: f32, right: f32, y: u32, color: Color, aa: bool) {
    if y >= fb.height() {
        return;
    }
    let x0 = left.floor() as i32;
    let x1 = right.ceil() as i32;
    let Color(cr, cg, cb, ca) = color;

    for px in x0..x1 {
        if px < 0 || px as u32 >= fb.width() {
            continue;
        }
        let coverage = if aa {
            span_coverage(left, right, px as f32)
        } else {
            // Hard edge: any pixel whose centre is inside the span.
            let centre = px as f32 + 0.5;
            if centre >= left && centre < right {
                1.0
            } else {
                0.0
            }
        };
        if coverage <= 0.0 {
            continue;
        }
        let alpha = (ca as f32 * coverage).round() as u8;
        fb.blend(
            px as u32,
            y,
            crate::framebuffer::pack_rgba(cr, cg, cb, alpha),
        );
    }
}

/// Core clipped-fill implementation that reuses caller-provided scratch buffers.
fn fill_polygon_clipped_with_scratch(
    fb: &mut Framebuffer,
    points: &[(f32, f32)],
    color: Color,
    fill_rule: FillRule,
    aa: bool,
    clip: crate::clip::ClipRect,
    scratch: &mut RasterizerScratch,
) {
    if points.len() < 3 {
        return;
    }

    // Bounding box.
    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for &(_, y) in points {
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    let y_clip_start = (min_y.floor() as i64).max(clip.y0) as i32;
    let y_end = (max_y.ceil() as i64).min(clip.y1) as i32;

    scratch.clear();
    build_edges_into(points, &mut scratch.global_edges);
    scratch.global_edges.sort_by(|a, b| {
        a.0.cmp(&b.0).then(
            a.1.x
                .partial_cmp(&b.1.x)
                .unwrap_or(core::cmp::Ordering::Equal),
        )
    });

    let mut gi = 0usize;

    for y in (min_y.floor() as i32)..y_end {
        while gi < scratch.global_edges.len() && scratch.global_edges[gi].0 == y {
            scratch.active.push(scratch.global_edges[gi].1.clone());
            gi += 1;
        }
        scratch.active.retain(|e| e.y_max > y);
        scratch
            .active
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(core::cmp::Ordering::Equal));
        if y >= y_clip_start {
            fill_spans_clipped(fb, &scratch.active, y as u32, color, fill_rule, aa, &clip);
        }
        for e in &mut scratch.active {
            e.x += e.dx;
        }
    }
}

/// Fill a polygon clipped to a [`crate::clip::ClipRect`].
///
/// This is the same as [`fill_polygon`] but additionally clips to `clip`.
///
/// For tight loops over many polygons, prefer [`Rasterizer::fill_polygon_clipped`]
/// which reuses scratch buffers across calls.
pub fn fill_polygon_clipped(
    fb: &mut Framebuffer,
    points: &[(f32, f32)],
    color: Color,
    fill_rule: FillRule,
    aa: bool,
    clip: crate::clip::ClipRect,
) {
    let mut scratch = RasterizerScratch::new();
    fill_polygon_clipped_with_scratch(fb, points, color, fill_rule, aa, clip, &mut scratch);
}

/// Like `fill_spans` but clips each span to `clip`.
fn fill_spans_clipped(
    fb: &mut Framebuffer,
    active: &[Edge],
    y: u32,
    color: Color,
    fill_rule: FillRule,
    aa: bool,
    clip: &crate::clip::ClipRect,
) {
    if y >= fb.height() || (y as i64) < clip.y0 || (y as i64) >= clip.y1 || active.is_empty() {
        return;
    }
    match fill_rule {
        FillRule::EvenOdd => {
            let mut i = 0;
            while i + 1 < active.len() {
                let left = active[i].x.max(clip.x0 as f32);
                let right = active[i + 1].x.min(clip.x1 as f32);
                if right > left {
                    paint_span(fb, left, right, y, color, aa);
                }
                i += 2;
            }
        }
        FillRule::NonZero => {
            let mut winding = 0i32;
            let mut fill_start: Option<f32> = None;
            for edge in active {
                let prev_winding = winding;
                winding += edge.winding;
                if prev_winding == 0 && winding != 0 {
                    fill_start = Some(edge.x.max(clip.x0 as f32));
                } else if prev_winding != 0 && winding == 0 {
                    if let Some(start) = fill_start.take() {
                        let end = edge.x.min(clip.x1 as f32);
                        if end > start {
                            paint_span(fb, start, end, y, color, aa);
                        }
                    }
                }
            }
        }
    }
}

/// Fill a triangle defined by three points into `fb` with coverage AA.
///
/// Delegates to [`fill_polygon`] with three vertices.
pub fn fill_triangle(
    fb: &mut Framebuffer,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    color: Color,
) {
    fill_polygon(fb, &[p0, p1, p2], color, FillRule::NonZero, true);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;

    fn fresh(w: u32, h: u32) -> Framebuffer {
        Framebuffer::with_fill(w, h, Color(0, 0, 0, 255))
    }

    #[test]
    fn triangle_fill_coverage() {
        let mut fb = fresh(20, 20);
        // Fill a large triangle covering roughly half the buffer.
        fill_triangle(
            &mut fb,
            (0.0, 0.0),
            (20.0, 0.0),
            (10.0, 20.0),
            Color(255, 255, 255, 255),
        );
        // Count non-background pixels.
        let mut count = 0u32;
        for y in 0..20 {
            for x in 0..20 {
                let (r, _, _, _) = fb.get_rgba(x, y).unwrap_or((0, 0, 0, 0));
                if r > 0 {
                    count += 1;
                }
            }
        }
        // The triangle should fill at least 50 pixels in a 20x20 buffer.
        assert!(count >= 50, "expected >= 50 filled pixels, got {count}");
    }

    #[test]
    fn polygon_fill_even_odd() {
        // 5-pointed star: a self-intersecting polygon where even-odd leaves the
        // center pentagon empty, while non-zero fills it.
        //
        // Use a large star (radius=30, centre=35,35) so individual arms are wide.
        let cx = 35.0f32;
        let cy = 35.0f32;
        let r = 30.0f32;
        let star: Vec<(f32, f32)> = (0..5)
            .map(|i| {
                // "Skip-one" order produces a 5-pointed star.
                let angle = std::f32::consts::PI * (2.0 * (2 * i) as f32 / 5.0 - 0.5);
                (cx + r * angle.cos(), cy + r * angle.sin())
            })
            .collect();

        let mut fb_eo = fresh(70, 70);
        let mut fb_nz = fresh(70, 70);
        fill_polygon(
            &mut fb_eo,
            &star,
            Color(255, 0, 0, 255),
            FillRule::EvenOdd,
            false,
        );
        fill_polygon(
            &mut fb_nz,
            &star,
            Color(255, 0, 0, 255),
            FillRule::NonZero,
            false,
        );

        // The top arm of the star spans roughly x=[32,38] at y=8 (well inside the arm).
        let (r_eo_tip, _, _, _) = fb_eo.get_rgba(35, 8).unwrap_or((0, 0, 0, 0));
        let (r_nz_tip, _, _, _) = fb_nz.get_rgba(35, 8).unwrap_or((0, 0, 0, 0));
        assert!(r_eo_tip > 0, "EvenOdd: star arm should be painted");
        assert!(r_nz_tip > 0, "NonZero: star arm should be painted");

        // Center pentagon: NonZero should fill it; EvenOdd should leave it empty.
        let (r_nz_ctr, _, _, _) = fb_nz.get_rgba(35, 35).unwrap_or((0, 0, 0, 0));
        assert!(r_nz_ctr > 0, "NonZero: star center must be filled");
        let (r_eo_ctr, _, _, _) = fb_eo.get_rgba(35, 35).unwrap_or((0, 0, 0, 0));
        assert_eq!(
            r_eo_ctr, 0,
            "EvenOdd: star center should be a hole (r={r_eo_ctr})"
        );
    }

    #[test]
    fn fill_rect_via_polygon() {
        let mut fb = fresh(10, 10);
        let pts = [(2.0f32, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)];
        fill_polygon(
            &mut fb,
            &pts,
            Color(0, 255, 0, 255),
            FillRule::NonZero,
            false,
        );
        // Centre should be green.
        assert_eq!(fb.get_rgba(5, 5), Some((0, 255, 0, 255)));
        // Corner outside should be black.
        assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 255)));
    }

    #[test]
    fn fill_rule_noop_on_empty_polygon() {
        let mut fb = fresh(5, 5);
        fill_polygon(
            &mut fb,
            &[],
            Color(255, 0, 0, 255),
            FillRule::NonZero,
            false,
        );
        // Nothing should change.
        assert_eq!(fb.get_rgba(2, 2), Some((0, 0, 0, 255)));
    }

    // -----------------------------------------------------------------------
    // S2: scanline buffer reuse regression test
    // -----------------------------------------------------------------------

    #[test]
    fn test_scanline_reuse_output_identical() {
        // Regression test: filling a triangle into a 100×100 framebuffer twice
        // using the `Rasterizer` (scratch-reuse path) must produce byte-identical
        // results across both calls.  This verifies that `scratch.clear()` fully
        // resets state between fills so stale edges never contaminate the next fill.
        let triangle: &[(f32, f32)] = &[(10.0, 90.0), (50.0, 10.0), (90.0, 90.0)];
        let color = Color(200, 100, 50, 255);

        // First fill — using the reuse-scratch Rasterizer.
        let mut fb1 = fresh(100, 100);
        let mut ras = Rasterizer::new();
        ras.fill_polygon(&mut fb1, triangle, color, FillRule::NonZero, true);

        // Second fill into a fresh buffer — same Rasterizer, scratch is reused.
        let mut fb2 = fresh(100, 100);
        ras.fill_polygon(&mut fb2, triangle, color, FillRule::NonZero, true);

        // Results must be byte-identical.
        for y in 0..100 {
            for x in 0..100 {
                let p1 = fb1.get_rgba(x, y);
                let p2 = fb2.get_rgba(x, y);
                assert_eq!(
                    p1, p2,
                    "pixel ({x},{y}) differs: first={p1:?} second={p2:?}"
                );
            }
        }

        // Also verify the public free-fn path gives the same pixels (API stability).
        let mut fb3 = fresh(100, 100);
        fill_polygon(&mut fb3, triangle, color, FillRule::NonZero, true);
        for y in 0..100 {
            for x in 0..100 {
                let p1 = fb1.get_rgba(x, y);
                let p3 = fb3.get_rgba(x, y);
                assert_eq!(
                    p1, p3,
                    "pixel ({x},{y}): Rasterizer={p1:?} vs free fn={p3:?}"
                );
            }
        }
    }
}
