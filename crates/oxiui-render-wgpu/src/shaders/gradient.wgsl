// Gradient fill shader for the headless wgpu backend.
//
// Supports linear and radial gradient fills over a rectangular region.
// Gradient geometry is provided as screen-space quads; the actual colour ramp
// is defined in a per-draw GradientUniforms uniform buffer.
//
// Uniform layout (GradientUniforms, 288 bytes):
//   offset   0 : p0            (vec2<f32>) — linear start / radial centre
//   offset   8 : p1            (vec2<f32>) — linear end / (unused)
//   offset  16 : radius        (f32)       — radial outer radius (0 for linear)
//   offset  20 : gradient_type (u32)       — 0=linear, 1=radial
//   offset  24 : stop_count    (u32)       — number of active stops (1–8)
//   offset  28 : _pad          (u32)
//   offset  32 : stop_offsets  (8 × vec4<f32>) — x = offset in [0,1]
//   offset 160 : stop_colors   (8 × vec4<f32>) — rgba in [0,1]

// Vertex input.
struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) local:    vec2<f32>,
}

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local: vec2<f32>,
}

struct Globals {
    viewport: vec2<f32>,
    _pad:     vec2<f32>,
}

struct GradientUniforms {
    p0:            vec2<f32>,
    p1:            vec2<f32>,
    radius:        f32,
    gradient_type: u32,
    stop_count:    u32,
    _pad:          u32,
    stop_offsets:  array<vec4<f32>, 8>,
    stop_colors:   array<vec4<f32>, 8>,
}

@group(0) @binding(0) var<uniform> globals:  Globals;
@group(0) @binding(1) var<uniform> gradient: GradientUniforms;

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
    out.local         = in.local;
    return out;
}

// ── Gradient colour sampling ─────────────────────────────────────────────────

// Interpolate gradient colour at normalised position t in [0, 1].
fn sample_gradient(t: f32) -> vec4<f32> {
    let tc = clamp(t, 0.0, 1.0);
    let n  = i32(gradient.stop_count);

    // Single-stop degenerate case.
    if n <= 1 { return gradient.stop_colors[0]; }

    // Find the two surrounding stops.
    for (var i = 0; i < 7; i++) {
        let a_off = gradient.stop_offsets[i].x;
        let b_off = gradient.stop_offsets[i + 1].x;
        if tc <= b_off || i == n - 2 {
            if tc <= a_off { return gradient.stop_colors[i]; }
            let seg = b_off - a_off;
            var frac: f32;
            if seg < 1e-6 {
                frac = 0.0;
            } else {
                frac = (tc - a_off) / seg;
            }
            return mix(gradient.stop_colors[i], gradient.stop_colors[i + 1], frac);
        }
    }

    // Past the last stop.
    return gradient.stop_colors[n - 1];
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var t: f32;

    if gradient.gradient_type == 0u {
        // Linear gradient: project local onto the gradient axis.
        let axis = gradient.p1 - gradient.p0;
        let len2 = dot(axis, axis);
        if len2 < 1e-12 {
            t = 0.0;
        } else {
            t = dot(in.local - gradient.p0, axis) / len2;
        }
    } else {
        // Radial gradient: distance from centre divided by radius.
        let d = length(in.local - gradient.p0);
        if gradient.radius < 1e-6 {
            t = 1.0;
        } else {
            t = d / gradient.radius;
        }
    }

    return sample_gradient(t);
}
