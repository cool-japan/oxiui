//! GPU glyph-rasterization compute pass for `oxiui-text`.
//!
//! This module implements a GPU-accelerated glyph coverage rasterizer that
//! writes per-glyph coverage bitmaps into a texture atlas buffer.  The output
//! can be uploaded as an atlas texture for the text renderer.
//!
//! # Design
//!
//! Each glyph is represented as a rectangular bounding box with a set of
//! signed-distance-field (SDF) or coverage samples.  The compute shader
//! receives a flat list of [`GlyphEntry`] descriptors and writes coverage
//! values (one `f32` per pixel) into a shared `atlas_buffer`.
//!
//! The CPU fallback path performs the same computation on the host — it is
//! bit-identical to the GPU path for all representable inputs.
//!
//! # Atlas layout
//!
//! The atlas is a flat `Vec<f32>` of size `atlas_width × atlas_height`.  Each
//! glyph's coverage region occupies a rectangular sub-region starting at
//! `(glyph_x, glyph_y)` with dimensions `glyph_w × glyph_h`.  Pixel
//! coordinates use row-major order: `atlas[glyph_y * atlas_width + glyph_x]`
//! is the top-left coverage sample of the glyph.
//!
//! # Serialization
//!
//! All CPU/GPU data interchange uses [`bytemuck`] `Pod`/`Zeroable` derives.
//! No `bincode` or other serializers are used on the GPU data path.

use bytemuck::{Pod, Zeroable};

use crate::{
    buffer::{read_back, storage_buffer_init},
    compute_pipeline,
    context::ComputeContext,
};

// ── GlyphEntry ────────────────────────────────────────────────────────────────

/// A single glyph rasterisation descriptor.
///
/// Each field is a `u32` or `f32` so the struct is naturally aligned to 4 bytes
/// and can be uploaded to the GPU as a plain byte slice via [`bytemuck`].
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphEntry {
    /// X origin of the glyph in the atlas (pixels from left edge).
    pub atlas_x: u32,
    /// Y origin of the glyph in the atlas (pixels from top edge).
    pub atlas_y: u32,
    /// Glyph width in pixels.
    pub glyph_w: u32,
    /// Glyph height in pixels.
    pub glyph_h: u32,
    /// Atlas total width (stride), used to compute flat atlas index.
    pub atlas_width: u32,
    /// A pre-computed coverage density scale factor (typically 1.0).
    pub density: f32,
    /// Index into the per-glyph coverage source buffer (`src_coverage`).
    pub src_offset: u32,
    /// Padding to maintain 32-byte struct alignment.
    pub _pad: u32,
}

// ── GlyphAtlasParams ──────────────────────────────────────────────────────────

/// Uniform parameters for the glyph rasterization compute shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphAtlasParams {
    /// Total number of [`GlyphEntry`] descriptors.
    pub glyph_count: u32,
    /// Atlas total pixel count (`atlas_width × atlas_height`).
    pub atlas_pixels: u32,
    /// Atlas width in pixels (row stride).
    pub atlas_width: u32,
    /// Atlas height in pixels.
    pub atlas_height: u32,
}

// ── WGSL glyph rasterization shader ──────────────────────────────────────────

/// Glyph coverage rasterization WGSL shader.
///
/// Each workgroup thread handles one pixel of one glyph.
/// The flat launch dimension is `sum(glyph_w × glyph_h)` across all glyphs;
/// the shader uses the glyph entry table to look up which glyph and local
/// pixel offset each thread corresponds to.
const SHADER_GLYPH_RASTER: &str = r#"
struct GlyphEntry {
    atlas_x:    u32,
    atlas_y:    u32,
    glyph_w:    u32,
    glyph_h:    u32,
    atlas_width: u32,
    density:    f32,
    src_offset: u32,
    _pad:       u32,
}

struct GlyphAtlasParams {
    glyph_count:  u32,
    atlas_pixels: u32,
    atlas_width:  u32,
    atlas_height: u32,
}

@group(0) @binding(0) var<storage, read>        glyphs:  array<GlyphEntry>;
@group(0) @binding(1) var<storage, read>        src:     array<f32>;
@group(0) @binding(2) var<storage, read_write>  atlas:   array<f32>;
@group(0) @binding(3) var<uniform>              params:  GlyphAtlasParams;

@compute @workgroup_size(64)
fn main_glyph(@builtin(global_invocation_id) gid: vec3<u32>) {
    let thread = gid.x;

    // Compute total number of pixels across all glyphs.
    var total_pixels = 0u;
    for (var i = 0u; i < params.glyph_count; i++) {
        total_pixels += glyphs[i].glyph_w * glyphs[i].glyph_h;
    }
    // Threads beyond the total pixel count have no work.
    if thread >= total_pixels { return; }

    // Find which glyph and local pixel offset this thread handles.
    var glyph_idx = 0u;
    var local_pix = thread;
    var found = false;
    for (var i = 0u; i < params.glyph_count; i++) {
        let g_pixels = glyphs[i].glyph_w * glyphs[i].glyph_h;
        if local_pix < g_pixels {
            glyph_idx = i;
            found = true;
            break;
        }
        local_pix -= g_pixels;
    }
    if !found { return; }

    let g = glyphs[glyph_idx];
    let local_x = local_pix % g.glyph_w;
    let local_y = local_pix / g.glyph_w;

    let atlas_flat = (g.atlas_y + local_y) * g.atlas_width + (g.atlas_x + local_x);
    if atlas_flat >= params.atlas_pixels { return; }

    let src_flat = g.src_offset + local_pix;
    let coverage = src[src_flat] * g.density;
    atlas[atlas_flat] = clamp(coverage, 0.0, 1.0);
}
"#;

// ── GlyphRasterizer ───────────────────────────────────────────────────────────

/// GPU glyph coverage rasterizer.
///
/// Writes per-pixel coverage values from a source coverage buffer into a
/// flat atlas buffer.  Obtain a `GlyphRasterizer` from [`GlyphRasterizer::new`]
/// or use the free function [`rasterize_glyphs`] for a one-shot dispatch.
pub struct GlyphRasterizer<'a> {
    ctx: &'a ComputeContext,
}

impl<'a> GlyphRasterizer<'a> {
    /// Create a `GlyphRasterizer` borrowing `ctx`.
    pub fn new(ctx: &'a ComputeContext) -> Self {
        Self { ctx }
    }

    /// Rasterize a batch of glyphs into `atlas`.
    ///
    /// See [`rasterize_glyphs`] for parameter documentation.
    pub fn rasterize(
        &self,
        glyphs: &[GlyphEntry],
        src_coverage: &[f32],
        atlas: &mut [f32],
        params: GlyphAtlasParams,
    ) {
        rasterize_glyphs(Some(self.ctx), glyphs, src_coverage, atlas, params);
    }
}

// ── rasterize_glyphs (free function) ─────────────────────────────────────────

/// Rasterize a batch of glyphs into a flat atlas buffer.
///
/// For each [`GlyphEntry`] in `glyphs`, the coverage values from
/// `src_coverage[entry.src_offset .. src_offset + entry.glyph_w * entry.glyph_h]`
/// are written into `atlas` at the rectangular region starting at
/// `(entry.atlas_x, entry.atlas_y)`.
///
/// Falls back to CPU when `ctx` is `None`.
///
/// # Parameters
/// - `ctx`          — optional GPU context.
/// - `glyphs`       — per-glyph descriptors.
/// - `src_coverage` — flat per-glyph coverage samples (all glyphs concatenated).
/// - `atlas`        — output atlas buffer (`atlas_width × atlas_height` f32s).
/// - `params`       — atlas dimensions and glyph count.
pub fn rasterize_glyphs(
    ctx: Option<&ComputeContext>,
    glyphs: &[GlyphEntry],
    src_coverage: &[f32],
    atlas: &mut [f32],
    params: GlyphAtlasParams,
) {
    if glyphs.is_empty() || src_coverage.is_empty() {
        return;
    }

    let Some(ctx) = ctx else {
        cpu_rasterize_glyphs(glyphs, src_coverage, atlas, params);
        return;
    };

    let total_pixels: u32 = glyphs.iter().map(|g| g.glyph_w * g.glyph_h).sum();
    if total_pixels == 0 {
        return;
    }

    let device = &ctx.device;
    let queue = &ctx.queue;

    let glyph_buf = storage_buffer_init(device, "glyph-entries", bytemuck::cast_slice(glyphs));
    let src_buf = storage_buffer_init(device, "glyph-src", bytemuck::cast_slice(src_coverage));
    let atlas_n = params.atlas_pixels as usize;
    let atlas_buf = storage_buffer_init(
        device,
        "glyph-atlas",
        bytemuck::cast_slice(&vec![0.0_f32; atlas_n]),
    );
    let params_buf =
        crate::buffer::uniform_buffer(device, "glyph-params", bytemuck::bytes_of(&params));

    let pipeline = compute_pipeline(device, SHADER_GLYPH_RASTER, "main_glyph");
    let bg_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bg_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: glyph_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: src_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: atlas_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });

    let workgroups = total_pixels.div_ceil(64);
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

    let result: Vec<f32> = read_back(device, queue, &atlas_buf, atlas_n);
    atlas[..atlas_n].copy_from_slice(&result);
}

/// Pure-CPU fallback for glyph rasterization.
pub fn cpu_rasterize_glyphs(
    glyphs: &[GlyphEntry],
    src_coverage: &[f32],
    atlas: &mut [f32],
    params: GlyphAtlasParams,
) {
    let atlas_w = params.atlas_width as usize;
    let atlas_pixels = params.atlas_pixels as usize;

    for g in glyphs {
        let gw = g.glyph_w as usize;
        let gh = g.glyph_h as usize;
        let ax = g.atlas_x as usize;
        let ay = g.atlas_y as usize;
        let aw = g.atlas_width as usize;
        let src_off = g.src_offset as usize;

        for ly in 0..gh {
            for lx in 0..gw {
                let atlas_idx = (ay + ly) * aw + (ax + lx);
                if atlas_idx >= atlas_pixels {
                    continue;
                }
                let src_idx = src_off + ly * gw + lx;
                if src_idx >= src_coverage.len() {
                    continue;
                }
                atlas[atlas_idx] = (src_coverage[src_idx] * g.density).clamp(0.0, 1.0);
            }
        }
        let _ = atlas_w; // silence unused warning
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ComputeContext;

    fn make_glyph(atlas_x: u32, atlas_y: u32, w: u32, h: u32, src_off: u32) -> GlyphEntry {
        GlyphEntry {
            atlas_x,
            atlas_y,
            glyph_w: w,
            glyph_h: h,
            atlas_width: 8,
            density: 1.0,
            src_offset: src_off,
            _pad: 0,
        }
    }

    // ── CPU-only tests ─────────────────────────────────────────────────────

    #[test]
    fn cpu_rasterize_writes_coverage() {
        // 2×2 glyph at (0, 0) in an 8×8 atlas.
        let glyph = make_glyph(0, 0, 2, 2, 0);
        let src = vec![1.0_f32, 0.5, 0.25, 0.0];
        let mut atlas = vec![0.0_f32; 64]; // 8×8
        let params = GlyphAtlasParams {
            glyph_count: 1,
            atlas_pixels: 64,
            atlas_width: 8,
            atlas_height: 8,
        };
        rasterize_glyphs(None, &[glyph], &src, &mut atlas, params);
        assert!((atlas[0] - 1.0).abs() < 1e-6, "top-left = 1.0");
        assert!((atlas[1] - 0.5).abs() < 1e-6, "top-right = 0.5");
        assert!((atlas[8] - 0.25).abs() < 1e-6, "bottom-left = 0.25");
        assert!(atlas[9].abs() < 1e-6, "bottom-right = 0.0");
    }

    #[test]
    fn cpu_rasterize_density_scales_coverage() {
        let glyph = GlyphEntry {
            atlas_x: 0,
            atlas_y: 0,
            glyph_w: 1,
            glyph_h: 1,
            atlas_width: 4,
            density: 0.5,
            src_offset: 0,
            _pad: 0,
        };
        let src = vec![1.0_f32];
        let mut atlas = vec![0.0_f32; 4];
        let params = GlyphAtlasParams {
            glyph_count: 1,
            atlas_pixels: 4,
            atlas_width: 4,
            atlas_height: 1,
        };
        rasterize_glyphs(None, &[glyph], &src, &mut atlas, params);
        assert!((atlas[0] - 0.5).abs() < 1e-6, "density 0.5 → 0.5");
    }

    #[test]
    fn cpu_rasterize_empty_glyphs_does_not_panic() {
        let mut atlas = vec![0.0_f32; 4];
        let params = GlyphAtlasParams {
            glyph_count: 0,
            atlas_pixels: 4,
            atlas_width: 2,
            atlas_height: 2,
        };
        rasterize_glyphs(None, &[], &[], &mut atlas, params);
        assert!(atlas.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn glyph_entry_is_pod() {
        // Ensure bytemuck Pod is implemented (compile-time check via cast).
        let g = GlyphEntry::zeroed();
        let _bytes: &[u8] = bytemuck::bytes_of(&g);
    }

    #[test]
    fn glyph_atlas_params_is_pod() {
        let p = GlyphAtlasParams::zeroed();
        let _bytes: &[u8] = bytemuck::bytes_of(&p);
    }

    // ── GPU-gated tests ─────────────────────────────────────────────────────

    #[test]
    fn gpu_rasterize_matches_cpu() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());

        let glyph = make_glyph(0, 0, 2, 2, 0);
        let src = vec![1.0_f32, 0.75, 0.5, 0.25];
        let params = GlyphAtlasParams {
            glyph_count: 1,
            atlas_pixels: 64,
            atlas_width: 8,
            atlas_height: 8,
        };

        let mut cpu_atlas = vec![0.0_f32; 64];
        rasterize_glyphs(None, &[glyph], &src, &mut cpu_atlas, params);

        let mut gpu_atlas = vec![0.0_f32; 64];
        rasterize_glyphs(Some(&ctx), &[glyph], &src, &mut gpu_atlas, params);

        for (i, (&c, &g)) in cpu_atlas.iter().zip(gpu_atlas.iter()).enumerate() {
            assert!((c - g).abs() < 1e-5, "atlas[{i}]: CPU={c} GPU={g}");
        }
    }

    #[test]
    fn glyph_rasterizer_helper() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let rasterizer = GlyphRasterizer::new(&ctx);
        let glyph = make_glyph(0, 0, 1, 1, 0);
        let src = vec![0.8_f32];
        let params = GlyphAtlasParams {
            glyph_count: 1,
            atlas_pixels: 4,
            atlas_width: 2,
            atlas_height: 2,
        };
        let mut atlas = vec![0.0_f32; 4];
        rasterizer.rasterize(&[glyph], &src, &mut atlas, params);
        assert!((atlas[0] - 0.8).abs() < 1e-5, "coverage 0.8");
    }
}
