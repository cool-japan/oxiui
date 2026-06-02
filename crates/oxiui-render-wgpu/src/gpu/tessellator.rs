//! CPU-side path tessellator for `DrawCommand::FillPath` and
//! `DrawCommand::StrokePath`.
//!
//! This module converts [`PathData`] into triangle fans / stroke quads that can
//! be submitted directly to the solid GPU pipeline (kind=0 triangles).  No
//! external dependencies are used — only `std`.
//!
//! # Fill tessellation
//!
//! 1. Flatten all path verbs into a contiguous list of pixel-space points using
//!    adaptive De Casteljau subdivision (tolerance = 0.25 px²).
//! 2. Fan-triangulate from the first point of each sub-path to produce a
//!    triangle list.  This is exact for convex paths and a reasonable
//!    approximation for simple non-self-intersecting concave paths (ear-clip
//!    quality is not required for typical UI shapes).
//!
//! # Stroke tessellation
//!
//! Each consecutive segment pair is expanded into a parallelogram quad using the
//! same perpendicular-vector approach as [`push_line_quad`].  Joins between
//! segments use a miter/bevel strategy.

use oxiui_core::paint::{LineCap, LineJoin, PathData, PathVerb, StrokeStyle};
use oxiui_core::Color;

use crate::gpu::buffer::{push_line_quad, push_triangle, LineQuadParams, Vertex};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum angle step for arc approximation (in radians).
const ARC_STEP: f32 = 0.1;
/// De Casteljau flatness tolerance in squared pixels.
const FLATNESS_SQ: f32 = 0.0625; // 0.25² px²

// ── Public API ────────────────────────────────────────────────────────────────

/// Tessellate `path` into solid colour triangles (kind=0) appended to `out`.
pub fn tessellate_fill(out: &mut Vec<Vertex>, path: &PathData, color: Color) {
    let sub_paths = flatten_path(path);
    for pts in &sub_paths {
        fan_triangulate(out, pts, color);
    }
}

/// Tessellate `path` into stroke quads appended to `out`.
pub fn tessellate_stroke(
    out: &mut Vec<Vertex>,
    path: &PathData,
    style: &StrokeStyle,
    color: Color,
) {
    let sub_paths = flatten_path(path);
    for pts in &sub_paths {
        stroke_sub_path(out, pts, style, color);
    }
}

// ── Path flattening ───────────────────────────────────────────────────────────

/// Convert a [`PathData`] into one or more lists of pixel-space points.
/// Each `MoveTo` starts a new sub-path.  `Close` reuses the first point
/// (so the caller can draw a closed stroke or filled shape).
fn flatten_path(path: &PathData) -> Vec<Vec<[f32; 2]>> {
    let mut result: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut last = [0.0f32; 2];

    for verb in &path.verbs {
        match *verb {
            PathVerb::MoveTo(p) => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
                last = [p.x, p.y];
                current.push(last);
            }
            PathVerb::LineTo(p) => {
                last = [p.x, p.y];
                current.push(last);
            }
            PathVerb::QuadTo { ctrl, end } => {
                flatten_quad(&mut current, last, [ctrl.x, ctrl.y], [end.x, end.y]);
                last = [end.x, end.y];
            }
            PathVerb::CubicTo { c1, c2, end } => {
                flatten_cubic(
                    &mut current,
                    last,
                    [c1.x, c1.y],
                    [c2.x, c2.y],
                    [end.x, end.y],
                );
                last = [end.x, end.y];
            }
            PathVerb::Close => {
                if current.len() > 1 {
                    let first = current[0];
                    // Only close if the last point isn't already at the first.
                    if dist_sq(last, first) > 1e-6 {
                        current.push(first);
                    }
                    result.push(std::mem::take(&mut current));
                }
            }
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Recursively flatten a quadratic Bézier using De Casteljau subdivision.
fn flatten_quad(out: &mut Vec<[f32; 2]>, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) {
    // Flatness test: is the control point close enough to the chord?
    let mx = (p0[0] + p2[0]) * 0.5;
    let my = (p0[1] + p2[1]) * 0.5;
    if dist_sq([mx, my], p1) <= FLATNESS_SQ {
        out.push(p2);
        return;
    }
    // Subdivide at t=0.5.
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let r = mid(q0, q1);
    flatten_quad(out, p0, q0, r);
    flatten_quad(out, r, q1, p2);
}

/// Recursively flatten a cubic Bézier using De Casteljau subdivision.
fn flatten_cubic(out: &mut Vec<[f32; 2]>, p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) {
    // Flatness test: max deviation of control points from the chord.
    let chord_sq = dist_sq(p0, p3).max(1e-9);
    let d1 = point_line_dist_sq(p1, p0, p3);
    let d2 = point_line_dist_sq(p2, p0, p3);
    if (d1 + d2) / chord_sq <= FLATNESS_SQ {
        out.push(p3);
        return;
    }
    // Subdivide at t=0.5.
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let q2 = mid(p2, p3);
    let r0 = mid(q0, q1);
    let r1 = mid(q1, q2);
    let s = mid(r0, r1);
    flatten_cubic(out, p0, q0, r0, s);
    flatten_cubic(out, s, r1, q2, p3);
}

// ── Fill tessellation ─────────────────────────────────────────────────────────

/// Fan-triangulate a sub-path from its first vertex.
fn fan_triangulate(out: &mut Vec<Vertex>, pts: &[[f32; 2]], color: Color) {
    if pts.len() < 3 {
        return;
    }
    let p0 = pts[0];
    for i in 1..pts.len() - 1 {
        push_triangle(out, p0, pts[i], pts[i + 1], color);
    }
}

// ── Stroke tessellation ───────────────────────────────────────────────────────

fn stroke_sub_path(out: &mut Vec<Vertex>, pts: &[[f32; 2]], style: &StrokeStyle, color: Color) {
    if pts.len() < 2 {
        return;
    }
    let hw = style.width * 0.5;
    let n = pts.len();

    for i in 0..n - 1 {
        let a = pts[i];
        let b = pts[i + 1];
        push_line_quad(
            out,
            LineQuadParams {
                from_x: a[0],
                from_y: a[1],
                to_x: b[0],
                to_y: b[1],
                half_width: hw,
                color,
                aa_smooth: false,
            },
        );
    }

    // End caps.
    match style.cap {
        LineCap::Round => {
            add_round_cap(out, pts[0], pts[1], hw, true, color);
            add_round_cap(out, pts[n - 1], pts[n - 2], hw, false, color);
        }
        LineCap::Square => {
            add_square_cap(out, pts[0], pts[1], hw, true, color);
            add_square_cap(out, pts[n - 1], pts[n - 2], hw, false, color);
        }
        LineCap::Butt => {}
    }

    // Joins between segments.
    if n > 2 {
        for i in 1..n - 1 {
            let prev = pts[i - 1];
            let cur = pts[i];
            let next = pts[i + 1];
            match style.join {
                LineJoin::Round => add_round_join(out, prev, cur, next, hw, color),
                LineJoin::Bevel => add_bevel_join(out, prev, cur, next, hw, color),
                LineJoin::Miter => {
                    add_miter_join(out, prev, cur, next, hw, style.miter_limit, color)
                }
            }
        }
    }
}

/// Add a semicircular end cap at the endpoint.
fn add_round_cap(
    out: &mut Vec<Vertex>,
    endpoint: [f32; 2],
    next: [f32; 2],
    hw: f32,
    is_start: bool,
    color: Color,
) {
    // Direction away from the line (towards cap exterior).
    // `is_start` is unused here because the direction formula is the same for
    // both endpoints (point away from the adjacent segment vertex).
    let _ = is_start;
    let dx = endpoint[0] - next[0];
    let dy = endpoint[1] - next[1];
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let tx = dx / len;
    let ty = dy / len;

    // Approximate semicircle with triangle fan.
    let steps = ((hw * std::f32::consts::PI / ARC_STEP).ceil() as u32).clamp(4, 32);
    let start_angle = ty.atan2(tx);
    let cx = endpoint[0];
    let cy = endpoint[1];
    for i in 0..steps {
        let a0 = start_angle + std::f32::consts::PI * i as f32 / steps as f32;
        let a1 = start_angle + std::f32::consts::PI * (i + 1) as f32 / steps as f32;
        let p0 = [cx + hw * a0.cos(), cy + hw * a0.sin()];
        let p1 = [cx + hw * a1.cos(), cy + hw * a1.sin()];
        push_triangle(out, [cx, cy], p0, p1, color);
    }
}

/// Add a square (projecting) end cap at the endpoint.
fn add_square_cap(
    out: &mut Vec<Vertex>,
    endpoint: [f32; 2],
    next: [f32; 2],
    hw: f32,
    _is_start: bool,
    color: Color,
) {
    let dx = endpoint[0] - next[0];
    let dy = endpoint[1] - next[1];
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let tx = dx / len;
    let ty = dy / len;
    let nx = -ty;
    let ny = tx;
    // The cap is a rectangle extending hw units beyond the endpoint.
    let a = [endpoint[0] + nx * hw, endpoint[1] + ny * hw];
    let b = [endpoint[0] - nx * hw, endpoint[1] - ny * hw];
    let c = [
        endpoint[0] + tx * hw - nx * hw,
        endpoint[1] + ty * hw - ny * hw,
    ];
    let d = [
        endpoint[0] + tx * hw + nx * hw,
        endpoint[1] + ty * hw + ny * hw,
    ];
    push_triangle(out, a, b, c, color);
    push_triangle(out, a, c, d, color);
}

/// Add a round join between two consecutive segments.
fn add_round_join(
    out: &mut Vec<Vertex>,
    prev: [f32; 2],
    cur: [f32; 2],
    next: [f32; 2],
    hw: f32,
    color: Color,
) {
    let dx0 = cur[0] - prev[0];
    let dy0 = cur[1] - prev[1];
    let dx1 = next[0] - cur[0];
    let dy1 = next[1] - cur[1];
    let len0 = (dx0 * dx0 + dy0 * dy0).sqrt().max(1e-6);
    let len1 = (dx1 * dx1 + dy1 * dy1).sqrt().max(1e-6);
    let a0 = (-dy0 / len0).atan2(dx0 / len0) + std::f32::consts::FRAC_PI_2;
    let a1 = (-dy1 / len1).atan2(dx1 / len1) + std::f32::consts::FRAC_PI_2;
    let mut da = a1 - a0;
    // Normalise to [-pi, pi].
    while da > std::f32::consts::PI {
        da -= std::f32::consts::TAU;
    }
    while da < -std::f32::consts::PI {
        da += std::f32::consts::TAU;
    }
    let steps = ((da.abs() / ARC_STEP).ceil() as u32).clamp(1, 32);
    let cx = cur[0];
    let cy = cur[1];
    for i in 0..steps {
        let aa = a0 + da * i as f32 / steps as f32;
        let ab = a0 + da * (i + 1) as f32 / steps as f32;
        let p0 = [cx + hw * aa.cos(), cy + hw * aa.sin()];
        let p1 = [cx + hw * ab.cos(), cy + hw * ab.sin()];
        push_triangle(out, [cx, cy], p0, p1, color);
    }
}

/// Add a bevel join between two consecutive segments.
fn add_bevel_join(
    out: &mut Vec<Vertex>,
    prev: [f32; 2],
    cur: [f32; 2],
    next: [f32; 2],
    hw: f32,
    color: Color,
) {
    let (n0, n1) = segment_normals(prev, cur, next);
    // Bevel = one triangle filling the gap.
    push_triangle(
        out,
        cur,
        [cur[0] + n0[0] * hw, cur[1] + n0[1] * hw],
        [cur[0] + n1[0] * hw, cur[1] + n1[1] * hw],
        color,
    );
}

/// Add a miter join (falls back to bevel if the miter ratio is exceeded).
fn add_miter_join(
    out: &mut Vec<Vertex>,
    prev: [f32; 2],
    cur: [f32; 2],
    next: [f32; 2],
    hw: f32,
    miter_limit: f32,
    color: Color,
) {
    let (n0, n1) = segment_normals(prev, cur, next);
    // Bisector direction (normalised average of the two normals).
    let bx = n0[0] + n1[0];
    let by = n0[1] + n1[1];
    let blen = (bx * bx + by * by).sqrt();
    if blen < 1e-6 {
        add_bevel_join(out, prev, cur, next, hw, color);
        return;
    }
    // Miter length = hw / cos(half_angle).
    let cos_half = ((n0[0] * n1[0] + n0[1] * n1[1]) * 0.5 + 0.5)
        .sqrt()
        .max(1e-6);
    let miter_len = hw / cos_half;
    if miter_len / hw > miter_limit {
        add_bevel_join(out, prev, cur, next, hw, color);
        return;
    }
    let scale = miter_len / blen;
    let mx = cur[0] + bx * scale;
    let my = cur[1] + by * scale;
    // Fill the miter triangle on the outside of the corner.
    push_triangle(
        out,
        [cur[0] + n0[0] * hw, cur[1] + n0[1] * hw],
        [mx, my],
        [cur[0] + n1[0] * hw, cur[1] + n1[1] * hw],
        color,
    );
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Squared distance between two points.
#[inline]
fn dist_sq(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// Midpoint of two points.
#[inline]
fn mid(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

/// Squared distance from point `p` to the line through `a` and `b`.
fn point_line_dist_sq(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let abx = b[0] - a[0];
    let aby = b[1] - a[1];
    let apx = p[0] - a[0];
    let apy = p[1] - a[1];
    let ab2 = abx * abx + aby * aby;
    if ab2 < 1e-12 {
        return apx * apx + apy * apy;
    }
    let cross = apx * aby - apy * abx;
    cross * cross / ab2
}

/// Compute the outward-facing unit normals of the two line segments meeting at
/// `cur`.  Returns `(normal_of_prev_seg, normal_of_next_seg)`.
fn segment_normals(prev: [f32; 2], cur: [f32; 2], next: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let n0 = perp_unit(prev, cur);
    let n1 = perp_unit(cur, next);
    (n0, n1)
}

/// Unit normal (perpendicular, left-hand side) to the segment from `a` to `b`.
fn perp_unit(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    [-dy / len, dx / len]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::geometry::Point;
    use oxiui_core::paint::{FillRule, PathData};
    use oxiui_core::Color;

    fn triangle_path() -> PathData {
        let mut p = PathData::new();
        p.move_to(Point::new(0.0, 0.0));
        p.line_to(Point::new(20.0, 0.0));
        p.line_to(Point::new(10.0, 15.0));
        p.close();
        p
    }

    fn quad_path() -> PathData {
        let mut p = PathData::new();
        p.move_to(Point::new(0.0, 0.0));
        p.quad_to(Point::new(10.0, -10.0), Point::new(20.0, 0.0));
        p.close();
        p
    }

    #[test]
    fn triangle_fill_produces_triangles() {
        let mut verts = Vec::new();
        tessellate_fill(&mut verts, &triangle_path(), Color(255, 0, 0, 255));
        // A closed triangle has 4 pts (p0, p1, p2, p0). Fan triangulation
        // from p0 produces 2 triangles = 6 vertices.
        assert_eq!(verts.len(), 6);
        assert_eq!(verts.len() % 3, 0);
    }

    #[test]
    fn quad_bezier_fills_polygon() {
        let mut verts = Vec::new();
        tessellate_fill(&mut verts, &quad_path(), Color(0, 255, 0, 255));
        // Flattened quad: multiple segments → multiple fan triangles.
        assert!(verts.len() >= 3);
        assert_eq!(verts.len() % 3, 0);
    }

    #[test]
    fn stroke_produces_quads() {
        let mut verts = Vec::new();
        let style = StrokeStyle {
            width: 2.0,
            ..Default::default()
        };
        tessellate_stroke(&mut verts, &triangle_path(), &style, Color(0, 0, 255, 255));
        // Each of the 3 segments produces 6 vertices.
        assert!(verts.len() >= 18);
    }

    #[test]
    fn flatten_path_empty_produces_no_sub_paths() {
        let path = PathData::new();
        let result = flatten_path(&path);
        assert!(result.is_empty());
    }

    #[test]
    fn mid_point_is_correct() {
        let m = mid([0.0, 0.0], [10.0, 4.0]);
        assert!((m[0] - 5.0).abs() < 1e-5);
        assert!((m[1] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn fill_rule_field_is_accessible() {
        let p = PathData::new().with_fill_rule(FillRule::EvenOdd);
        assert_eq!(p.fill_rule, FillRule::EvenOdd);
    }
}
