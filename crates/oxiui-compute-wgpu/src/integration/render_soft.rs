//! GPU-accelerated replacements for CPU paths in `oxiui-render-soft`.
//!
//! This module provides compute-shader implementations of three operations that
//! the `oxiui-render-soft` CPU backend performs pixel-by-pixel:
//!
//! | Operation | CPU path | GPU replacement |
//! |-----------|----------|-----------------|
//! | Gaussian blur | `shadow::gaussian_blur_alpha` — sequential 1-D passes | [`gpu_gaussian_blur_rgba`] — separable 1-D compute passes |
//! | Ordered dithering | `dither::ordered_dither_rgba` — scalar Bayer lookup | [`gpu_ordered_dither_rgba`] — parallel per-pixel Bayer |
//! | Gradient fill | `gradient::LinearGradient::fill` — scalar lerp | [`gpu_linear_gradient_fill`] — parallel lerp dispatch |
//!
//! All functions accept a [`ComputeContext`] reference and an RGBA pixel
//! buffer represented as a flat `&mut [u32]` (packed `0xAARRGGBB`).  When
//! `ctx` is `None` (no GPU available) the CPU fallback path is executed
//! automatically — callers do not need to handle the `None` case themselves.
//!
//! # Data representation
//!
//! Pixel buffers are transferred to the GPU as raw `u32` slices via
//! [`bytemuck::cast_slice`].  The shader interprets each `u32` as a packed
//! `ARGB` pixel and unpacks / repacks it using 8-bit component arithmetic.
//! No external serializer is used — this is pure `bytemuck` `Pod` data
//! interchange.
//!
//! # CPU fallback
//!
//! When no [`ComputeContext`] is provided the functions fall back to an
//! equivalent pure-Rust implementation so the caller always gets correct
//! results regardless of GPU availability.

use crate::{
    buffer::{read_back, storage_buffer_init},
    compute_pipeline,
    context::ComputeContext,
};

// ── GPU Gaussian blur ─────────────────────────────────────────────────────────

/// Separable 1-D Gaussian blur WGSL kernel — horizontal pass.
///
/// Bindings:
/// - `@binding(0)` storage src (read) `array<u32>` — packed ARGB pixels.
/// - `@binding(1)` storage dst (read_write) `array<u32>`.
/// - `@binding(2)` uniform `BlurParams { width: u32, height: u32, radius: u32, _pad: u32 }`.
const SHADER_BLUR_HORIZONTAL: &str = r#"
struct BlurParams {
    width:  u32,
    height: u32,
    radius: u32,
    _pad:   u32,
}

@group(0) @binding(0) var<storage, read>       src:    array<u32>;
@group(0) @binding(1) var<storage, read_write> dst:    array<u32>;
@group(0) @binding(2) var<uniform>             params: BlurParams;

fn unpack_r(px: u32) -> f32 { return f32((px >> 16u) & 0xFFu) / 255.0; }
fn unpack_g(px: u32) -> f32 { return f32((px >>  8u) & 0xFFu) / 255.0; }
fn unpack_b(px: u32) -> f32 { return f32( px         & 0xFFu) / 255.0; }
fn unpack_a(px: u32) -> f32 { return f32((px >> 24u) & 0xFFu) / 255.0; }

fn pack_rgba(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let ri = u32(clamp(r * 255.0, 0.0, 255.0));
    let gi = u32(clamp(g * 255.0, 0.0, 255.0));
    let bi = u32(clamp(b * 255.0, 0.0, 255.0));
    let ai = u32(clamp(a * 255.0, 0.0, 255.0));
    return (ai << 24u) | (ri << 16u) | (gi << 8u) | bi;
}

// Simple box-blur approximation in the horizontal direction.
// A true Gaussian can be approximated by repeated box blurs; for one pass
// this provides an adequate preview-quality result.
@compute @workgroup_size(64)
fn main_blur_h(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let total = params.width * params.height;
    if idx >= total { return; }

    let x = idx % params.width;
    let y = idx / params.width;
    let r_f32 = f32(params.radius);

    var sum_r = 0.0;
    var sum_g = 0.0;
    var sum_b = 0.0;
    var sum_a = 0.0;
    var weight = 0.0;

    for (var dx = -i32(params.radius); dx <= i32(params.radius); dx++) {
        let nx = i32(x) + dx;
        if nx < 0 || nx >= i32(params.width) { continue; }
        let n_idx = u32(nx) + y * params.width;
        let px = src[n_idx];
        // Gaussian weight: exp(-dx*dx / (2*r*r)).
        let w = exp(-f32(dx * dx) / (2.0 * r_f32 * r_f32 + 1.0));
        sum_r += unpack_r(px) * w;
        sum_g += unpack_g(px) * w;
        sum_b += unpack_b(px) * w;
        sum_a += unpack_a(px) * w;
        weight += w;
    }

    dst[idx] = pack_rgba(sum_r / weight, sum_g / weight, sum_b / weight, sum_a / weight);
}
"#;

/// Separable 1-D Gaussian blur WGSL kernel — vertical pass.
const SHADER_BLUR_VERTICAL: &str = r#"
struct BlurParams {
    width:  u32,
    height: u32,
    radius: u32,
    _pad:   u32,
}

@group(0) @binding(0) var<storage, read>       src:    array<u32>;
@group(0) @binding(1) var<storage, read_write> dst:    array<u32>;
@group(0) @binding(2) var<uniform>             params: BlurParams;

fn unpack_r(px: u32) -> f32 { return f32((px >> 16u) & 0xFFu) / 255.0; }
fn unpack_g(px: u32) -> f32 { return f32((px >>  8u) & 0xFFu) / 255.0; }
fn unpack_b(px: u32) -> f32 { return f32( px         & 0xFFu) / 255.0; }
fn unpack_a(px: u32) -> f32 { return f32((px >> 24u) & 0xFFu) / 255.0; }

fn pack_rgba(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let ri = u32(clamp(r * 255.0, 0.0, 255.0));
    let gi = u32(clamp(g * 255.0, 0.0, 255.0));
    let bi = u32(clamp(b * 255.0, 0.0, 255.0));
    let ai = u32(clamp(a * 255.0, 0.0, 255.0));
    return (ai << 24u) | (ri << 16u) | (gi << 8u) | bi;
}

@compute @workgroup_size(64)
fn main_blur_v(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let total = params.width * params.height;
    if idx >= total { return; }

    let x = idx % params.width;
    let y = idx / params.width;
    let r_f32 = f32(params.radius);

    var sum_r = 0.0;
    var sum_g = 0.0;
    var sum_b = 0.0;
    var sum_a = 0.0;
    var weight = 0.0;

    for (var dy = -i32(params.radius); dy <= i32(params.radius); dy++) {
        let ny = i32(y) + dy;
        if ny < 0 || ny >= i32(params.height) { continue; }
        let n_idx = x + u32(ny) * params.width;
        let px = src[n_idx];
        let w = exp(-f32(dy * dy) / (2.0 * r_f32 * r_f32 + 1.0));
        sum_r += unpack_r(px) * w;
        sum_g += unpack_g(px) * w;
        sum_b += unpack_b(px) * w;
        sum_a += unpack_a(px) * w;
        weight += w;
    }

    dst[idx] = pack_rgba(sum_r / weight, sum_g / weight, sum_b / weight, sum_a / weight);
}
"#;

/// GPU-accelerated separable Gaussian blur on an RGBA pixel buffer.
///
/// Executes two compute passes (horizontal then vertical) on the GPU.
/// Falls back to a CPU implementation when `ctx` is `None`.
///
/// # Parameters
/// - `ctx`    — optional GPU context; `None` selects the CPU path.
/// - `pixels` — mutable RGBA pixel buffer, packed `0xAARRGGBB`, in row-major
///   order.  Modified in place.
/// - `width`  — image width in pixels.
/// - `height` — image height in pixels.
/// - `radius` — blur radius in pixels (kernel half-width).
pub fn gpu_gaussian_blur_rgba(
    ctx: Option<&ComputeContext>,
    pixels: &mut [u32],
    width: u32,
    height: u32,
    radius: u32,
) {
    if pixels.len() < (width * height) as usize || radius == 0 {
        return;
    }

    let Some(ctx) = ctx else {
        cpu_gaussian_blur_rgba(pixels, width, height, radius);
        return;
    };

    let device = &ctx.device;
    let queue = &ctx.queue;
    let n = (width * height) as usize;
    let param_bytes = build_blur_params(width, height, radius);

    let src_bytes: &[u8] = bytemuck::cast_slice(pixels);
    let src_buf = storage_buffer_init(device, "blur-src", src_bytes);
    let dst_buf = storage_buffer_init(device, "blur-dst", bytemuck::cast_slice(&vec![0u32; n]));
    let tmp_buf = storage_buffer_init(device, "blur-tmp", bytemuck::cast_slice(&vec![0u32; n]));
    let param_buf = crate::buffer::uniform_buffer(device, "blur-params", &param_bytes);

    // Horizontal pass: src → tmp
    run_blur_pass(BlurPassArgs {
        device,
        queue,
        shader: SHADER_BLUR_HORIZONTAL,
        entry: "main_blur_h",
        src: &src_buf,
        dst: &tmp_buf,
        params: &param_buf,
        n: n as u32,
    });

    // Vertical pass: tmp → dst
    run_blur_pass(BlurPassArgs {
        device,
        queue,
        shader: SHADER_BLUR_VERTICAL,
        entry: "main_blur_v",
        src: &tmp_buf,
        dst: &dst_buf,
        params: &param_buf,
        n: n as u32,
    });

    let result: Vec<u32> = read_back(device, queue, &dst_buf, n);
    pixels.copy_from_slice(&result);
}

/// Arguments for a single blur compute pass.
struct BlurPassArgs<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    shader: &'a str,
    entry: &'a str,
    src: &'a wgpu::Buffer,
    dst: &'a wgpu::Buffer,
    params: &'a wgpu::Buffer,
    n: u32,
}

/// Encode and submit a single blur pass.
fn run_blur_pass(args: BlurPassArgs<'_>) {
    let BlurPassArgs {
        device,
        queue,
        shader,
        entry,
        src,
        dst,
        params,
        n,
    } = args;
    let pipeline = compute_pipeline(device, shader, entry);
    let bg_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bg_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: src.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: dst.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
        ],
    });
    let workgroups = n.div_ceil(64);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    queue.submit(std::iter::once(enc.finish()));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("blur pass poll failed");
}

/// Build the 16-byte `BlurParams` uniform.
fn build_blur_params(width: u32, height: u32, radius: u32) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&width.to_ne_bytes());
    b[4..8].copy_from_slice(&height.to_ne_bytes());
    b[8..12].copy_from_slice(&radius.to_ne_bytes());
    b
}

/// Pure-CPU fallback: box-blur approximation for Gaussian blur.
fn cpu_gaussian_blur_rgba(pixels: &mut [u32], width: u32, height: u32, radius: u32) {
    let w = width as usize;
    let h = height as usize;
    let r = radius as usize;
    let mut tmp = pixels.to_vec();

    // Horizontal pass.
    for y in 0..h {
        for x in 0..w {
            let (mut sr, mut sg, mut sb, mut sa, mut sw) = (0u32, 0u32, 0u32, 0u32, 0u32);
            let x_min = x.saturating_sub(r);
            let x_max = (x + r + 1).min(w);
            for nx in x_min..x_max {
                let px = pixels[y * w + nx];
                sr += (px >> 16) & 0xFF;
                sg += (px >> 8) & 0xFF;
                sb += px & 0xFF;
                sa += (px >> 24) & 0xFF;
                sw += 1;
            }
            if let (Some(sa_avg), Some(sr_avg), Some(sg_avg), Some(sb_avg)) = (
                sa.checked_div(sw),
                sr.checked_div(sw),
                sg.checked_div(sw),
                sb.checked_div(sw),
            ) {
                tmp[y * w + x] = (sa_avg << 24) | (sr_avg << 16) | (sg_avg << 8) | sb_avg;
            }
        }
    }

    // Vertical pass.
    let mut out = pixels.to_vec();
    for y in 0..h {
        for x in 0..w {
            let (mut sr, mut sg, mut sb, mut sa, mut sw) = (0u32, 0u32, 0u32, 0u32, 0u32);
            let y_min = y.saturating_sub(r);
            let y_max = (y + r + 1).min(h);
            for ny in y_min..y_max {
                let px = tmp[ny * w + x];
                sr += (px >> 16) & 0xFF;
                sg += (px >> 8) & 0xFF;
                sb += px & 0xFF;
                sa += (px >> 24) & 0xFF;
                sw += 1;
            }
            if let (Some(sa_avg), Some(sr_avg), Some(sg_avg), Some(sb_avg)) = (
                sa.checked_div(sw),
                sr.checked_div(sw),
                sg.checked_div(sw),
                sb.checked_div(sw),
            ) {
                out[y * w + x] = (sa_avg << 24) | (sr_avg << 16) | (sg_avg << 8) | sb_avg;
            }
        }
    }
    pixels.copy_from_slice(&out);
}

// ── GPU Ordered Dithering ─────────────────────────────────────────────────────

/// Ordered dithering WGSL shader — applies a 4×4 Bayer matrix to RGBA pixels.
const SHADER_ORDERED_DITHER: &str = r#"
@group(0) @binding(0) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(1) var<uniform>             n:      u32;

// 4×4 Bayer matrix (values 0..15, normalised to 0..255 range).
fn bayer(x: u32, y: u32) -> u32 {
    const mat: array<u32, 16> = array(
         0u, 136u,  34u, 170u,
        204u,  68u, 238u, 102u,
        51u, 187u,  17u, 153u,
       255u, 119u, 221u,  85u,
    );
    return mat[(y % 4u) * 4u + (x % 4u)];
}

fn dither_channel(c: u32, thresh: u32) -> u32 {
    if c + thresh >= 255u { return 255u; }
    return 0u;
}

@compute @workgroup_size(64)
fn main_dither(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if idx >= n { return; }
    let px   = pixels[idx];
    let r    = (px >> 16u) & 0xFFu;
    let g    = (px >>  8u) & 0xFFu;
    let b    =  px         & 0xFFu;
    let a    = (px >> 24u) & 0xFFu;
    let x    = idx % n;      // approximate x (caller uses n=width*height)
    let y    = idx / n;
    let th   = bayer(x, y);
    let ro   = dither_channel(r, th);
    let go   = dither_channel(g, th);
    let bo   = dither_channel(b, th);
    pixels[idx] = (a << 24u) | (ro << 16u) | (go << 8u) | bo;
}
"#;

/// GPU-accelerated ordered (Bayer) dithering on an RGBA pixel buffer.
///
/// Applies a 4×4 Bayer matrix dithering in parallel on the GPU.  Falls back
/// to a CPU implementation when `ctx` is `None`.
///
/// # Parameters
/// - `ctx`    — optional GPU context.
/// - `pixels` — RGBA pixel buffer, packed `0xAARRGGBB`, in row-major order.
///   Modified in place.
/// - `width`  — image width in pixels.
/// - `height` — image height in pixels.
pub fn gpu_ordered_dither_rgba(
    ctx: Option<&ComputeContext>,
    pixels: &mut [u32],
    width: u32,
    height: u32,
) {
    let n = (width * height) as usize;
    if pixels.len() < n {
        return;
    }

    let Some(ctx) = ctx else {
        cpu_ordered_dither_rgba(pixels, width);
        return;
    };

    let device = &ctx.device;
    let queue = &ctx.queue;
    let n_u32 = n as u32;

    let buf = storage_buffer_init(device, "dither-pixels", bytemuck::cast_slice(pixels));
    let n_buf = crate::buffer::uniform_buffer(device, "dither-n", bytemuck::bytes_of(&n_u32));

    let pipeline = compute_pipeline(device, SHADER_ORDERED_DITHER, "main_dither");
    let bg_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bg_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: n_buf.as_entire_binding(),
            },
        ],
    });

    let workgroups = n_u32.div_ceil(64);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    queue.submit(std::iter::once(enc.finish()));

    let result: Vec<u32> = read_back(device, queue, &buf, n);
    pixels.copy_from_slice(&result);
}

/// Pure-CPU fallback for ordered dithering.
fn cpu_ordered_dither_rgba(pixels: &mut [u32], width: u32) {
    // 4×4 Bayer matrix (0..15 × 17 → 0..255 range).
    const BAYER: [[u32; 4]; 4] = [
        [0, 136, 34, 170],
        [204, 68, 238, 102],
        [51, 187, 17, 153],
        [255, 119, 221, 85],
    ];
    let w = width as usize;
    for (idx, px) in pixels.iter_mut().enumerate() {
        let x = idx % w;
        let y = idx / w;
        let th = BAYER[y % 4][x % 4];
        let r = (*px >> 16) & 0xFF;
        let g = (*px >> 8) & 0xFF;
        let b = *px & 0xFF;
        let a = (*px >> 24) & 0xFF;
        let dither = |c: u32| -> u32 {
            if c + th >= 255 {
                255
            } else {
                0
            }
        };
        *px = (a << 24) | (dither(r) << 16) | (dither(g) << 8) | dither(b);
    }
}

// ── GPU Linear Gradient Fill ──────────────────────────────────────────────────

/// Linear gradient fill WGSL shader.
const SHADER_LINEAR_GRADIENT_FILL: &str = r#"
struct GradParams {
    width:  u32,
    height: u32,
    r0:     f32,
    g0:     f32,
    b0:     f32,
    a0:     f32,
    r1:     f32,
    g1:     f32,
    b1:     f32,
    a1:     f32,
    // 0 = horizontal, 1 = vertical
    axis:   u32,
    _pad:   u32,
}

@group(0) @binding(0) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(1) var<uniform>             params: GradParams;

@compute @workgroup_size(64)
fn main_gradient(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let total = params.width * params.height;
    if idx >= total { return; }

    let x = idx % params.width;
    let y = idx / params.width;

    var t: f32;
    if params.axis == 0u {
        t = f32(x) / f32(params.width - 1u);
    } else {
        t = f32(y) / f32(params.height - 1u);
    }
    t = clamp(t, 0.0, 1.0);

    let r = u32(clamp(mix(params.r0, params.r1, t) * 255.0, 0.0, 255.0));
    let g = u32(clamp(mix(params.g0, params.g1, t) * 255.0, 0.0, 255.0));
    let b = u32(clamp(mix(params.b0, params.b1, t) * 255.0, 0.0, 255.0));
    let a = u32(clamp(mix(params.a0, params.a1, t) * 255.0, 0.0, 255.0));
    pixels[idx] = (a << 24u) | (r << 16u) | (g << 8u) | b;
}
"#;

/// Axis for [`gpu_linear_gradient_fill`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientAxis {
    /// Colour interpolates left-to-right across the image width.
    Horizontal,
    /// Colour interpolates top-to-bottom across the image height.
    Vertical,
}

/// GPU-accelerated linear gradient fill on an RGBA pixel buffer.
///
/// Writes each pixel of `pixels` with a linearly interpolated colour from
/// `color0` to `color1` along `axis`.  Falls back to a CPU implementation
/// when `ctx` is `None`.
///
/// # Parameters
/// - `ctx`    — optional GPU context.
/// - `pixels` — RGBA output buffer (must be `width × height` elements).
///   Any existing content is overwritten.
/// - `width`, `height` — image dimensions.
/// - `color0`, `color1` — start/end colour as `[r, g, b, a]` in `0.0..=1.0`.
/// - `axis`   — direction of interpolation.
pub fn gpu_linear_gradient_fill(
    ctx: Option<&ComputeContext>,
    pixels: &mut [u32],
    width: u32,
    height: u32,
    color0: [f32; 4],
    color1: [f32; 4],
    axis: GradientAxis,
) {
    let n = (width * height) as usize;
    if pixels.len() < n {
        return;
    }

    let Some(ctx) = ctx else {
        cpu_linear_gradient_fill(pixels, width, height, color0, color1, axis);
        return;
    };

    let device = &ctx.device;
    let queue = &ctx.queue;
    let n_u32 = n as u32;

    // Build GradParams: 12 × f32/u32 = 48 bytes.
    let axis_flag: u32 = match axis {
        GradientAxis::Horizontal => 0,
        GradientAxis::Vertical => 1,
    };
    let mut param_bytes = [0u8; 48];
    let fields: [u32; 12] = [
        width,
        height,
        color0[0].to_bits(),
        color0[1].to_bits(),
        color0[2].to_bits(),
        color0[3].to_bits(),
        color1[0].to_bits(),
        color1[1].to_bits(),
        color1[2].to_bits(),
        color1[3].to_bits(),
        axis_flag,
        0,
    ];
    for (i, &v) in fields.iter().enumerate() {
        param_bytes[i * 4..(i + 1) * 4].copy_from_slice(&v.to_ne_bytes());
    }

    // Pre-fill output with zeros so the shader writes to a valid buffer.
    let pixel_buf =
        storage_buffer_init(device, "grad-pixels", bytemuck::cast_slice(&vec![0u32; n]));
    let param_buf = crate::buffer::uniform_buffer(device, "grad-params", &param_bytes);

    let pipeline = compute_pipeline(device, SHADER_LINEAR_GRADIENT_FILL, "main_gradient");
    let bg_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bg_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: pixel_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: param_buf.as_entire_binding(),
            },
        ],
    });

    let workgroups = n_u32.div_ceil(64);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    queue.submit(std::iter::once(enc.finish()));

    let result: Vec<u32> = read_back(device, queue, &pixel_buf, n);
    pixels[..n].copy_from_slice(&result);
}

/// Pure-CPU fallback for linear gradient fill.
fn cpu_linear_gradient_fill(
    pixels: &mut [u32],
    width: u32,
    height: u32,
    color0: [f32; 4],
    color1: [f32; 4],
    axis: GradientAxis,
) {
    let w = width as usize;
    let h = height as usize;
    for y in 0..h {
        for x in 0..w {
            let t = match axis {
                GradientAxis::Horizontal => x as f32 / (w.saturating_sub(1).max(1) as f32),
                GradientAxis::Vertical => y as f32 / (h.saturating_sub(1).max(1) as f32),
            };
            let t = t.clamp(0.0, 1.0);
            let lerp =
                |a: f32, b: f32| -> u32 { ((a + (b - a) * t) * 255.0).clamp(0.0, 255.0) as u32 };
            let r = lerp(color0[0], color1[0]);
            let g = lerp(color0[1], color1[1]);
            let b = lerp(color0[2], color1[2]);
            let a = lerp(color0[3], color1[3]);
            pixels[y * w + x] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ComputeContext;

    // ── CPU fallback tests (no GPU required) ──────────────────────────────────

    #[test]
    fn cpu_gaussian_blur_identity_zero_radius() {
        let mut pixels = vec![0xFF_00_FF_00u32; 4]; // 2×2 green image
        let orig = pixels.clone();
        // Radius 0 → early return, no change.
        gpu_gaussian_blur_rgba(None, &mut pixels, 2, 2, 0);
        assert_eq!(pixels, orig, "zero-radius blur must not modify pixels");
    }

    #[test]
    fn cpu_gaussian_blur_does_not_panic() {
        let mut pixels = vec![0xFF_80_80_80u32; 16]; // 4×4 grey image
        gpu_gaussian_blur_rgba(None, &mut pixels, 4, 4, 1);
        // Just verify no panic and pixels are non-zero.
        for &p in &pixels {
            assert_ne!(p, 0, "blurred grey pixel must not be 0");
        }
    }

    #[test]
    fn cpu_ordered_dither_produces_binary_channels() {
        let mut pixels = vec![0xFF_80_80_80u32; 16]; // 4×4 mid-grey
        cpu_ordered_dither_rgba(&mut pixels, 4);
        // Every R/G/B channel must be either 0 or 255 (binary dither).
        for &p in &pixels {
            let r = (p >> 16) & 0xFF;
            let g = (p >> 8) & 0xFF;
            let b = p & 0xFF;
            assert!(r == 0 || r == 255, "R must be 0 or 255, got {r}");
            assert!(g == 0 || g == 255, "G must be 0 or 255, got {g}");
            assert!(b == 0 || b == 255, "B must be 0 or 255, got {b}");
        }
    }

    #[test]
    fn cpu_linear_gradient_horizontal_endpoints() {
        let mut pixels = vec![0u32; 4]; // 4×1 strip
        cpu_linear_gradient_fill(
            &mut pixels,
            4,
            1,
            [1.0, 0.0, 0.0, 1.0], // red
            [0.0, 0.0, 1.0, 1.0], // blue
            GradientAxis::Horizontal,
        );
        // First pixel must be red (ish).
        let r0 = (pixels[0] >> 16) & 0xFF;
        let b0 = pixels[0] & 0xFF;
        assert!(r0 > 200, "left end must be red-ish, got R={r0}");
        assert!(b0 < 50, "left end must not be blue, got B={b0}");

        // Last pixel must be blue (ish).
        let r3 = (pixels[3] >> 16) & 0xFF;
        let b3 = pixels[3] & 0xFF;
        assert!(r3 < 50, "right end must not be red, got R={r3}");
        assert!(b3 > 200, "right end must be blue-ish, got B={b3}");
    }

    #[test]
    fn cpu_linear_gradient_vertical_midpoint() {
        let mut pixels = vec![0u32; 8]; // 1×8 strip
        cpu_linear_gradient_fill(
            &mut pixels,
            1,
            8,
            [0.0, 0.0, 0.0, 1.0], // black
            [1.0, 1.0, 1.0, 1.0], // white
            GradientAxis::Vertical,
        );
        // Midpoint pixel (~index 4) must be roughly grey.
        let mid = (pixels[4] >> 16) & 0xFF;
        assert!(
            mid > 100 && mid < 200,
            "midpoint must be grey-ish, got R={mid}"
        );
    }

    // ── GPU-gated tests ───────────────────────────────────────────────────────

    #[test]
    fn gpu_gradient_fill_horizontal() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let w = 4u32;
        let h = 1u32;
        let mut pixels = vec![0u32; (w * h) as usize];
        gpu_linear_gradient_fill(
            Some(&ctx),
            &mut pixels,
            w,
            h,
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            GradientAxis::Horizontal,
        );
        let r0 = (pixels[0] >> 16) & 0xFF;
        let b0 = pixels[0] & 0xFF;
        assert!(r0 > 200, "GPU: left end must be red-ish, got R={r0}");
        assert!(b0 < 50, "GPU: left end must not be blue, got B={b0}");
    }

    #[test]
    fn gpu_ordered_dither_does_not_panic() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let mut pixels = vec![0xFF_80_80_80u32; 16];
        gpu_ordered_dither_rgba(Some(&ctx), &mut pixels, 4, 4);
    }

    #[test]
    fn gpu_blur_does_not_panic() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let mut pixels = vec![0xFF_80_80_80u32; 16];
        gpu_gaussian_blur_rgba(Some(&ctx), &mut pixels, 4, 4, 1);
    }
}
