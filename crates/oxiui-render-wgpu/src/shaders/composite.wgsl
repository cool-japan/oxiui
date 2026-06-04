// Shadow composite pass.
// Bind group 0: Globals (viewport) — vertex
// Bind group 1: blurred mask texture + sampler + CompUniforms — fragment

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

struct CompUniforms {
    tint:       vec4<f32>,
    texel_size: vec2<f32>,
    _pad:       vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals:   Globals;
@group(1) @binding(0) var          mask_tex:  texture_2d<f32>;
@group(1) @binding(1) var          mask_samp: sampler;
@group(1) @binding(2) var<uniform> comp:      CompUniforms;

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
    out.local = in.local;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.local * comp.texel_size;
    let mask = textureSample(mask_tex, mask_samp, uv).a;
    return vec4<f32>(comp.tint.rgb, comp.tint.a * mask);
}
