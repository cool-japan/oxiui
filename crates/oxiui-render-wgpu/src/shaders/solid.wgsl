// Solid-fill / SDF shader for the headless wgpu backend.
//
// Handles multiple primitive kinds dispatched by the `kind` vertex attribute:
//
//   0 = solid rectangle (no SDF, colour emitted verbatim)
//   1 = SDF circle (anti-aliased disc)
//   2 = SDF rounded rectangle, uniform radius
//   3 = SDF rounded rectangle, per-corner radii (packed u16 pairs in f32 bits)
//   4 = SDF ellipse
//   5 = SDF line segment (parametric distance field)
//
// Coordinates arrive in *pixel* space (origin top-left, +y down). The vertex
// stage applies a 2-D orthographic projection into NDC (x,y in [-1,1], +y up).
//
// Per-vertex layout (56 bytes, 14 x f32):
//   offset  0 : position (vec2<f32>)   — pixel-space quad corner
//   offset  8 : color    (vec4<f32>)   — straight-alpha RGBA in [0, 1]
//   offset 24 : local    (vec2<f32>)   — pixel-space position (SDF coord)
//   offset 32 : shape_xy (vec2<f32>)   — centre for circle/rrect/ellipse; line start
//   offset 40 : shape_r  (f32)         — circle radius / packed radii / line half_width
//   offset 44 : kind     (f32)         — primitive discriminator (0-5)
//   offset 48 : extra    (vec2<f32>)   — kind-specific: ellipse (rx,ry);
//               rrect-uniform (hw,hh); rrect-pc (brbl_packed, hwhh_packed);
//               line-seg (to.x, to.y)

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) color:    vec4<f32>,
    @location(2) local:    vec2<f32>,
    @location(3) shape_xy: vec2<f32>,
    @location(4) shape_r:  f32,
    @location(5) kind:     f32,
    @location(6) extra:    vec2<f32>,
}

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color:    vec4<f32>,
    @location(1) local:    vec2<f32>,
    @location(2) shape_xy: vec2<f32>,
    @location(3) shape_r:  f32,
    @location(4) kind:     f32,
    @location(5) extra:    vec2<f32>,
}

struct Globals {
    viewport: vec2<f32>,
    _pad:     vec2<f32>,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

fn pixel_to_ndc(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        p.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - p.y / globals.viewport.y * 2.0,
    );
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_position = vec4<f32>(pixel_to_ndc(in.position), 0.0, 1.0);
    out.color    = in.color;
    out.local    = in.local;
    out.shape_xy = in.shape_xy;
    out.shape_r  = in.shape_r;
    out.kind     = in.kind;
    out.extra    = in.extra;
    return out;
}

// ── SDF helpers ──────────────────────────────────────────────────────────────

// Signed distance to a line segment from a to b.
fn sdf_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h  = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let k = in.kind;

    // ── 0: solid rectangle ────────────────────────────────────────────────────
    if k < 0.5 {
        return in.color;
    }

    // ── 1: SDF circle ─────────────────────────────────────────────────────────
    if k < 1.5 {
        let dist     = length(in.local - in.shape_xy);
        let coverage = 1.0 - smoothstep(in.shape_r - 1.0, in.shape_r, dist);
        if coverage <= 0.0 { discard; }
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }

    // ── 2: SDF rounded rect (uniform radius) ──────────────────────────────────
    if k < 2.5 {
        // shape_xy = centre; shape_r = corner radius; extra = (hw, hh).
        let p = in.local - in.shape_xy;
        let b = in.extra;              // half-extents
        let r = in.shape_r;
        let q = abs(p) - b + vec2<f32>(r, r);
        let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
        let coverage = smoothstep(0.5, -0.5, d);
        if coverage <= 0.0 { discard; }
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }

    // ── 3: SDF rounded rect (per-corner) ─────────────────────────────────────
    if k < 3.5 {
        // Encoding (integer-arithmetic, avoids GPU denormal issues):
        //   shape_xy  = centre (cx, cy).
        //   shape_r   = tl_r * 256.0 + tr_r        (radii in [0, 255])
        //   extra[0]  = br_r * 256.0 + bl_r
        //   extra[1]  = hw_i * 4096.0 + hh_i       (half-extents in [0, 4095])
        let p = in.local - in.shape_xy;

        let hwhh  = in.extra[1];
        let hw    = floor(hwhh / 4096.0);
        let hh    = hwhh - hw * 4096.0;
        let b     = vec2<f32>(hw, hh);

        let tltr = in.shape_r;
        let tl   = floor(tltr / 256.0);
        let tr   = tltr - tl * 256.0;

        let brbl = in.extra[0];
        let br   = floor(brbl / 256.0);
        let bl   = brbl - br * 256.0;

        // Select radius for the fragment's quadrant (right = px>0, bottom = py>0).
        var r_corner: f32;
        if p.x > 0.0 {
            if p.y > 0.0 { r_corner = br; }
            else         { r_corner = tr; }
        } else {
            if p.y > 0.0 { r_corner = bl; }
            else         { r_corner = tl; }
        }
        let q = abs(p) - b + vec2<f32>(r_corner, r_corner);
        let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r_corner;
        let coverage = smoothstep(0.5, -0.5, d);
        if coverage <= 0.0 { discard; }
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }

    // ── 4: SDF ellipse ────────────────────────────────────────────────────────
    if k < 4.5 {
        // shape_xy = centre; extra = (rx, ry).
        let p  = in.local - in.shape_xy;
        let ab = in.extra;
        // Ellipse SDF: normalise coords, compute signed distance.
        let norm = p / ab;
        let len_n = length(norm);
        // Scale distance by the minor radius for an approximate pixel-space AA band.
        let scale = min(ab.x, ab.y);
        let d = (len_n - 1.0) * scale;
        let coverage = smoothstep(0.5, -0.5, d);
        if coverage <= 0.0 { discard; }
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }

    // ── 5: SDF line segment ───────────────────────────────────────────────────
    if k < 5.5 {
        // shape_xy = endpoint A; extra = endpoint B.
        // shape_r  = half_width (integer part) + 0.5 if AA mode.
        let a  = in.shape_xy;
        let b  = in.extra;
        let hw = floor(in.shape_r + 0.01);
        let do_aa = (in.shape_r - hw) > 0.25;

        let dist = sdf_segment(in.local, a, b);
        if do_aa {
            let coverage = smoothstep(hw + 1.0, hw - 0.5, dist);
            if coverage <= 0.0 { discard; }
            return vec4<f32>(in.color.rgb, in.color.a * coverage);
        } else {
            if dist > hw + 0.5 { discard; }
            return in.color;
        }
    }

    // Fallback (unreachable with current kind set).
    return in.color;
}
