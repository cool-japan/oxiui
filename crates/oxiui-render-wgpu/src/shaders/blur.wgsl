// Separable Gaussian blur pass.
// Bind group 0: Globals (viewport) — vertex
// Bind group 1: input texture + sampler + BlurUniforms — fragment

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

struct BlurUniforms {
    direction:  vec2<f32>,
    texel_size: vec2<f32>,
    radius:     f32,
    sigma:      f32,
    _pad:       vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals:   Globals;
@group(1) @binding(0) var          src_tex:   texture_2d<f32>;
@group(1) @binding(1) var          src_samp:  sampler;
@group(1) @binding(2) var<uniform> blur:      BlurUniforms;

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
    // Convert pixel-space local to UV.
    let uv = in.local * blur.texel_size;
    let step = blur.direction * blur.texel_size;

    var colour_sum = vec4<f32>(0.0);
    var weight_sum = 0.0;

    // Radius clamped to 64 in Rust; iterate up to 2*64+1 = 129 taps.
    let r = i32(blur.radius);
    for (var i = -r; i <= r; i++) {
        let fi = f32(i);
        let w = exp(-fi * fi / (2.0 * blur.sigma * blur.sigma));
        let sample_uv = uv + step * fi;
        colour_sum += textureSample(src_tex, src_samp, sample_uv) * w;
        weight_sum += w;
    }

    if weight_sum < 1e-6 { return vec4<f32>(0.0); }
    return colour_sum / weight_sum;
}
