//! HDR and wide-gamut surface format selection for the wgpu backend.
//!
//! This module provides:
//!
//! - [`SurfaceColorFormat`] — an enumeration of supported colour formats with
//!   their metadata (bits per channel, colour gamut, HDR capability).
//! - [`HdrGpuContext`] — a headless GPU context that uses `Rgba16Float`
//!   (wide-gamut / HDR) instead of the default `Rgba8Unorm`.
//! - [`select_surface_format`] — a heuristic that picks the best available
//!   format from a list of adapter-supported formats, preferring HDR variants
//!   when available and the caller opts in.
//!
//! # Colour-space handling in the fragment shader
//!
//! `Rgba8Unorm` stores gamma-encoded sRGB values; `Rgba16Float` stores linear
//! light values in the Rec.2020 or Display-P3 colour space (depending on the
//! OS colour management layer).  When rendering into an `Rgba16Float` target
//! the fragment shader should output linear light values; the display pipeline
//! applies the appropriate OETF (transfer function) on presentation.
//!
//! For **headless / offscreen** rendering (`HdrGpuContext`) the application
//! reads back raw `f16` values; it is the caller's responsibility to apply any
//! tone-mapping required before displaying or saving the image.

use oxiui_core::UiError;

// ── SurfaceColorFormat ────────────────────────────────────────────────────────

/// A supported colour surface format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SurfaceColorFormat {
    /// 8 bits per channel, sRGB-encoded.  Standard SDR format.
    #[default]
    Rgba8Unorm,
    /// 8 bits per channel, gamma-corrected sRGB (GPU applies sRGB encoding on
    /// store / decoding on sample).
    Rgba8UnormSrgb,
    /// 8 bits per channel, BGRA byte order (common on Windows/Metal).
    Bgra8Unorm,
    /// 8 bits per channel, BGRA, sRGB.
    Bgra8UnormSrgb,
    /// 10 bits per colour channel + 2-bit alpha, sRGB.  Increases precision for
    /// SDR content; may be used for extended-range (scRGB) content.
    Rgb10a2Unorm,
    /// 16 bits per channel, floating-point (linear light).  HDR-capable.
    Rgba16Float,
}

impl SurfaceColorFormat {
    /// Return the corresponding [`wgpu::TextureFormat`].
    pub fn wgpu_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            Self::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
            Self::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
            Self::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
            Self::Rgb10a2Unorm => wgpu::TextureFormat::Rgb10a2Unorm,
            Self::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        }
    }

    /// Return `true` if this format is capable of representing HDR values
    /// (i.e. luminance > 1.0 in linear light).
    pub fn is_hdr(self) -> bool {
        matches!(self, Self::Rgba16Float)
    }

    /// Return the nominal bits per channel for this format.
    pub fn bits_per_channel(self) -> u32 {
        match self {
            Self::Rgba8Unorm | Self::Rgba8UnormSrgb | Self::Bgra8Unorm | Self::Bgra8UnormSrgb => 8,
            Self::Rgb10a2Unorm => 10,
            Self::Rgba16Float => 16,
        }
    }

    /// Return `true` if the GPU pipeline should output *linear* light values
    /// when rendering into this format (as opposed to gamma-encoded sRGB).
    ///
    /// `Rgba16Float` expects linear light.  The `Unorm` variants are usually
    /// treated as gamma-encoded sRGB in practice; the `UnormSrgb` variants
    /// have explicit sRGB encoding built into the attachment load/store.
    pub fn expects_linear_light(self) -> bool {
        matches!(self, Self::Rgba16Float)
    }
}

// ── select_surface_format ─────────────────────────────────────────────────────

/// Choose the best format from `supported_formats` given a preference.
///
/// When `prefer_hdr` is `true`, `Rgba16Float` is preferred if available.
/// Otherwise, sRGB variants are preferred over linear unorm (for correct gamma
/// on standard SDR displays).  Falls back to the first format in the list, or
/// [`SurfaceColorFormat::Rgba8Unorm`] if the list is empty.
pub fn select_surface_format(
    supported_formats: &[wgpu::TextureFormat],
    prefer_hdr: bool,
) -> SurfaceColorFormat {
    // Map known wgpu formats to our enum.
    let mapped: Vec<SurfaceColorFormat> = supported_formats
        .iter()
        .filter_map(|&f| match f {
            wgpu::TextureFormat::Rgba8Unorm => Some(SurfaceColorFormat::Rgba8Unorm),
            wgpu::TextureFormat::Rgba8UnormSrgb => Some(SurfaceColorFormat::Rgba8UnormSrgb),
            wgpu::TextureFormat::Bgra8Unorm => Some(SurfaceColorFormat::Bgra8Unorm),
            wgpu::TextureFormat::Bgra8UnormSrgb => Some(SurfaceColorFormat::Bgra8UnormSrgb),
            wgpu::TextureFormat::Rgb10a2Unorm => Some(SurfaceColorFormat::Rgb10a2Unorm),
            wgpu::TextureFormat::Rgba16Float => Some(SurfaceColorFormat::Rgba16Float),
            _ => None,
        })
        .collect();

    if mapped.is_empty() {
        return SurfaceColorFormat::default();
    }

    if prefer_hdr {
        // Prefer HDR formats in order of quality.
        for candidate in &[
            SurfaceColorFormat::Rgba16Float,
            SurfaceColorFormat::Rgb10a2Unorm,
        ] {
            if mapped.contains(candidate) {
                return *candidate;
            }
        }
    }

    // Prefer sRGB variants for correct SDR gamma.
    for candidate in &[
        SurfaceColorFormat::Bgra8UnormSrgb,
        SurfaceColorFormat::Rgba8UnormSrgb,
        SurfaceColorFormat::Bgra8Unorm,
        SurfaceColorFormat::Rgba8Unorm,
    ] {
        if mapped.contains(candidate) {
            return *candidate;
        }
    }

    mapped[0]
}

// ── HdrGpuContext ─────────────────────────────────────────────────────────────

/// A headless GPU context backed by an `Rgba16Float` offscreen texture.
///
/// Unlike [`crate::GpuContext`] (which uses `Rgba8Unorm` for byte-exact
/// readback), `HdrGpuContext` uses `Rgba16Float` so fragment shaders can
/// output linear-light values in `[0, +∞)`.  Readback returns raw `f16`
/// bytes; use a half-to-float conversion to get the actual float values.
///
/// HDR capability requires the adapter to support `Rgba16Float` as a render
/// attachment.  The `from_device` constructor validates this and returns
/// [`UiError::Unsupported`] if the format is not supported.
pub struct HdrGpuContext {
    /// The logical GPU device.
    pub device: wgpu::Device,
    /// The command queue.
    pub queue: wgpu::Queue,
    /// The `Rgba16Float` offscreen texture.
    pub color_texture: wgpu::Texture,
    /// View over `color_texture`.
    pub color_view: wgpu::TextureView,
    /// Target width in physical pixels.
    pub width: u32,
    /// Target height in physical pixels.
    pub height: u32,
}

/// The HDR offscreen format.
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl HdrGpuContext {
    /// Create a headless HDR context.
    ///
    /// # Errors
    ///
    /// - [`UiError::Unsupported`] if no GPU adapter is available, if the
    ///   dimensions are zero, or if the adapter does not support `Rgba16Float`
    ///   as a render attachment.
    /// - [`UiError::Backend`] if device creation fails.
    pub fn headless(width: u32, height: u32) -> Result<Self, UiError> {
        if width == 0 || height == 0 {
            return Err(UiError::Unsupported(
                "HdrGpuContext dimensions must be non-zero".to_string(),
            ));
        }

        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| UiError::Unsupported(format!("no GPU adapter: {e}")))?;

        // Verify Rgba16Float RENDER_ATTACHMENT support.
        let fmt_features = adapter.get_texture_format_features(HDR_FORMAT);
        if !fmt_features
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(UiError::Unsupported(
                "adapter does not support Rgba16Float as a render attachment".to_string(),
            ));
        }

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("oxiui-render-wgpu hdr device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| UiError::Backend(format!("HDR GPU device request failed: {e}")))?;

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxiui-render-wgpu hdr target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
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

    /// Read back the HDR texture as raw bytes.
    ///
    /// The returned buffer contains `width * height * 8` bytes (4 channels ×
    /// 2 bytes per channel, f16 little-endian), tightly packed (row padding
    /// stripped).
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] if the GPU poll or buffer mapping fails.
    pub fn readback_f16(&self) -> Result<Vec<u8>, UiError> {
        // f16 = 2 bytes per channel × 4 channels = 8 bytes per pixel.
        let bytes_per_pixel = 8u32;
        let unpadded_bytes_per_row = self.width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * self.height) as wgpu::BufferAddress;

        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxiui-render-wgpu hdr readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxiui-render-wgpu hdr readback encoder"),
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

        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| UiError::Render(format!("HdrGpuContext GPU poll failed: {e:?}")))?;

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
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_color_format_is_hdr() {
        assert!(SurfaceColorFormat::Rgba16Float.is_hdr());
        assert!(!SurfaceColorFormat::Rgba8Unorm.is_hdr());
        assert!(!SurfaceColorFormat::Bgra8UnormSrgb.is_hdr());
    }

    #[test]
    fn surface_color_format_bits_per_channel() {
        assert_eq!(SurfaceColorFormat::Rgba8Unorm.bits_per_channel(), 8);
        assert_eq!(SurfaceColorFormat::Rgb10a2Unorm.bits_per_channel(), 10);
        assert_eq!(SurfaceColorFormat::Rgba16Float.bits_per_channel(), 16);
    }

    #[test]
    fn surface_color_format_expects_linear() {
        assert!(SurfaceColorFormat::Rgba16Float.expects_linear_light());
        assert!(!SurfaceColorFormat::Rgba8Unorm.expects_linear_light());
        assert!(!SurfaceColorFormat::Rgba8UnormSrgb.expects_linear_light());
    }

    #[test]
    fn select_surface_format_prefers_hdr_when_available() {
        let fmts = &[
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba16Float,
        ];
        let chosen = select_surface_format(fmts, true);
        assert_eq!(chosen, SurfaceColorFormat::Rgba16Float);
    }

    #[test]
    fn select_surface_format_falls_back_to_srgb_when_no_hdr() {
        let fmts = &[
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ];
        let chosen = select_surface_format(fmts, true); // HDR not available
                                                        // Should pick sRGB variant.
        assert_eq!(chosen, SurfaceColorFormat::Bgra8UnormSrgb);
    }

    #[test]
    fn select_surface_format_prefers_srgb_without_hdr_preference() {
        let fmts = &[
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba16Float,
        ];
        let chosen = select_surface_format(fmts, false);
        // Without HDR preference, picks sRGB first.
        assert_eq!(chosen, SurfaceColorFormat::Bgra8UnormSrgb);
    }

    #[test]
    fn select_surface_format_empty_list_returns_default() {
        let chosen = select_surface_format(&[], false);
        assert_eq!(chosen, SurfaceColorFormat::default());
    }

    #[test]
    fn hdr_gpu_context_creates_or_skips() {
        // Either creates successfully or returns Unsupported (no GPU / no f16 support).
        match HdrGpuContext::headless(32, 32) {
            Ok(ctx) => {
                assert_eq!(ctx.width, 32);
                assert_eq!(ctx.height, 32);
            }
            Err(e @ oxiui_core::UiError::Unsupported(_)) => {
                println!("skip: HDR not supported: {e}");
            }
            Err(e) => {
                panic!("unexpected error creating HdrGpuContext: {e}");
            }
        }
    }

    #[test]
    fn hdr_format_is_rgba16float() {
        assert_eq!(HDR_FORMAT, wgpu::TextureFormat::Rgba16Float);
    }
}
