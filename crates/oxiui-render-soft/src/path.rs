//! 2D path representation with Bézier flattening, fill, and stroke.
//!
//! [`Path`] stores a sequence of `PathCmd` commands; [`PathBuilder`] offers
//! a fluent API for constructing paths. Flattening uses De Casteljau adaptive
//! subdivision. Fill delegates to [`crate::scanline`]; stroke constructs a
//! parallel-offset polygon and fills it.

use crate::framebuffer::Framebuffer;
use crate::scanline::{FillRule, Rasterizer};
use oxiui_core::Color;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single 2-D point.
pub type Point = (f32, f32);

/// Stroke join style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Join {
    /// Sharp corner, limited by `miter_limit`.
    Miter,
    /// Bevel: flat triangle fills the gap.
    Bevel,
    /// Round: circular arc fills the gap.
    Round,
}

/// Stroke cap style for open sub-paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cap {
    /// Flat cap flush with the endpoint.
    Butt,
    /// Round cap: semicircle centred on the endpoint.
    Round,
    /// Square cap: extends half the line width past the endpoint.
    Square,
}

/// Stroke configuration.
#[derive(Clone, Debug)]
pub struct StrokeStyle {
    /// Line width in pixels.
    pub width: f32,
    /// Line join style.
    pub join: Join,
    /// Line cap style.
    pub cap: Cap,
    /// Miter length limit (multiples of half line-width).
    pub miter_limit: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            join: Join::Miter,
            cap: Cap::Butt,
            miter_limit: 4.0,
        }
    }
}

/// A single path command.
#[derive(Clone, Debug)]
enum PathCmd {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),         // (control, end)
    CubicTo(Point, Point, Point), // (c1, c2, end)
    Close,
}

// ---------------------------------------------------------------------------
// Path
// ---------------------------------------------------------------------------

/// A 2-D path storing a sequence of draw commands.
///
/// Use [`PathBuilder`] (or the inherent builder methods) to populate a path,
/// then call [`Path::fill`] or [`Path::stroke`] to rasterise it.
#[derive(Clone, Debug, Default)]
pub struct Path {
    cmds: Vec<PathCmd>,
    fill_rule: FillRule,
}

impl Path {
    /// Create an empty path with the default (non-zero) fill rule.
    pub fn new() -> Self {
        Self {
            cmds: Vec::new(),
            fill_rule: FillRule::default(),
        }
    }

    /// Set the fill rule (consumes and returns `self` for chaining).
    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.fill_rule = rule;
        self
    }

    /// Move the current point to `p` without drawing.
    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.cmds.push(PathCmd::MoveTo(p));
        self
    }

    /// Add a straight line from the current point to `p`.
    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.cmds.push(PathCmd::LineTo(p));
        self
    }

    /// Add a quadratic Bézier curve with control point `ctrl` ending at `end`.
    pub fn quad_to(&mut self, ctrl: Point, end: Point) -> &mut Self {
        self.cmds.push(PathCmd::QuadTo(ctrl, end));
        self
    }

    /// Add a cubic Bézier curve with control points `c1`/`c2` ending at `end`.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> &mut Self {
        self.cmds.push(PathCmd::CubicTo(c1, c2, end));
        self
    }

    /// Close the current sub-path (draw a line back to the last `MoveTo`).
    pub fn close(&mut self) -> &mut Self {
        self.cmds.push(PathCmd::Close);
        self
    }

    // -----------------------------------------------------------------------
    // Flattening
    // -----------------------------------------------------------------------

    /// Flatten all sub-paths to sequences of line vertices.
    ///
    /// Returns one `Vec<Point>` per sub-path (contour).  The last point of
    /// each contour is not duplicated at the start; callers should treat the
    /// contour as closed (connect last → first).
    ///
    /// `tolerance` is the maximum chord-distance deviation allowed per Bézier
    /// segment (in pixels). A value of `0.25` gives visually smooth curves.
    pub fn flatten(&self, tolerance: f32) -> Vec<Vec<Point>> {
        let tol = tolerance.max(0.01);
        let mut contours: Vec<Vec<Point>> = Vec::new();
        let mut current: Vec<Point> = Vec::new();
        let mut start: Option<Point> = None;
        let mut cursor: Point = (0.0, 0.0);

        for cmd in &self.cmds {
            match *cmd {
                PathCmd::MoveTo(p) => {
                    if !current.is_empty() {
                        contours.push(core::mem::take(&mut current));
                    }
                    start = Some(p);
                    cursor = p;
                    current.push(p);
                }
                PathCmd::LineTo(p) => {
                    current.push(p);
                    cursor = p;
                }
                PathCmd::QuadTo(ctrl, end) => {
                    flatten_quad(cursor, ctrl, end, tol, &mut current);
                    cursor = end;
                }
                PathCmd::CubicTo(c1, c2, end) => {
                    flatten_cubic(cursor, c1, c2, end, tol, &mut current);
                    cursor = end;
                }
                PathCmd::Close => {
                    if let Some(s) = start {
                        if let Some(&last) = current.last() {
                            if dist2(last, s) > f32::EPSILON {
                                current.push(s);
                            }
                        }
                    }
                    if !current.is_empty() {
                        contours.push(core::mem::take(&mut current));
                    }
                    if let Some(s) = start {
                        cursor = s;
                    }
                }
            }
        }
        if !current.is_empty() {
            contours.push(current);
        }
        contours
    }

    // -----------------------------------------------------------------------
    // Fill
    // -----------------------------------------------------------------------

    /// Fill this path's interior into `fb` using the configured fill rule.
    ///
    /// A single [`Rasterizer`] is created for the duration of this call so
    /// that scratch buffers are reused across all contours of the path.
    pub fn fill(&self, fb: &mut Framebuffer, color: Color) {
        let tolerance = 0.25_f32;
        let contours = self.flatten(tolerance);
        let mut ras = Rasterizer::new();
        for contour in contours {
            if contour.len() >= 3 {
                ras.fill_polygon(fb, &contour, color, self.fill_rule, true);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Stroke
    // -----------------------------------------------------------------------

    /// Stroke this path into `fb` using the given style.
    ///
    /// Each sub-path is stroked independently; closed sub-paths produce
    /// closed outlines (the cap style is ignored for closed paths).
    ///
    /// A single [`Rasterizer`] is created for the duration of this call so
    /// that scratch buffers are reused across all contours.
    pub fn stroke(&self, fb: &mut Framebuffer, style: &StrokeStyle, color: Color) {
        let half = (style.width * 0.5).max(0.5);
        let tolerance = 0.25_f32;
        let contours = self.flatten(tolerance);
        let mut ras = Rasterizer::new();
        for contour in &contours {
            if contour.len() < 2 {
                continue;
            }
            stroke_contour_with_ras(fb, contour, half, style, color, &mut ras);
        }
    }

    /// Fill this path's interior, clipped to `clip`.
    ///
    /// Uses `fill_polygon_clipped` so pixels outside `clip` are never written.
    /// Anti-aliasing is enabled by default.
    pub fn fill_clipped(&self, fb: &mut Framebuffer, color: Color, clip: crate::clip::ClipRect) {
        self.fill_clipped_aa(fb, color, clip, true);
    }

    /// Stroke this path, clipped to `clip`.
    ///
    /// Constructs the stroke polygon and fills it using `fill_polygon_clipped`.
    pub fn stroke_clipped(
        &self,
        fb: &mut Framebuffer,
        style: &StrokeStyle,
        color: Color,
        clip: crate::clip::ClipRect,
    ) {
        self.stroke_clipped_aa(fb, style, color, clip, true);
    }

    /// Fill this path's interior, clipped to `clip`, with explicit AA control.
    ///
    /// When `aa` is `false`, edges are aliased (faster; no coverage blending).
    ///
    /// A single [`Rasterizer`] is created for the duration of this call so
    /// that scratch buffers are reused across all contours.
    pub fn fill_clipped_aa(
        &self,
        fb: &mut Framebuffer,
        color: Color,
        clip: crate::clip::ClipRect,
        aa: bool,
    ) {
        let tolerance = 0.25_f32;
        let contours = self.flatten(tolerance);
        let mut ras = Rasterizer::new();
        for contour in contours {
            if contour.len() >= 3 {
                ras.fill_polygon_clipped(fb, &contour, color, self.fill_rule, aa, clip);
            }
        }
    }

    /// Stroke this path, clipped to `clip`, with explicit AA control.
    ///
    /// When `aa` is `false`, the stroke polygon edges are aliased.
    ///
    /// A single [`Rasterizer`] is created for the duration of this call so
    /// that scratch buffers are reused across all contours.
    pub fn stroke_clipped_aa(
        &self,
        fb: &mut Framebuffer,
        style: &StrokeStyle,
        color: Color,
        clip: crate::clip::ClipRect,
        aa: bool,
    ) {
        let half = (style.width * 0.5).max(0.5);
        let tolerance = 0.25_f32;
        let contours = self.flatten(tolerance);
        let mut ras = Rasterizer::new();
        for contour in &contours {
            if contour.len() < 2 {
                continue;
            }
            stroke_contour_clipped_inner_with_ras(
                fb, contour, half, style, color, clip, aa, &mut ras,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// PathBuilder — fluent constructor
// ---------------------------------------------------------------------------

/// Fluent builder for [`Path`].
///
/// ```rust
/// use oxiui_render_soft::path::PathBuilder;
/// use oxiui_render_soft::FillRule;
/// let path = PathBuilder::new()
///     .move_to((10.0, 10.0))
///     .line_to((50.0, 10.0))
///     .line_to((50.0, 50.0))
///     .close()
///     .fill_rule(FillRule::EvenOdd)
///     .build();
/// ```
#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    path: Path,
}

impl PathBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Move to `p`.
    pub fn move_to(mut self, p: Point) -> Self {
        self.path.move_to(p);
        self
    }

    /// Line to `p`.
    pub fn line_to(mut self, p: Point) -> Self {
        self.path.line_to(p);
        self
    }

    /// Quadratic Bézier to `end` via `ctrl`.
    pub fn quad_to(mut self, ctrl: Point, end: Point) -> Self {
        self.path.quad_to(ctrl, end);
        self
    }

    /// Cubic Bézier to `end` via `c1`, `c2`.
    pub fn cubic_to(mut self, c1: Point, c2: Point, end: Point) -> Self {
        self.path.cubic_to(c1, c2, end);
        self
    }

    /// Close the current sub-path.
    pub fn close(mut self) -> Self {
        self.path.close();
        self
    }

    /// Set the fill rule.
    pub fn fill_rule(mut self, rule: FillRule) -> Self {
        self.path.fill_rule = rule;
        self
    }

    /// Consume the builder and return the finished [`Path`].
    pub fn build(self) -> Path {
        self.path
    }
}

// ---------------------------------------------------------------------------
// Bézier flattening (De Casteljau)
// ---------------------------------------------------------------------------

/// Squared Euclidean distance between two points.
#[inline]
fn dist2(a: Point, b: Point) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    dx * dx + dy * dy
}

/// Midpoint of a segment.
#[inline]
fn mid(a: Point, b: Point) -> Point {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

/// Adaptive De Casteljau flattening for a quadratic Bézier.
///
/// Appends intermediate points to `out`; does NOT re-push `p0`.
pub fn flatten_quad(p0: Point, p1: Point, p2: Point, tol: f32, out: &mut Vec<Point>) {
    // Chord-deviation test: if the control point is close enough to the chord, done.
    let chord_dev = {
        let mx = (p0.0 + p2.0) * 0.5;
        let my = (p0.1 + p2.1) * 0.5;
        let dx = p1.0 - mx;
        let dy = p1.1 - my;
        dx * dx + dy * dy
    };
    if chord_dev <= tol * tol * 4.0 {
        // Close enough — emit endpoint.
        out.push(p2);
        return;
    }
    // Subdivide at t = 0.5.
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let r0 = mid(q0, q1);
    flatten_quad(p0, q0, r0, tol, out);
    flatten_quad(r0, q1, p2, tol, out);
}

/// Adaptive De Casteljau flattening for a cubic Bézier.
///
/// Appends intermediate points to `out`; does NOT re-push `p0`.
pub fn flatten_cubic(p0: Point, p1: Point, p2: Point, p3: Point, tol: f32, out: &mut Vec<Point>) {
    // Chord-deviation test.
    let chord_dev = {
        // For cubic: the max deviation from the chord [p0, p3] is bounded by
        // 3/4 * max(|p1 - chord(t1)|, |p2 - chord(t2)|).
        // A simpler conservative bound: check both hull diagonals.
        let d1 = {
            let mx = (2.0 * p0.0 + p3.0) / 3.0;
            let my = (2.0 * p0.1 + p3.1) / 3.0;
            let dx = p1.0 - mx;
            let dy = p1.1 - my;
            dx * dx + dy * dy
        };
        let d2 = {
            let mx = (p0.0 + 2.0 * p3.0) / 3.0;
            let my = (p0.1 + 2.0 * p3.1) / 3.0;
            let dx = p2.0 - mx;
            let dy = p2.1 - my;
            dx * dx + dy * dy
        };
        d1.max(d2)
    };
    if chord_dev <= tol * tol {
        out.push(p3);
        return;
    }
    // Subdivide at t = 0.5.
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let q2 = mid(p2, p3);
    let r0 = mid(q0, q1);
    let r1 = mid(q1, q2);
    let s0 = mid(r0, r1);
    flatten_cubic(p0, q0, r0, s0, tol, out);
    flatten_cubic(s0, r1, q2, p3, tol, out);
}

// ---------------------------------------------------------------------------
// Stroke helper
// ---------------------------------------------------------------------------

/// Compute the unit normal (perpendicular) to a segment, scaled by `half_w`.
#[inline]
fn normal(a: Point, b: Point, half_w: f32) -> (f32, f32) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < f32::EPSILON {
        return (0.0, half_w);
    }
    let nx = -dy / len * half_w;
    let ny = dx / len * half_w;
    (nx, ny)
}

/// Offset a point by a normal vector.
#[inline]
fn offset(p: Point, n: (f32, f32)) -> Point {
    (p.0 + n.0, p.1 + n.1)
}

/// Offset a point by the negative of a normal vector.
#[inline]
fn offset_neg(p: Point, n: (f32, f32)) -> Point {
    (p.0 - n.0, p.1 - n.1)
}

/// Stroke a single (possibly closed) contour, filling the resulting polygon
/// via a caller-supplied `Rasterizer` for scratch reuse.
fn stroke_contour_with_ras(
    fb: &mut Framebuffer,
    pts: &[Point],
    half_w: f32,
    style: &StrokeStyle,
    color: Color,
    ras: &mut Rasterizer,
) {
    let poly = build_stroke_poly(pts, half_w, style);
    if poly.len() >= 3 {
        ras.fill_polygon(fb, &poly, color, FillRule::NonZero, true);
    }
}

/// Build the parallel-offset stroke polygon for a single contour.
///
/// Returns the stroke polygon as a `Vec<Point>` (left side forward + right
/// side reversed). Returns an empty `Vec` if the contour is too short.
fn build_stroke_poly(pts: &[Point], half_w: f32, style: &StrokeStyle) -> Vec<Point> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let closed = dist2(pts[0], pts[n - 1]) < 1e-4;
    let effective_n = if closed { n - 1 } else { n };
    if effective_n < 2 {
        return Vec::new();
    }

    let seg_count = effective_n - 1;
    let mut normals: Vec<(f32, f32)> = Vec::with_capacity(seg_count);
    for i in 0..seg_count {
        normals.push(normal(pts[i], pts[i + 1], half_w));
    }

    let mut left: Vec<Point> = Vec::with_capacity(effective_n + 8);
    let mut right: Vec<Point> = Vec::with_capacity(effective_n + 8);

    // --- Start cap ---
    if !closed {
        let n0 = normals[0];
        match style.cap {
            Cap::Butt => {
                left.push(offset(pts[0], n0));
                right.push(offset_neg(pts[0], n0));
            }
            Cap::Square => {
                let dir = direction(pts[0], pts[1], half_w);
                left.push(offset(offset_neg(pts[0], dir), n0));
                right.push(offset_neg(offset_neg(pts[0], dir), n0));
            }
            Cap::Round => {
                add_round_cap(&mut left, pts[0], n0, true);
                right.push(offset_neg(pts[0], n0));
            }
        }
    } else {
        let n_last = normals[seg_count - 1];
        let n_first = normals[0];
        if style.join == Join::Round {
            add_round_join(&mut left, &mut right, pts[0], n_last, n_first);
        } else {
            let (jl, jr) = compute_join(pts[0], n_last, n_first, style.join, style.miter_limit);
            left.push(jl);
            right.push(jr);
        }
    }

    // --- Interior vertices ---
    for i in 1..effective_n - 1 {
        let n_prev = normals[i - 1];
        let n_next = normals[i];
        match style.join {
            Join::Round => {
                add_round_join(&mut left, &mut right, pts[i], n_prev, n_next);
            }
            _ => {
                let (jl, jr) = compute_join(pts[i], n_prev, n_next, style.join, style.miter_limit);
                left.push(jl);
                right.push(jr);
            }
        }
    }

    // --- End point ---
    if !closed {
        let n_last = normals[seg_count - 1];
        let end = pts[effective_n - 1];
        match style.cap {
            Cap::Butt => {
                left.push(offset(end, n_last));
                right.push(offset_neg(end, n_last));
            }
            Cap::Square => {
                let dir = direction(pts[effective_n - 2], end, half_w);
                left.push(offset(offset(end, dir), n_last));
                right.push(offset_neg(offset(end, dir), n_last));
            }
            Cap::Round => {
                left.push(offset(end, n_last));
                add_round_cap(&mut right, end, n_last, false);
            }
        }
    } else {
        let n_prev = normals[seg_count - 1];
        let n_first = normals[0];
        if style.join == Join::Round {
            add_round_join(&mut left, &mut right, pts[effective_n - 1], n_prev, n_first);
        } else {
            let (jl, jr) = compute_join(
                pts[effective_n - 1],
                n_prev,
                n_first,
                style.join,
                style.miter_limit,
            );
            left.push(jl);
            right.push(jr);
        }
    }

    // Combine left (forward) + right (reversed) into a single polygon.
    let mut poly: Vec<Point> = Vec::with_capacity(left.len() + right.len());
    poly.extend_from_slice(&left);
    for &p in right.iter().rev() {
        poly.push(p);
    }
    poly
}

/// Stroke a single contour, clipped to `clip`, with explicit AA flag and
/// scratch-reuse via a caller-provided `Rasterizer`.
#[allow(clippy::too_many_arguments)]
fn stroke_contour_clipped_inner_with_ras(
    fb: &mut Framebuffer,
    pts: &[Point],
    half_w: f32,
    style: &StrokeStyle,
    color: Color,
    clip: crate::clip::ClipRect,
    aa: bool,
    ras: &mut Rasterizer,
) {
    let poly = build_stroke_poly(pts, half_w, style);
    if poly.len() >= 3 {
        ras.fill_polygon_clipped(fb, &poly, color, FillRule::NonZero, aa, clip);
    }
}

/// Compute the direction unit vector of a segment, scaled by `scale`.
#[inline]
fn direction(a: Point, b: Point, scale: f32) -> (f32, f32) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < f32::EPSILON {
        return (scale, 0.0);
    }
    (dx / len * scale, dy / len * scale)
}

/// Compute the left and right join vertices at an interior joint, using the
/// configured join style.
///
/// Returns `(left_offset, right_offset)` where "left" is the offset in the
/// positive normal direction and "right" is in the negative direction.
///
/// Note: `Join::Round` is handled separately in `stroke_contour` via
/// `add_round_join` (which inserts a full arc fan). This function is used for
/// `Miter` and `Bevel` joins only.
fn compute_join(
    pt: Point,
    n_prev: (f32, f32),
    n_next: (f32, f32),
    join: Join,
    miter_limit: f32,
) -> (Point, Point) {
    match join {
        Join::Bevel | Join::Round => (offset(pt, n_next), offset_neg(pt, n_next)),
        Join::Miter => miter_join(pt, n_prev, n_next, miter_limit),
    }
}

/// Compute a miter join with limit fallback to bevel.
fn miter_join(
    pt: Point,
    n_prev: (f32, f32),
    n_next: (f32, f32),
    miter_limit: f32,
) -> (Point, Point) {
    // Average the two normals (both point to the "outside" of their segments).
    let avg_x = (n_prev.0 + n_next.0) * 0.5;
    let avg_y = (n_prev.1 + n_next.1) * 0.5;
    let len_sq = avg_x * avg_x + avg_y * avg_y;
    if len_sq < f32::EPSILON {
        // Normals cancel (180° turn) → bevel.
        return (offset(pt, n_next), offset_neg(pt, n_next));
    }
    // The miter offset length equals half_w / |avg| (from geometry).
    // |avg|² = len_sq; we want to scale avg so that |result| = half_w/sin(θ/2)
    // where the half_w is already encoded in n_prev/n_next magnitude.
    let scale = 1.0 / len_sq;
    // half_w magnitude from n_prev.
    let half_w_sq = n_prev.0 * n_prev.0 + n_prev.1 * n_prev.1;
    let miter_len_sq = half_w_sq * scale;
    // Miter limit check: if miter_len / half_w > miter_limit → bevel.
    if miter_len_sq > miter_limit * miter_limit * half_w_sq {
        return (offset(pt, n_next), offset_neg(pt, n_next));
    }
    let mx = avg_x * scale * half_w_sq;
    let my = avg_y * scale * half_w_sq;
    let left = (pt.0 + mx, pt.1 + my);
    let right = (pt.0 - mx, pt.1 - my);
    (left, right)
}

/// Insert arc-fan points for a round join at vertex `pt`.
///
/// The join fans between `n_prev` (end of the previous segment's normal) and
/// `n_next` (start of the next segment's normal). The outer side receives the
/// fan; the inner side receives a single miter-like point at the intersection.
fn add_round_join(
    left: &mut Vec<Point>,
    right: &mut Vec<Point>,
    pt: Point,
    n_prev: (f32, f32),
    n_next: (f32, f32),
) {
    let half_w = (n_prev.0 * n_prev.0 + n_prev.1 * n_prev.1).sqrt();
    if half_w < f32::EPSILON {
        return;
    }
    // Cross product sign determines whether the turn is left or right.
    // n_prev and n_next are "left" normals (perpendicular pointing left of travel).
    // cross = n_prev.x * n_next.y - n_prev.y * n_next.x
    let cross = n_prev.0 * n_next.1 - n_prev.1 * n_next.0;

    // Start and end angles of the arc.
    let start_angle = n_prev.1.atan2(n_prev.0);
    let end_angle = n_next.1.atan2(n_next.0);

    // Sweep the shorter arc.
    let mut sweep = end_angle - start_angle;
    // Normalise sweep to [-π, π].
    while sweep > std::f32::consts::PI {
        sweep -= 2.0 * std::f32::consts::PI;
    }
    while sweep < -std::f32::consts::PI {
        sweep += 2.0 * std::f32::consts::PI;
    }

    const STEPS: usize = 8;
    let fan_pts: Vec<Point> = (0..=STEPS)
        .map(|step| {
            let a = start_angle + sweep * (step as f32 / STEPS as f32);
            (pt.0 + a.cos() * half_w, pt.1 + a.sin() * half_w)
        })
        .collect();

    if cross >= 0.0 {
        // Left turn: the left side is the outer side (gets the fan).
        for &p in &fan_pts {
            left.push(p);
        }
        // Right (inner) side: single bevel point.
        right.push(offset_neg(pt, n_next));
    } else {
        // Right turn: the right side is the outer (gets the negated fan).
        left.push(offset(pt, n_next));
        for &p in fan_pts.iter().rev() {
            // Mirror the fan to the right (negative normal).
            let dx = p.0 - pt.0;
            let dy = p.1 - pt.1;
            right.push((pt.0 - dx, pt.1 - dy));
        }
    }
}

/// Add semicircle fan points for a round cap. Appends to `target`.
fn add_round_cap(target: &mut Vec<Point>, center: Point, n: (f32, f32), forward: bool) {
    // Approximate semicircle with 8 steps.
    const STEPS: usize = 8;
    let (nx, ny) = n;
    let half_w = (nx * nx + ny * ny).sqrt();
    if half_w < f32::EPSILON {
        return;
    }
    let ux = nx / half_w;
    let uy = ny / half_w;
    // Start angle: the normal direction (pointing left of travel).
    // We sweep 180° (π) for the cap.
    let start_angle = uy.atan2(ux);
    let sign = if forward { 1.0_f32 } else { -1.0_f32 };
    for i in 0..=STEPS {
        let a = start_angle + sign * std::f32::consts::PI * (i as f32 / STEPS as f32);
        let px = center.0 + a.cos() * half_w;
        let py = center.1 + a.sin() * half_w;
        target.push((px, py));
    }
}

// ---------------------------------------------------------------------------
// Convenience flattening public API
// ---------------------------------------------------------------------------

/// Flatten a quadratic Bézier into a sequence of points (including endpoints).
pub fn flatten_quad_bezier(p0: Point, p1: Point, p2: Point, tolerance: f32) -> Vec<Point> {
    let mut pts = vec![p0];
    flatten_quad(p0, p1, p2, tolerance.max(0.01), &mut pts);
    pts
}

/// Flatten a cubic Bézier into a sequence of points (including endpoints).
pub fn flatten_cubic_bezier(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tolerance: f32,
) -> Vec<Point> {
    let mut pts = vec![p0];
    flatten_cubic(p0, p1, p2, p3, tolerance.max(0.01), &mut pts);
    pts
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
    fn bezier_flatten_endpoints() {
        // Flattened cubic must start at p0 and end at p3.
        let p0 = (0.0f32, 0.0);
        let p1 = (10.0, 20.0);
        let p2 = (20.0, -10.0);
        let p3 = (30.0, 0.0);
        let pts = flatten_cubic_bezier(p0, p1, p2, p3, 0.25);
        assert!(pts.len() >= 2);
        let first = pts[0];
        let last = *pts.last().expect("at least one point");
        assert!((first.0 - p0.0).abs() < 0.01 && (first.1 - p0.1).abs() < 0.01);
        assert!((last.0 - p3.0).abs() < 0.01 && (last.1 - p3.1).abs() < 0.01);
    }

    #[test]
    fn quad_flatten_endpoints() {
        let p0 = (0.0f32, 0.0);
        let p1 = (5.0, 10.0);
        let p2 = (10.0, 0.0);
        let pts = flatten_quad_bezier(p0, p1, p2, 0.25);
        let last = *pts.last().expect("at least one point");
        assert!((last.0 - p2.0).abs() < 0.01 && (last.1 - p2.1).abs() < 0.01);
    }

    #[test]
    fn path_fill_triangle() {
        let mut fb = fresh(20, 20);
        let mut path = Path::new();
        path.move_to((0.0, 0.0))
            .line_to((20.0, 0.0))
            .line_to((10.0, 20.0))
            .close();
        path.fill(&mut fb, Color(255, 0, 0, 255));
        // Centre of triangle should be painted.
        let (r, _, _, _) = fb.get_rgba(10, 10).unwrap_or((0, 0, 0, 0));
        assert!(r > 0, "centre should be painted");
    }

    #[test]
    fn path_fill_rule_cases() {
        // Self-intersecting 5-pointed star via Path:
        // EvenOdd leaves the center pentagon as a hole; NonZero fills it.
        // Use a large star so arms are wide enough to hit individual pixels.
        let cx = 35.0f32;
        let cy = 35.0f32;
        let r = 30.0f32;
        let star_pts: Vec<(f32, f32)> = (0..5)
            .map(|i| {
                let angle = std::f32::consts::PI * (2.0 * (2 * i) as f32 / 5.0 - 0.5);
                (cx + r * angle.cos(), cy + r * angle.sin())
            })
            .collect();

        let build_star = |rule: FillRule| {
            let mut p = Path::new().with_fill_rule(rule);
            p.move_to(star_pts[0]);
            for &pt in &star_pts[1..] {
                p.line_to(pt);
            }
            p.close();
            p
        };

        let path_eo = build_star(FillRule::EvenOdd);
        let path_nz = build_star(FillRule::NonZero);

        let mut fb_eo = fresh(70, 70);
        let mut fb_nz = fresh(70, 70);
        path_eo.fill(&mut fb_eo, Color(255, 0, 0, 255));
        path_nz.fill(&mut fb_nz, Color(255, 0, 0, 255));

        // Both should paint a star arm.
        let (r_eo_tip, _, _, _) = fb_eo.get_rgba(35, 8).unwrap_or((0, 0, 0, 0));
        assert!(r_eo_tip > 0, "EvenOdd: star arm should be painted");

        // NonZero: center should be filled.
        let (r_nz_ctr, _, _, _) = fb_nz.get_rgba(35, 35).unwrap_or((0, 0, 0, 0));
        assert!(r_nz_ctr > 0, "NonZero: star center should be filled");

        // EvenOdd: center should be a hole.
        let (r_eo_ctr, _, _, _) = fb_eo.get_rgba(35, 35).unwrap_or((0, 0, 0, 0));
        assert_eq!(
            r_eo_ctr, 0,
            "EvenOdd: star center should be a hole (r={r_eo_ctr})"
        );
    }

    #[test]
    fn path_builder_works() {
        let path = PathBuilder::new()
            .move_to((0.0, 0.0))
            .line_to((10.0, 0.0))
            .line_to((5.0, 10.0))
            .close()
            .build();
        let mut fb = fresh(15, 15);
        path.fill(&mut fb, Color(0, 255, 0, 255));
        let (_, g, _, _) = fb.get_rgba(5, 3).unwrap_or((0, 0, 0, 0));
        assert!(g > 0, "builder path: interior should be green");
    }

    #[test]
    fn path_stroke_produces_pixels() {
        let mut fb = fresh(30, 30);
        let mut path = Path::new();
        path.move_to((5.0, 15.0)).line_to((25.0, 15.0));
        let style = StrokeStyle {
            width: 4.0,
            join: Join::Miter,
            cap: Cap::Butt,
            miter_limit: 4.0,
        };
        path.stroke(&mut fb, &style, Color(0, 0, 255, 255));
        // Some pixel along the stroke should be painted blue.
        let mut found = false;
        for x in 0..30 {
            let (_, _, b, _) = fb.get_rgba(x, 15).unwrap_or((0, 0, 0, 0));
            if b > 0 {
                found = true;
                break;
            }
        }
        assert!(found, "stroke should produce blue pixels along y=15");
    }

    #[test]
    fn round_join_differs_from_bevel() {
        // Verify that Round join produces a distinct rendering from Bevel at a 90° corner.
        // Round inserts an arc fan; Bevel cuts with a flat edge. The two produce different
        // pixel distributions in the corner region.
        let mut fb_round = fresh(40, 40);
        let mut fb_bevel = fresh(40, 40);

        let mut path_r = Path::new();
        path_r
            .move_to((5.0, 20.0))
            .line_to((20.0, 20.0))
            .line_to((20.0, 5.0));

        let mut path_b = Path::new();
        path_b
            .move_to((5.0, 20.0))
            .line_to((20.0, 20.0))
            .line_to((20.0, 5.0));

        let style_round = StrokeStyle {
            width: 6.0,
            join: Join::Round,
            cap: Cap::Butt,
            miter_limit: 4.0,
        };
        let style_bevel = StrokeStyle {
            width: 6.0,
            join: Join::Bevel,
            cap: Cap::Butt,
            miter_limit: 4.0,
        };

        path_r.stroke(&mut fb_round, &style_round, Color(255, 0, 0, 255));
        path_b.stroke(&mut fb_bevel, &style_bevel, Color(255, 0, 0, 255));

        // Both should paint some pixels.
        let count = |fb: &Framebuffer| -> u32 {
            (0..40u32)
                .flat_map(|y| (0..40u32).map(move |x| (x, y)))
                .filter(|&(x, y)| fb.get_rgba(x, y).is_some_and(|(r, _, _, _)| r > 0))
                .count() as u32
        };
        let round_px = count(&fb_round);
        let bevel_px = count(&fb_bevel);
        assert!(
            round_px > 0,
            "Round join should produce some pixels (got {round_px})"
        );
        assert!(
            bevel_px > 0,
            "Bevel join should produce some pixels (got {bevel_px})"
        );
        // The pixel counts differ (different corner treatment).
        assert_ne!(
            round_px, bevel_px,
            "Round and Bevel joins should differ in pixel count (both={round_px})"
        );
    }

    #[test]
    fn miter_limit_prevents_spike() {
        // A very acute join should fall back to bevel (no infinite spike).
        // We just verify the function doesn't panic and returns finite values.
        let pt = (10.0f32, 10.0);
        let n_prev = (0.0f32, 2.0); // pointing up
        let n_next = (0.0f32, -2.0); // pointing down (180° → degenerate)
        let (left, right) = miter_join(pt, n_prev, n_next, 4.0);
        assert!(left.0.is_finite() && left.1.is_finite());
        assert!(right.0.is_finite() && right.1.is_finite());
    }
}
