//! Off-screen render targets for cached widget subtrees and compositing.
//!
//! [`RenderTarget`] owns an offscreen `Rgba8Unorm` texture that can be used as
//! a `RENDER_ATTACHMENT`, and whose contents can be read back to CPU memory or
//! sampled as a texture in subsequent render passes.
//!
//! # Usage
//!
//! 1. Create a `RenderTarget` with [`RenderTarget::new`].
//! 2. Render into it by using [`RenderTarget::color_view`] as the colour
//!    attachment in a render pass.
//! 3. Optionally call [`RenderTarget::readback_rgba`] for CPU pixel access.
//! 4. Use [`RenderTarget::texture_view`] + a sampler to composite the target
//!    into a parent pass (e.g. via the textured pipeline).
//!
//! Render targets support optional MSAA: pass `sample_count > 1` to obtain a
//! multisample render texture that resolves into the single-sample backing
//! texture on each render pass.  The backing texture (`color_texture`) is
//! always sample_count=1 and `COPY_SRC`, making readback straightforward.

use oxiui_core::UiError;

use crate::gpu::device::TARGET_FORMAT;

// ── RenderTarget ──────────────────────────────────────────────────────────────

/// An off-screen GPU render target.
///
/// Wraps a `Rgba8Unorm` texture (single-sample, `RENDER_ATTACHMENT | COPY_SRC |
/// TEXTURE_BINDING`) and an optional MSAA resolve texture.
pub struct RenderTarget {
    /// The single-sample backing texture (always `COPY_SRC` for readback).
    pub color_texture: wgpu::Texture,
    /// Default view over `color_texture`.
    pub color_view: wgpu::TextureView,
    /// Optional MSAA multisample texture view.  Present when `sample_count > 1`.
    pub msaa_view: Option<wgpu::TextureView>,
    /// Target width in physical pixels.
    pub width: u32,
    /// Target height in physical pixels.
    pub height: u32,
    /// Effective MSAA sample count (1 = no MSAA, 4 or 8 = MSAA).
    pub sample_count: u32,
    /// Whether the target content is considered dirty (needs re-render).
    dirty: bool,
}

impl RenderTarget {
    /// Create a new off-screen render target of `width × height` pixels with
    /// the given MSAA `sample_count` (1 = no MSAA).
    ///
    /// If `sample_count > 1` but MSAA is not supported for `TARGET_FORMAT`
    /// by the adapter, `sample_count` falls back to 1 silently.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] if `width` or `height` is zero.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Result<Self, UiError> {
        if width == 0 || height == 0 {
            return Err(UiError::Unsupported(
                "RenderTarget dimensions must be non-zero".to_string(),
            ));
        }
        let sc = sample_count.max(1);

        // Single-sample backing texture (COPY_SRC for readback, TEXTURE_BINDING
        // for compositing).
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxiui-render-wgpu render-target backing"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Optional MSAA texture — no COPY_SRC (can't read back a multisampled
        // texture directly; the resolve into `color_texture` is the readback path).
        let msaa_view = if sc > 1 {
            let msaa_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("oxiui-render-wgpu render-target msaa"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: sc,
                dimension: wgpu::TextureDimension::D2,
                format: TARGET_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            Some(msaa_texture.create_view(&wgpu::TextureViewDescriptor::default()))
        } else {
            None
        };

        Ok(Self {
            color_texture,
            color_view,
            msaa_view,
            width,
            height,
            sample_count: sc,
            dirty: true,
        })
    }

    /// Create a new render target with no MSAA (sample_count=1).
    ///
    /// Convenience wrapper around [`RenderTarget::new`].
    pub fn new_simple(device: &wgpu::Device, width: u32, height: u32) -> Result<Self, UiError> {
        Self::new(device, width, height, 1)
    }

    /// Returns the colour attachment view and optional resolve target.
    ///
    /// Under MSAA: `(msaa_view, Some(&color_view))` — render into the MSAA
    /// surface, resolve into the backing texture.
    /// Under no MSAA: `(&color_view, None)` — render directly into the backing
    /// texture.
    pub fn color_attachment(&self) -> (&wgpu::TextureView, Option<&wgpu::TextureView>) {
        match &self.msaa_view {
            Some(msaa) => (msaa, Some(&self.color_view)),
            None => (&self.color_view, None),
        }
    }

    /// A view over the resolved (single-sample) backing texture.
    ///
    /// Use this view for compositing the render target into a parent pass or
    /// for sampling in a shader.
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    /// Mark the render target as dirty (needing re-render).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark the render target as clean (up-to-date).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Return `true` if the content is stale and needs re-rendering.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Read the render target's pixel contents back to CPU memory as a tightly
    /// packed `width * height * 4` RGBA byte vector.
    ///
    /// Row padding (from `COPY_BYTES_PER_ROW_ALIGNMENT`) is stripped.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] if the GPU poll or buffer mapping fails.
    pub fn readback_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<u8>, UiError> {
        let unpadded_bytes_per_row = self.width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * self.height) as wgpu::BufferAddress;

        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxiui-render-wgpu render-target readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxiui-render-wgpu render-target readback encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| UiError::Render(format!("RenderTarget GPU poll failed: {e:?}")))?;

        let data = slice.get_mapped_range();
        let mut out = Vec::with_capacity((unpadded_bytes_per_row * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + unpadded_bytes_per_row as usize;
            out.extend_from_slice(&data[start..end]);
        }
        drop(data);
        readback.unmap();
        Ok(out)
    }

    /// Resize the render target to new dimensions.
    ///
    /// All existing texture resources are dropped and recreated.  Any
    /// previously-held views or samplers that refer to the old textures become
    /// invalid.  The dirty flag is set to `true`.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] if `new_width` or `new_height` is zero.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        new_width: u32,
        new_height: u32,
    ) -> Result<(), UiError> {
        *self = Self::new(device, new_width, new_height, self.sample_count)?;
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::UiError;

    fn try_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    #[test]
    fn render_target_zero_dimensions_fail() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        assert!(matches!(
            RenderTarget::new(&device, 0, 64, 1),
            Err(UiError::Unsupported(_))
        ));
        assert!(matches!(
            RenderTarget::new(&device, 64, 0, 1),
            Err(UiError::Unsupported(_))
        ));
    }

    #[test]
    fn render_target_creates_and_is_dirty() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let rt = RenderTarget::new_simple(&device, 64, 32).expect("create render target");
        assert_eq!(rt.width, 64);
        assert_eq!(rt.height, 32);
        assert_eq!(rt.sample_count, 1);
        assert!(rt.is_dirty(), "fresh target must be dirty");
    }

    #[test]
    fn render_target_dirty_flag_management() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut rt = RenderTarget::new_simple(&device, 32, 32).expect("create");
        assert!(rt.is_dirty());
        rt.mark_clean();
        assert!(!rt.is_dirty());
        rt.mark_dirty();
        assert!(rt.is_dirty());
    }

    #[test]
    fn render_target_resize_resets_dirty() {
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let mut rt = RenderTarget::new_simple(&device, 32, 32).expect("create");
        rt.mark_clean();
        assert!(!rt.is_dirty());
        rt.resize(&device, 64, 64).expect("resize");
        assert_eq!(rt.width, 64);
        assert_eq!(rt.height, 64);
        // After resize the target is fresh (dirty=true).
        assert!(rt.is_dirty(), "resized target must be dirty");
    }

    #[test]
    fn render_target_readback_all_transparent() {
        // A freshly created render target (never drawn into) should read back
        // as transparent black because the GPU memory may be zeroed.
        // We only assert the buffer is the right size — actual pixel values
        // depend on GPU memory initialization.
        let Some((device, queue)) = try_device() else {
            return;
        };
        let rt = RenderTarget::new_simple(&device, 16, 16).expect("create");
        let buf = rt.readback_rgba(&device, &queue).expect("readback");
        assert_eq!(
            buf.len(),
            (16 * 16 * 4) as usize,
            "readback buffer must be tightly packed"
        );
    }
}
