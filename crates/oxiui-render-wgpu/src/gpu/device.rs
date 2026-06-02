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
    pub color_texture: wgpu::Texture,
    /// A default view over [`color_texture`](GpuContext::color_texture).
    pub color_view: wgpu::TextureView,
    /// Target width in physical pixels.
    pub width: u32,
    /// Target height in physical pixels.
    pub height: u32,
}

impl GpuContext {
    /// Initialise a headless GPU context with an offscreen target of
    /// `width × height` pixels.
    ///
    /// # Errors
    ///
    /// * [`UiError::Unsupported`] — no GPU adapter is available (the caller
    ///   should treat this as "skip", not a hard failure), or the requested
    ///   target dimensions are zero.
    /// * [`UiError::Backend`] — the adapter was found but the device request
    ///   failed.
    pub fn headless(width: u32, height: u32) -> Result<Self, UiError> {
        if width == 0 || height == 0 {
            return Err(UiError::Unsupported(
                "headless target dimensions must be non-zero".to_string(),
            ));
        }

        // `Instance::default()` enables `Backends::all()` with no display handle
        // — exactly what a headless (no-window) context needs.  On this host
        // that resolves to the Metal backend.
        let instance = wgpu::Instance::default();

        // Block on the async adapter request.  A `None`/`Err` here means no GPU
        // is usable on this host → surface as Unsupported so tests can skip.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| UiError::Unsupported(format!("no GPU adapter available: {e}")))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("oxiui-render-wgpu headless device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| UiError::Backend(format!("GPU device request failed: {e}")))?;

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

        Ok(Self {
            device,
            queue,
            color_texture,
            color_view,
            width,
            height,
        })
    }
}
