// Instanced solid-rect shader for oxiui-render-wgpu.
//
// Renders many axis-aligned rectangles using a single indexed quad mesh and a
// per-instance buffer.  Each instance carries:
//
//   instance @location(0)  pos:    vec2<f32>   — top-left corner in pixel space
//   instance @location(1)  size:   vec2<f32>   — width × height in pixels
//   instance @location(2)  color:  vec4<f32>   — straight-alpha RGBA in [0,1]
//   instance @location(3)  corner_radius: f32  — uniform corner radius (0 = sharp)
//
// The mesh vertex carries a UV-like local coordinate (0..1) that is used to
// compute the pixel-space position and the SDF corner evaluation.
//
// Vertex (per-vertex, step=Vertex):
//   @location(4)  local: vec2<f32>   — 0..1 quad coordinate

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

// ── Vertex I/O ────────────────────────────────────────────────────────────────

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color:         vec4<f32>,
    @location(1) local:         vec2<f32>,  // pixel-space position relative to rect
    @location(2) half_size:     vec2<f32>,  // (w/2, h/2)
    @location(3) corner_radius: f32,
}

@vertex
fn vs_main(
    // Per-vertex inputs (step_mode = Vertex)
    @location(4) uv: vec2<f32>,

    // Per-instance inputs (step_mode = Instance)
    @location(0) inst_pos:           vec2<f32>,
    @location(1) inst_size:          vec2<f32>,
    @location(2) inst_color:         vec4<f32>,
    @location(3) inst_corner_radius: f32,
) -> VsOut {
    // Pixel-space corner position.
    let pixel_pos = inst_pos + uv * inst_size;

    // Pixel-space position relative to the rect centre (for SDF).
    let centre    = inst_pos + inst_size * 0.5;
    let local_pos = pixel_pos - centre;

    var out: VsOut;
    out.clip_pos     = vec4<f32>(pixel_to_ndc(pixel_pos), 0.0, 1.0);
    out.color        = inst_color;
    out.local        = local_pos;
    out.half_size    = inst_size * 0.5;
    out.corner_radius = inst_corner_radius;
    return out;
}

// ── Rounded-rect SDF ──────────────────────────────────────────────────────────

// Signed distance to a rounded rectangle centred at the origin with
// half-extents `b` and corner radius `r`.
fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0)))
         + min(max(q.x, q.y), 0.0)
         - r;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let r = in.corner_radius;
    if r > 0.5 {
        let d = sdf_rounded_rect(in.local, in.half_size, r);
        let coverage = smoothstep(0.5, -0.5, d);
        if coverage <= 0.0 { discard; }
        return vec4<f32>(in.color.rgb, in.color.a * coverage);
    }
    // Sharp rect — no SDF.
    return in.color;
}
