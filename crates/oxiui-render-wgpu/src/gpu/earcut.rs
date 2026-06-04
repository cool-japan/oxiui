//! Ear-clipping triangulator for the fill tessellation pipeline.
//!
//! # Overview
//!
//! Converts a list of 2D contours (each a closed polygon) into a flat list of
//! triangles, correctly handling:
//! - Concave (non-convex) single polygons.
//! - Multi-contour shapes with holes (donut/glyph outlines).
//! - Both [`FillRule::NonZero`] and [`FillRule::EvenOdd`].
//!
//! # Public entry point
//!
//! ```text
//! triangulate(contours, fill_rule) -> Vec<[[f32;2];3]>
//! ```
//!
//! # Algorithm stages
//!
//! 1. **Contour preparation** — remove duplicate closing vertex, discard degenerate contours.
//! 2. **Single-contour fast path** — skip nesting analysis; ear-clip directly.
//! 3. **Multi-contour nesting** — build a parent forest by containment + depth.
//! 4. **Fill-rule resolution** — decide solid vs. hole per contour.
//! 5. **Hole bridging** — splice each hole into its outer ring via a bridge edge.
//! 6. **Ear clipping** — O(n²) with a fan fallback for degenerate input.

use oxiui_core::paint::FillRule;

// ── Public API ────────────────────────────────────────────────────────────────

/// Triangulate a list of closed contours into a flat triangle list.
///
/// Each contour is a slice of 2-D points in screen space.  Contours need not
/// be explicitly closed (the first and last point need not match) — duplicate
/// closing vertices are stripped during preparation.
///
/// Returns a `Vec` of `[[x,y];3]` triangles in *winding order preserved* form
/// (each triangle inherits the orientation established by ear clipping, which
/// operates on a CCW-normalised polygon).
pub fn triangulate(contours: &[Vec<[f32; 2]>], fill_rule: FillRule) -> Vec<[[f32; 2]; 3]> {
    // ── Step A: prepare contours ──────────────────────────────────────────────
    let prepared: Vec<Vec<[f32; 2]>> = contours.iter().filter_map(|c| prepare_contour(c)).collect();

    if prepared.is_empty() {
        return Vec::new();
    }

    // ── Step B: single-contour fast path ─────────────────────────────────────
    if prepared.len() == 1 {
        return ear_clip_simple(&prepared[0]);
    }

    // ── Step C: multi-contour nesting + fill-rule ─────────────────────────────
    let n = prepared.len();
    // Signed areas (shoelace).
    let areas: Vec<f64> = prepared.iter().map(|c| signed_area(c)).collect();
    // Parent index (None = root).
    let parents: Vec<Option<usize>> = compute_parents(&prepared);
    // Nesting depths.
    let depths: Vec<usize> = compute_depths(&parents, n);

    // Determine which contours are "solid" (outer ring of a filled region)
    // vs "hole" (inner ring that must be subtracted).
    let solid_flags: Vec<bool> = (0..n)
        .map(|i| is_solid(i, &depths, &areas, fill_rule, &parents))
        .collect();

    // Collect (outer_idx, [hole_indices]) pairs.
    let regions = collect_regions(n, &solid_flags, &parents, &depths);

    // ── Steps D + E: bridge holes + ear-clip each region ─────────────────────
    let mut out: Vec<[[f32; 2]; 3]> = Vec::new();
    for (outer_idx, hole_indices) in &regions {
        let outer = &prepared[*outer_idx];
        let holes: Vec<&Vec<[f32; 2]>> = hole_indices.iter().map(|&hi| &prepared[hi]).collect();
        let poly = bridge_holes(outer, &holes);
        let tris = ear_clip_simple(&poly);
        out.extend_from_slice(&tris);
    }

    out
}

// ── Contour preparation ───────────────────────────────────────────────────────

/// Prepare one contour for triangulation.
///
/// - Strip trailing duplicate of first point (if within 1e-4).
/// - Discard if fewer than 3 points remain.
/// - Discard if |area| < 1e-4 (degenerate / collinear).
fn prepare_contour(pts: &[[f32; 2]]) -> Option<Vec<[f32; 2]>> {
    if pts.is_empty() {
        return None;
    }
    let mut v: Vec<[f32; 2]> = pts.to_vec();
    // Remove duplicate closing vertex.
    if v.len() >= 2 {
        let first = v[0];
        let last = *v.last()?;
        let dx = first[0] - last[0];
        let dy = first[1] - last[1];
        if dx * dx + dy * dy < 1e-8 {
            v.pop();
        }
    }
    if v.len() < 3 {
        return None;
    }
    let area = signed_area(&v);
    if area.abs() < 1e-4 {
        return None;
    }
    Some(v)
}

// ── Signed area (shoelace) ────────────────────────────────────────────────────

/// Compute the signed area of a polygon via the shoelace formula.
///
/// Positive = CCW; negative = CW (in standard screen-Y-down coordinates this
/// is reversed from maths convention, but the sign convention is consistent
/// throughout this module).
fn signed_area(pts: &[[f32; 2]]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for i in 0..n {
        let j = (i + 1) % n;
        sum += (pts[i][0] as f64) * (pts[j][1] as f64);
        sum -= (pts[j][0] as f64) * (pts[i][1] as f64);
    }
    sum * 0.5
}

// ── Parent / depth computation ────────────────────────────────────────────────

/// For each contour, find the *immediate parent* = the smallest-area enclosing
/// contour (using the contour's first vertex as a representative point).
fn compute_parents(contours: &[Vec<[f32; 2]>]) -> Vec<Option<usize>> {
    let n = contours.len();
    let areas: Vec<f64> = contours.iter().map(|c| signed_area(c).abs()).collect();
    let mut parents: Vec<Option<usize>> = vec![None; n];

    for i in 0..n {
        let probe = contours[i][0];
        // Among all contours that contain `probe`, pick the smallest-area one.
        let mut best_area = f64::MAX;
        let mut best_idx: Option<usize> = None;

        for j in 0..n {
            if j == i {
                continue;
            }
            if point_in_polygon(probe, &contours[j]) {
                let a = areas[j];
                if a < best_area {
                    best_area = a;
                    best_idx = Some(j);
                }
            }
        }
        parents[i] = best_idx;
    }
    parents
}

/// Compute the nesting depth for each contour (depth = number of ancestors).
fn compute_depths(parents: &[Option<usize>], n: usize) -> Vec<usize> {
    let mut depths = vec![usize::MAX; n];
    for i in 0..n {
        if depths[i] == usize::MAX {
            fill_depth(i, parents, &mut depths);
        }
    }
    depths
}

/// Recursive depth fill (iterative to avoid stack overflow on deep nesting).
fn fill_depth(start: usize, parents: &[Option<usize>], depths: &mut [usize]) {
    // Walk the ancestor chain collecting unresolved indices.
    let mut chain: Vec<usize> = Vec::new();
    let mut cur = start;
    loop {
        if depths[cur] != usize::MAX {
            // Already resolved — propagate back.
            let base = depths[cur];
            for (k, &idx) in chain.iter().rev().enumerate() {
                depths[idx] = base + k + 1;
            }
            return;
        }
        chain.push(cur);
        match parents[cur] {
            None => {
                // Root: depth 0.
                let len = chain.len();
                for (k, &idx) in chain.iter().enumerate() {
                    depths[idx] = len - 1 - k;
                }
                return;
            }
            Some(p) => {
                cur = p;
            }
        }
    }
}

// ── Fill-rule resolution ──────────────────────────────────────────────────────

/// Returns `true` if contour `i` is a *solid* ring under `fill_rule`.
fn is_solid(
    i: usize,
    depths: &[usize],
    areas: &[f64],
    fill_rule: FillRule,
    parents: &[Option<usize>],
) -> bool {
    let depth = depths[i];
    match fill_rule {
        FillRule::EvenOdd => {
            // Even depth = solid, odd depth = hole.
            depth.is_multiple_of(2)
        }
        FillRule::NonZero => {
            // Walk the parent chain, summing orientation signs.
            // A contour is solid if the running winding ≠ 0.
            let winding = winding_sum(i, areas, parents);
            winding != 0
        }
    }
}

/// Sum orientation signs walking from contour `i` to the root.
fn winding_sum(i: usize, areas: &[f64], parents: &[Option<usize>]) -> i32 {
    let mut sum = 0i32;
    let mut cur = i;
    loop {
        let sign = if areas[cur] >= 0.0 { 1i32 } else { -1i32 };
        sum += sign;
        match parents[cur] {
            None => break,
            Some(p) => cur = p,
        }
    }
    sum
}

// ── Region collection ─────────────────────────────────────────────────────────

/// Collect (outer_idx, hole_indices) pairs for each solid outer ring.
///
/// A solid ring's direct solid-ring children at depth+2 are processed as
/// separate outer rings.  Direct *hole* children (depth+1, not solid) are
/// collected under this outer.
fn collect_regions(
    n: usize,
    solid_flags: &[bool],
    parents: &[Option<usize>],
    depths: &[usize],
) -> Vec<(usize, Vec<usize>)> {
    let mut regions: Vec<(usize, Vec<usize>)> = Vec::new();

    for i in 0..n {
        if !solid_flags[i] {
            continue;
        }
        // Check this solid ring is a "topmost" or "re-appearing" solid —
        // its parent (if any) is NOT solid.
        let is_outer = match parents[i] {
            None => true,
            Some(p) => !solid_flags[p],
        };
        if !is_outer {
            continue;
        }

        // Collect direct children that are not-solid (holes) for this outer.
        let my_depth = depths[i];
        let holes: Vec<usize> = (0..n)
            .filter(|&j| !solid_flags[j] && parents[j] == Some(i) && depths[j] == my_depth + 1)
            .collect();

        regions.push((i, holes));
    }

    // Degenerate fallback: if we found no regions at all, treat every
    // contour as its own standalone outer ring with no holes.
    if regions.is_empty() {
        for i in 0..n {
            regions.push((i, Vec::new()));
        }
    }

    regions
}

// ── Hole bridging ─────────────────────────────────────────────────────────────

/// Splice all `holes` into `outer` by successive bridge-edge insertion,
/// returning a single simple (possibly non-convex) polygon.
fn bridge_holes(outer: &[[f32; 2]], holes: &[&Vec<[f32; 2]>]) -> Vec<[f32; 2]> {
    if holes.is_empty() {
        return outer.to_vec();
    }

    // Ensure outer is CCW.
    let mut poly = ensure_ccw(outer.to_vec());

    // Splice holes one at a time.
    for &hole in holes {
        // Ensure hole is CW (so interior is solid when spliced into CCW outer).
        let hole_cw = ensure_cw(hole.to_vec());
        poly = splice_one_hole(&poly, &hole_cw);
    }

    poly
}

/// Splice a single CW hole into a CCW outer polygon.
///
/// Algorithm (§D in the spec):
/// 1. Find the hole vertex M with maximum x.
/// 2. Cast a +x ray from M; find the nearest outer edge intersected.
/// 3. Among visible outer vertices near the intersection, pick the best (V).
/// 4. Splice: outer[..V+1] + bridge + hole[M..] + hole[..M+1] + bridge + outer[V..].
fn splice_one_hole(outer: &[[f32; 2]], hole: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let m_idx = max_x_vertex(hole);
    let m = hole[m_idx];

    // Find nearest outer edge that the +x ray from M intersects.
    let bridge_result = find_bridge_vertex(m, outer);
    let v_idx = match bridge_result {
        Some(v) => v,
        None => {
            // Fallback: bridge to closest outer vertex.
            closest_vertex(m, outer)
        }
    };

    // Splice: build merged polygon.
    // Walk outer CCW starting just after V, return to V, then bridge to M,
    // walk hole starting at M (already CW) all the way around back to M,
    // then bridge back to V, and continue rest of outer.
    let on = outer.len();
    let hn = hole.len();
    let mut result = Vec::with_capacity(on + hn + 2);

    // outer[0..=v_idx]
    for &p in &outer[..=v_idx] {
        result.push(p);
    }
    // bridge to hole[m_idx], walk full hole (CW), return to m_idx
    for k in 0..=hn {
        result.push(hole[(m_idx + k) % hn]);
    }
    // bridge back to outer[v_idx]
    result.push(outer[v_idx]);
    // rest of outer: outer[v_idx+1..]
    for &p in &outer[v_idx + 1..] {
        result.push(p);
    }

    result
}

/// Find the index in `outer` of the best *visible* vertex from point `m`
/// along a +x ray.  Returns `None` if no outer edge is intersected.
fn find_bridge_vertex(m: [f32; 2], outer: &[[f32; 2]]) -> Option<usize> {
    let n = outer.len();
    let mx = m[0];
    let my = m[1];

    // Find nearest edge intersection along the +x ray.
    let mut nearest_x = f32::MAX;
    let mut nearest_edge_a: usize = n; // sentinel

    for i in 0..n {
        let j = (i + 1) % n;
        let ax = outer[i][0];
        let ay = outer[i][1];
        let bx = outer[j][0];
        let by = outer[j][1];

        // Does the horizontal ray y=my, x>mx cross edge (a,b)?
        let (min_y, max_y) = if ay <= by { (ay, by) } else { (by, ay) };
        if my < min_y || my >= max_y {
            continue; // Ray does not cross this edge's y range.
        }
        // x of intersection.
        let t = (my - ay) / (by - ay);
        let ix = ax + t * (bx - ax);
        if ix > mx && ix < nearest_x {
            nearest_x = ix;
            nearest_edge_a = i;
        }
    }

    if nearest_edge_a == n {
        return None; // No intersection found.
    }

    let ea = nearest_edge_a;
    let eb = (ea + 1) % n;

    // Among vertices visible from M within the triangle M–hit–V, pick
    // the one with the greatest x (rightmost, nearest to M).
    // Candidate: the edge endpoint with greater x.
    let va_x = outer[ea][0];
    let vb_x = outer[eb][0];
    let best_edge_v = if va_x >= vb_x { ea } else { eb };

    // Check whether any outer vertex lies strictly inside the triangle
    // (M, hit=(nearest_x,my), outer[best_edge_v]) with greater x than
    // best_edge_v — if so, prefer that vertex.
    let hit = [nearest_x, my];
    let v_pt = outer[best_edge_v];
    let mut best_v = best_edge_v;
    let mut best_x = v_pt[0];

    for (k, &p) in outer.iter().enumerate() {
        if k == ea || k == eb {
            continue;
        }
        if p[0] > mx && p[0] < best_x {
            // Only if it's strictly inside the triangle m–hit–v.
            if point_in_triangle(p, m, hit, v_pt) {
                best_v = k;
                best_x = p[0];
            }
        }
    }

    Some(best_v)
}

/// Index of the vertex in `pts` with the maximum x coordinate.
fn max_x_vertex(pts: &[[f32; 2]]) -> usize {
    let mut best = 0;
    let mut best_x = pts[0][0];
    for (i, &p) in pts.iter().enumerate().skip(1) {
        if p[0] > best_x {
            best_x = p[0];
            best = i;
        }
    }
    best
}

/// Index of the vertex in `pts` closest (Euclidean) to `m`.
fn closest_vertex(m: [f32; 2], pts: &[[f32; 2]]) -> usize {
    let mut best = 0;
    let mut best_d = f32::MAX;
    for (i, &p) in pts.iter().enumerate() {
        let dx = p[0] - m[0];
        let dy = p[1] - m[1];
        let d = dx * dx + dy * dy;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

// ── Orientation helpers ───────────────────────────────────────────────────────

/// Ensure the polygon is counter-clockwise (area > 0).
fn ensure_ccw(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// Ensure the polygon is clockwise (area < 0).
fn ensure_cw(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    if signed_area(&pts) > 0.0 {
        pts.reverse();
    }
    pts
}

// ── Ear clipping ──────────────────────────────────────────────────────────────

/// Ear-clip a simple polygon (no holes, no self-intersections) into triangles.
///
/// The polygon must have at least 3 vertices.  It is normalised to CCW
/// orientation before processing.
pub fn ear_clip_simple(pts: &[[f32; 2]]) -> Vec<[[f32; 2]; 3]> {
    if pts.len() < 3 {
        return Vec::new();
    }

    // Normalise to CCW.
    let verts = ensure_ccw(pts.to_vec());
    let n = verts.len();

    if n == 3 {
        return vec![[verts[0], verts[1], verts[2]]];
    }

    // Build doubly-linked circular index list.
    let mut prev: Vec<usize> = (0..n).map(|i| if i == 0 { n - 1 } else { i - 1 }).collect();
    let mut next: Vec<usize> = (0..n).map(|i| if i == n - 1 { 0 } else { i + 1 }).collect();
    let mut alive: Vec<bool> = vec![true; n];
    let mut remaining = n;

    // Precompute which vertices are reflex (cross product ≤ 0).
    let mut is_reflex: Vec<bool> = (0..n)
        .map(|i| {
            let p = prev[i];
            let nx = next[i];
            cross_2d(verts[p], verts[i], verts[nx]) <= 0.0
        })
        .collect();

    let mut out: Vec<[[f32; 2]; 3]> = Vec::with_capacity(n - 2);

    let cap = n * 2;
    let mut iters = 0usize;
    let mut cur = 0usize;

    while remaining > 3 {
        iters += 1;
        if iters > cap {
            // Safety net: fan-triangulate remaining vertices.
            fan_remaining(&verts, &alive, &mut out);
            return out;
        }

        // Find next alive vertex.
        if !alive[cur] {
            cur = next_alive(cur, &alive, &next, n);
        }

        let p = prev[cur];
        let nx = next[cur];

        if is_ear(cur, p, nx, &verts, &is_reflex) {
            // Emit triangle.
            out.push([verts[p], verts[cur], verts[nx]]);
            // Remove cur from the list.
            alive[cur] = false;
            next[p] = nx;
            prev[nx] = p;
            remaining -= 1;

            // Re-check convexity of prev and next.
            let pp = prev[p];
            let nnx = next[nx];
            is_reflex[p] = cross_2d(verts[pp], verts[p], verts[nx]) <= 0.0;
            is_reflex[nx] = cross_2d(verts[p], verts[nx], verts[nnx]) <= 0.0;

            // Continue from nx.
            cur = nx;
            iters = 0; // Reset iteration counter after a successful clip.
        } else {
            cur = next_alive(cur, &alive, &next, n);
        }
    }

    // Emit the final triangle.
    let (a, b, c) = last_three(&alive, &next, n);
    out.push([verts[a], verts[b], verts[c]]);

    out
}

/// Test whether vertex `cur` (with predecessor `p` and successor `nx`) is an ear.
///
/// A vertex is an ear if:
/// - The triangle (prev, cur, next) is convex (cross > 0).
/// - No other *reflex* vertex lies strictly inside the triangle.
fn is_ear(cur: usize, p: usize, nx: usize, verts: &[[f32; 2]], is_reflex: &[bool]) -> bool {
    let pv = verts[p];
    let cv = verts[cur];
    let nv = verts[nx];

    // Must be convex.
    if cross_2d(pv, cv, nv) <= 0.0 {
        return false;
    }

    // No reflex vertex strictly inside the triangle.
    for (k, &reflex) in is_reflex.iter().enumerate() {
        if !reflex {
            continue;
        }
        if k == p || k == cur || k == nx {
            continue;
        }
        if point_in_triangle_strict(verts[k], pv, cv, nv) {
            return false;
        }
    }

    true
}

/// Find the next alive vertex starting from (but not including) `start`.
fn next_alive(start: usize, alive: &[bool], next: &[usize], n: usize) -> usize {
    let mut cur = next[start];
    let mut limit = n;
    while !alive[cur] && limit > 0 {
        cur = next[cur];
        limit -= 1;
    }
    cur
}

/// Collect the three vertices that remain when `remaining == 3`.
fn last_three(alive: &[bool], next: &[usize], n: usize) -> (usize, usize, usize) {
    let a = alive.iter().position(|&b| b).unwrap_or(0);
    let b = next[a];
    let c = next[b];
    let _ = n;
    (a, b, c)
}

/// Fan-triangulate all alive vertices from the first alive vertex.
fn fan_remaining(verts: &[[f32; 2]], alive: &[bool], out: &mut Vec<[[f32; 2]; 3]>) {
    let alive_verts: Vec<[f32; 2]> = alive
        .iter()
        .enumerate()
        .filter_map(|(i, &a)| if a { Some(verts[i]) } else { None })
        .collect();
    if alive_verts.len() < 3 {
        return;
    }
    let p0 = alive_verts[0];
    for i in 1..alive_verts.len() - 1 {
        out.push([p0, alive_verts[i], alive_verts[i + 1]]);
    }
}

// ── Geometric primitives ──────────────────────────────────────────────────────

/// 2-D cross product of vectors (b-a) × (c-a).
///
/// Positive = left turn (CCW), negative = right turn (CW), zero = collinear.
#[inline]
fn cross_2d(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Test whether point `p` lies strictly inside or on the boundary of triangle (a, b, c),
/// returning `true` if the point would invalidate an ear.
///
/// For ear-clipping we treat "on the boundary" as an invalid ear as well, since
/// collinear / touching configurations can produce overlapping triangulation.
/// We use a small epsilon to make the boundary test robust to floating-point noise.
fn point_in_triangle_strict(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    const EPS: f32 = 1e-5;
    let d1 = cross_2d(a, b, p);
    let d2 = cross_2d(b, c, p);
    let d3 = cross_2d(c, a, p);

    // If all signs are the same (all >= -EPS for CCW triangle), the point is inside or on boundary.
    // We treat "on or inside" as invalidating the ear.
    let all_non_neg = d1 >= -EPS && d2 >= -EPS && d3 >= -EPS;
    let all_non_pos = d1 <= EPS && d2 <= EPS && d3 <= EPS;

    all_non_neg || all_non_pos
}

/// Test whether point `p` lies inside or on the boundary of triangle (a,b,c).
fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let d1 = cross_2d(a, b, p);
    let d2 = cross_2d(b, c, p);
    let d3 = cross_2d(c, a, p);

    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;

    !(has_neg && has_pos)
}

/// Ray-crossing point-in-polygon test (Jordan curve theorem).
///
/// Returns `true` if `pt` is strictly inside `polygon`.
fn point_in_polygon(pt: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let (px, py) = (pt[0], pt[1]);

    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (polygon[i][0], polygon[i][1]);
        let (xj, yj) = (polygon[j][0], polygon[j][1]);

        // Does the edge (xj,yj)–(xi,yi) cross the horizontal ray y=py from px rightward?
        let crosses_y = (yi > py) != (yj > py);
        if crosses_y {
            let x_intersect = (xj - xi) * (py - yi) / (yj - yi) + xi;
            if px < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }

    inside
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper: area of a triangle ────────────────────────────────────────────

    fn tri_area(t: &[[f32; 2]; 3]) -> f64 {
        let a = t[0];
        let b = t[1];
        let c = t[2];
        let area = (b[0] as f64 - a[0] as f64) * (c[1] as f64 - a[1] as f64)
            - (c[0] as f64 - a[0] as f64) * (b[1] as f64 - a[1] as f64);
        (area * 0.5).abs()
    }

    fn total_area(tris: &[[[f32; 2]; 3]]) -> f64 {
        tris.iter().map(tri_area).sum()
    }

    // ── Convex square ─────────────────────────────────────────────────────────

    #[test]
    fn convex_square_area_correct() {
        // CCW unit square [0,0]-[1,0]-[1,1]-[0,1] → area = 1.0.
        let sq = vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let tris = triangulate(&[sq], FillRule::NonZero);
        assert_eq!(tris.len(), 2, "square → 2 triangles");
        let area = total_area(&tris);
        assert!((area - 1.0).abs() < 1e-3, "area should be ~1.0, got {area}");
    }

    // ── Concave notch ─────────────────────────────────────────────────────────

    #[test]
    fn concave_notch_area_correct() {
        // L-shape: 6 vertices, area should be less than bounding box.
        let pts = vec![
            [0.0f32, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [5.0, 5.0], // concave notch
            [0.0, 10.0],
            [0.0, 0.0],
        ];
        let tris = triangulate(&[pts], FillRule::NonZero);
        assert!(!tris.is_empty(), "should produce triangles");
        let area = total_area(&tris);
        // Area should be ~75 (not the full 100×100 box).
        assert!(
            area > 30.0 && area < 100.0,
            "area should be in (30,100), got {area}"
        );
    }

    // ── Donut: outer CCW + inner CW ───────────────────────────────────────────

    #[test]
    fn donut_hole_correct_area() {
        // 10×10 outer (CCW), 2×2 inner (CW hole) centred.
        let outer = vec![[0.0f32, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        // CW inner (reversed from CCW).
        let inner = vec![[4.0f32, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0]];
        let tris = triangulate(&[outer, inner], FillRule::NonZero);
        assert!(!tris.is_empty());
        let area = total_area(&tris);
        // Should be ~96 (100 - 4).
        assert!(
            (area - 96.0).abs() < 2.0,
            "donut area should be ~96.0, got {area}"
        );
    }

    // ── EvenOdd vs NonZero differ ─────────────────────────────────────────────

    #[test]
    fn evenodd_vs_nonzero_differ() {
        // Two CCW squares nested: outer large, inner small (same winding).
        // NonZero: inner winding = 2 → filled → total area = both.
        // EvenOdd: inner depth = 1 → hole → total area = outer - inner.
        let outer = vec![[0.0f32, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let inner = vec![[3.0f32, 3.0], [7.0, 3.0], [7.0, 7.0], [3.0, 7.0]];
        let tris_nz = triangulate(&[outer.clone(), inner.clone()], FillRule::NonZero);
        let tris_eo = triangulate(&[outer, inner], FillRule::EvenOdd);
        let area_nz = total_area(&tris_nz);
        let area_eo = total_area(&tris_eo);
        // NonZero fills inner: area ≈ 100 (outer) + 16 (inner both covered).
        // EvenOdd: inner is hole: area ≈ 100 - 16 = 84.
        assert!(
            area_nz > area_eo,
            "NonZero should fill more than EvenOdd: nz={area_nz}, eo={area_eo}"
        );
    }

    // ── Degenerate: fewer than 3 points → empty ───────────────────────────────

    #[test]
    fn degenerate_too_few_points_is_empty() {
        let pts = vec![[0.0f32, 0.0], [1.0, 0.0]];
        let tris = triangulate(&[pts], FillRule::NonZero);
        assert!(tris.is_empty());
    }

    // ── Degenerate: collinear → empty ─────────────────────────────────────────

    #[test]
    fn degenerate_collinear_is_empty() {
        let pts = vec![[0.0f32, 0.0], [1.0, 0.0], [2.0, 0.0]];
        let tris = triangulate(&[pts], FillRule::NonZero);
        assert!(tris.is_empty());
    }

    // ── No panic on empty input ───────────────────────────────────────────────

    #[test]
    fn empty_contours_is_empty() {
        let tris = triangulate(&[], FillRule::NonZero);
        assert!(tris.is_empty());
    }

    // ── Triangle: 3 vertices → 1 triangle ────────────────────────────────────

    #[test]
    fn triangle_gives_one_triangle() {
        let pts = vec![[0.0f32, 0.0], [1.0, 0.0], [0.5, 1.0]];
        let tris = triangulate(&[pts], FillRule::NonZero);
        assert_eq!(tris.len(), 1);
    }
}
