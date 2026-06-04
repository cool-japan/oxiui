//! Texture upload utilities for the textured pipeline.
//!
//! [`upload_image`] converts an [`ImageData`] into a GPU texture + sampler pair
//! ready to bind in a render pass.  `queue.write_texture` is used directly — it
//! does **not** require 256-byte row alignment (unlike `copy_buffer_to_texture`).
//!
//! [`TexturedDraw`] carries the pre-built [`TexVertex`] list alongside the
//! image data and per-draw state needed by the render loop in `renderer.rs`.

use crate::gpu::buffer::TexVertex;
use crate::gpu::device::TARGET_FORMAT;
use oxiui_core::paint::{ImageData, ImageFilter};
use oxiui_core::UiError;

// ── TexturedDraw ──────────────────────────────────────────────────────────────

/// One textured draw call: a vertex list plus the source image and draw state.
///
/// Built during `build_geometry` and consumed in the textured pass of
/// `execute()`.
pub struct TexturedDraw {
    /// Pre-built vertices (2 triangles per quad, 9 × 6 for nine-slice).
    pub verts: Vec<TexVertex>,
    /// Owned copy of the source image.
    pub image: ImageData,
    /// Texture filter to apply when uploading and sampling.
    pub filter: ImageFilter,
    /// Optional hardware scissor rect `[x, y, w, h]` in physical pixels.
    pub scissor: Option<[u32; 4]>,
}

// ── upload_image ──────────────────────────────────────────────────────────────

/// Upload `image` to a new `Rgba8Unorm` GPU texture and create a matching
/// sampler.
///
/// Uses `queue.write_texture` which does NOT require row-alignment padding —
/// `bytes_per_row = image.width * 4` is correct here.
///
/// # Errors
///
/// Returns [`UiError::Render`] when the image dimensions are zero.
pub fn upload_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &ImageData,
    filter: ImageFilter,
) -> Result<(wgpu::TextureView, wgpu::Sampler), UiError> {
    if image.width == 0 || image.height == 0 {
        return Err(UiError::Render("zero-dimension image".to_string()));
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("oxiui-render-wgpu uploaded texture"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width * 4),
            rows_per_image: Some(image.height),
        },
        wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let filter_mode = match filter {
        ImageFilter::Nearest => wgpu::FilterMode::Nearest,
        ImageFilter::Bilinear => wgpu::FilterMode::Linear,
    };

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("oxiui-render-wgpu texture sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: filter_mode,
        min_filter: filter_mode,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 32.0,
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    });

    Ok((view, sampler))
}
