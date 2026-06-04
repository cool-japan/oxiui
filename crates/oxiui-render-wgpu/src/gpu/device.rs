//! Headless wgpu device acquisition and the offscreen colour target.
//!
//! [`GpuContext::headless`] performs the full no-window initialisation chain:
//! `Instance` → `request_adapter` → `request_device`/`queue` → an offscreen
//! colour texture.  Every step that can fail returns a [`UiError`]; in
//! particular, a missing adapter (no GPU available) yields
//! [`UiError::Unsupported`] so callers — most importantly the headless tests —
//! can *gracefully skip* rather than panic on machines without a usable GPU.
//!
//! The offscreen target uses [`wgpu::TextureFormat::Rgba8Unorm`] (the *linear*,
//! non-sRGB variant) so that a solid colour written by the fragment shader is
//! copied back byte-for-byte.  An sRGB target would apply the OETF on store and
//! distort the asserted pixel values.

use oxiui_core::UiError;

/// The non-sRGB offscreen colour format.  Chosen so solid-colour readback is
/// byte-exact (sRGB encoding would skew the stored values).
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// An initialised headless GPU context: device, queue, and an offscreen colour
/// texture (plus its view) sized to the requested surface.
///
/// No window or swap-chain surface is involved — rendering goes to the
/// offscreen texture, which can then be read back to CPU memory with
/// [`crate::gpu::renderer`]'s readback path.
pub struct GpuContext {
    /// The logical GPU device.
    pub device: wgpu::Device,
    /// The command queue for the device.
    pub queue: wgpu::Queue,
    /// The offscreen colour texture (`RENDER_ATTACHMENT | COPY_SRC`).
    /// This is always the resolve/readback target (sample_count=1).
    pub color_texture: wgpu::Texture,
    /// A default view over [`color_texture`](GpuContext::color_texture).
    /// When MSAA is active this is the *resolve* target; when no MSAA it is
    /// the direct render target.
    pub color_view: wgpu::TextureView,
    /// Target width in physical pixels.
    pub width: u32,
    /// Target height in physical pixels.
    pub height: u32,
    /// The effective MSAA sample count (1 = no MSAA, 4 or 8 = MSAA).
    pub sample_count: u32,
    /// The MSAA multisample texture view, present only when `sample_count > 1`.
    pub msaa_view: Option<wgpu::TextureView>,
}

impl GpuContext {
    /// Initialise a headless GPU context with an offscreen target of
    /// `width × height` pixels and the specified MSAA sample count.
    ///
    /// The `requested` sample count is validated against adapter capabilities.
    /// If the adapter does not support the requested count, `sample_count=1`
    /// (no MSAA) is used instead.  Only 4 and 8 are recognised as valid MSAA
    /// counts; anything else silently falls back to 1.
    ///
    /// # Errors
    ///
    /// * [`UiError::Unsupported`] — no GPU adapter is available, or the
    ///   requested target dimensions are zero.
    /// * [`UiError::Backend`] — the adapter was found but the device request
    ///   failed.
    pub fn headless_with_sample_count(
        width: u32,
        height: u32,
        requested: u32,
    ) -> Result<Self, UiError> {
        if width == 0 || height == 0 {
            return Err(UiError::Unsupported(
                "headless target dimensions must be non-zero".to_string(),
            ));
        }

        // `Instance::default()` enables `Backends::all()` with no display handle
        // — exactly what a headless (no-window) context needs.
        let instance = wgpu::Instance::default();

        // Block on the async adapter request.  A `None`/`Err` here means no GPU
        // is usable on this host → surface as Unsupported so tests can skip.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| UiError::Unsupported(format!("no GPU adapter available: {e}")))?;

        // Validate requested MSAA count against adapter texture format capabilities.
        let effective_count = match requested {
            4 => {
                let flags = adapter.get_texture_format_features(TARGET_FORMAT).flags;
                if flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4) {
                    4
                } else {
                    1
                }
            }
            8 => {
                let flags = adapter.get_texture_format_features(TARGET_FORMAT).flags;
                if flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X8) {
                    8
                } else {
                    1
                }
            }
            _ => 1,
        };

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("oxiui-render-wgpu headless device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| UiError::Backend(format!("GPU device request failed: {e}")))?;

        // The offscreen colour target is always sample_count=1 so that
        // `copy_texture_to_buffer` (readback) works.  When MSAA is active it
        // acts as the *resolve* target.
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxiui-render-wgpu offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Allocate MSAA multisample texture when effective_count > 1.
        // MSAA textures must NOT have COPY_SRC — they cannot be read back
        // directly; they are resolved into the colour_texture above.
        let msaa_view = if effective_count > 1 {
            let msaa_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("oxiui-render-wgpu msaa color"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: effective_count,
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
            device,
            queue,
            color_texture,
            color_view,
            width,
            height,
            sample_count: effective_count,
            msaa_view,
        })
    }

    /// Initialise a headless GPU context with an offscreen target of
    /// `width × height` pixels without MSAA (sample_count=1).
    ///
    /// This delegates to [`headless_with_sample_count`] with `requested=1`,
    /// preserving the exact same code path as before MSAA support was added.
    ///
    /// # Errors
    ///
    /// * [`UiError::Unsupported`] — no GPU adapter is available (the caller
    ///   should treat this as "skip", not a hard failure), or the requested
    ///   target dimensions are zero.
    /// * [`UiError::Backend`] — the adapter was found but the device request
    ///   failed.
    ///
    /// [`headless_with_sample_count`]: GpuContext::headless_with_sample_count
    pub fn headless(width: u32, height: u32) -> Result<Self, UiError> {
        Self::headless_with_sample_count(width, height, 1)
    }

    /// Returns the colour attachment view and optional resolve target.
    ///
    /// Under MSAA (sample_count > 1): render into `msaa_view`, resolve into
    /// `color_view`.
    /// Under no MSAA (sample_count == 1): render directly into `color_view`,
    /// no resolve.
    pub fn color_attachment(&self) -> (&wgpu::TextureView, Option<&wgpu::TextureView>) {
        match &self.msaa_view {
            Some(msaa) => (msaa, Some(&self.color_view)),
            None => (&self.color_view, None),
        }
    }

    /// Returns the effective MSAA sample count (1 = no MSAA, 4 or 8 = MSAA).
    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Resize the offscreen colour target to `new_width × new_height` pixels.
    ///
    /// Recreates only the colour texture and its view (and the MSAA texture if
    /// active).  The device, queue, and sample count are preserved.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] if either dimension is zero.
    pub fn resize(&mut self, new_width: u32, new_height: u32) -> Result<(), UiError> {
        if new_width == 0 || new_height == 0 {
            return Err(UiError::Unsupported(
                "GpuContext resize dimensions must be non-zero".to_string(),
            ));
        }

        // Recreate the single-sample colour target.
        let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxiui-render-wgpu offscreen target (resized)"),
            size: wgpu::Extent3d {
                width: new_width,
                height: new_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Recreate the MSAA texture if necessary.
        let msaa_view = if self.sample_count > 1 {
            let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("oxiui-render-wgpu msaa color (resized)"),
                size: wgpu::Extent3d {
                    width: new_width,
                    height: new_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: TARGET_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            Some(msaa_texture.create_view(&wgpu::TextureViewDescriptor::default()))
        } else {
            None
        };

        self.color_texture = color_texture;
        self.color_view = color_view;
        self.msaa_view = msaa_view;
        self.width = new_width;
        self.height = new_height;

        Ok(())
    }
}
