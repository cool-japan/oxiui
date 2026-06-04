// Gaussian blur via compute shader — horizontal pass.
//
// Dispatched as a 2-D workgroup of 16×16 threads.  Each thread reads one
// output texel from `src_texture` (sampling along the x-axis for the
// horizontal pass), accumulates a Gaussian-weighted sum over `radius` taps on
// each side, and writes the result to `dst_texture` via an image store.
//
// A second dispatching of this shader with `horizontal = 0` performs the
// vertical pass (the two passes form a separable Gaussian blur).
//
// Uniforms (BlurComputeUniforms, binding 0 group 0):
//   radius:     u32   — number of taps on each side (kernel half-width)
//   horizontal: u32   — 1 = horizontal pass, 0 = vertical pass
//   sigma:      f32   — Gaussian sigma (for weight computation)
//   _pad:       f32   — alignment padding
//
// Textures/storage (group 1):
//   binding 0 — src_texture : texture_2d<f32>   (sampled input)
//   binding 1 — dst_texture : texture_storage_2d<rgba8unorm, write> (output)
//   binding 2 — linear_sampler : sampler

struct BlurComputeUniforms {
    radius:     u32,
    horizontal: u32,
    sigma:      f32,
    _pad:       f32,
}

@group(0) @binding(0)
var<uniform> params: BlurComputeUniforms;

@group(1) @binding(0)
var src_texture: texture_2d<f32>;

@group(1) @binding(1)
var dst_texture: texture_storage_2d<rgba8unorm, write>;

@group(1) @binding(2)
var linear_sampler: sampler;

// Maximum blur radius supported by this shader.
const MAX_RADIUS: u32 = 64u;

@compute @workgroup_size(16, 16, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(src_texture);
    let out_x = gid.x;
    let out_y = gid.y;
    if out_x >= dims.x || out_y >= dims.y {
        return;
    }

    let radius = min(params.radius, MAX_RADIUS);
    let sigma = params.sigma;

    // Centre UV of this output texel.
    let centre_uv = (vec2<f32>(f32(out_x), f32(out_y)) + vec2<f32>(0.5, 0.5))
                     / vec2<f32>(f32(dims.x), f32(dims.y));

    // Step size in UV space (one texel).
    let step_x: f32 = select(1.0 / f32(dims.x), 0.0, params.horizontal == 0u);
    let step_y: f32 = select(1.0 / f32(dims.y), 0.0, params.horizontal != 0u);

    var color_sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var weight_sum: f32 = 0.0;

    // Accumulate Gaussian-weighted samples.
    for (var i: i32 = -i32(radius); i <= i32(radius); i++) {
        let t  = f32(i);
        let w  = exp(-0.5 * (t / sigma) * (t / sigma));
        let uv = centre_uv + vec2<f32>(t * step_x, t * step_y);
        color_sum  += textureSampleLevel(src_texture, linear_sampler, uv, 0.0) * w;
        weight_sum += w;
    }

    let result = color_sum / max(weight_sum, 0.0001);
    textureStore(dst_texture, vec2<i32>(i32(out_x), i32(out_y)), result);
}
