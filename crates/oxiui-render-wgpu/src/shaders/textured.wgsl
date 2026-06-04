// Textured pipeline shader for the headless wgpu backend.
//
// Renders texture-mapped quads with optional tint. Supports both plain image
// blits (DrawCommand::Image) and nine-slice patches (DrawCommand::NineSlice).
//
// Coordinates arrive in *pixel* space (origin top-left, +y down). The vertex
// stage applies a 2-D orthographic projection into NDC (x,y in [-1,1], +y up).
//
// Per-vertex layout (32 bytes, 8 x f32):
//   offset  0 : position (vec2<f32>)  — pixel-space quad corner
//   offset  8 : uv       (vec2<f32>)  — texture UV in [0,1]
//   offset 16 : tint     (vec4<f32>)  — RGBA tint multiplier (usually [1,1,1,1])
//
// Bind groups:
//   group(0) binding(0) — Globals uniform (viewport size, vertex stage)
//   group(1) binding(0) — texture_2d<f32> (fragment stage)
//   group(1) binding(1) — sampler (fragment stage)

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) uv:       vec2<f32>,
    @location(2) tint:     vec4<f32>,
}

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv:   vec2<f32>,
    @location(1) tint: vec4<f32>,
}

struct Globals {
    viewport: vec2<f32>,
    _pad:     vec2<f32>,
}

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(1) @binding(0)
var tex: texture_2d<f32>;

@group(1) @binding(1)
var samp: sampler;

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
    out.uv   = in.uv;
    out.tint = in.tint;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(tex, samp, in.uv);
    return sampled * in.tint;
}
