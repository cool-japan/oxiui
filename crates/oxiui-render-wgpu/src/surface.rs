//! Windowed wgpu surface — swap-chain context for real-time rendering.
//!
//! [`SurfaceContext`] wraps a `wgpu::Surface` created from a raw OS window
//! handle alongside the `Device`, `Queue`, and swap-chain configuration
//! needed for real-time frame presentation.
//!
//! # Safety
//!
//! Creating a [`SurfaceContext`] requires raw window/display handles whose
//! lifetime the caller must manage.  See [`SurfaceContext::from_raw_handles`].

use oxiui_core::UiError;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

// ── SurfaceConfig ─────────────────────────────────────────────────────────────

/// Configuration for a [`SurfaceContext`] swap-chain.
#[derive(Clone, Debug)]
pub struct SurfaceConfig {
    /// Width of the swap-chain in physical pixels.
    pub width: u32,
    /// Height of the swap-chain in physical pixels.
    pub height: u32,
    /// Controls how the surface is presented to the display.
    pub present_mode: wgpu::PresentMode,
    /// Compositing alpha mode for the surface.
    pub alpha_mode: wgpu::CompositeAlphaMode,
    /// Preferred texture format.  When `None` the first format reported by
    /// `surface.get_capabilities(&adapter)` is used (preferring sRGB).
    pub desired_format: Option<wgpu::TextureFormat>,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            desired_format: None,
        }
    }
}

// ── SurfaceContext ────────────────────────────────────────────────────────────

/// A windowed GPU context: device, queue, surface, and swap-chain config.
///
/// Unlike [`crate::GpuContext`] (headless), `SurfaceContext` presents frames
/// to an OS window via a wgpu swap-chain.
pub struct SurfaceContext {
    /// The logical GPU device.
    pub device: wgpu::Device,
    /// The command queue for the device.
    pub queue: wgpu::Queue,
    /// The wgpu surface attached to the OS window.
    pub surface: wgpu::Surface<'static>,
    /// The texture format selected for the swap-chain.
    pub surface_format: wgpu::TextureFormat,
    /// Current swap-chain configuration.
    config: wgpu::SurfaceConfiguration,
}

impl SurfaceContext {
    /// Create a windowed surface context from raw OS window/display handles.
    ///
    /// # Safety
    ///
    /// The caller must ensure that both `window_handle` and `display_handle`
    /// refer to valid OS objects and **remain valid** for the entire lifetime
    /// of the returned `SurfaceContext`.  Dropping the underlying window while
    /// a `SurfaceContext` is live is undefined behaviour.
    pub unsafe fn from_raw_handles(
        window_handle: RawWindowHandle,
        display_handle: RawDisplayHandle,
        config: SurfaceConfig,
    ) -> Result<Self, UiError> {
        let instance = wgpu::Instance::default();

        // Build the unsafe surface target from the raw handles.
        let surface_target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_window_handle: window_handle,
            raw_display_handle: Some(display_handle),
        };

        // SAFETY: caller guarantees that the raw handles remain valid.
        let surface = unsafe { instance.create_surface_unsafe(surface_target) }
            .map_err(|e| UiError::Backend(format!("wgpu surface creation failed: {e}")))?;

        // Request an adapter that is compatible with the surface.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .map_err(|e| UiError::Unsupported(format!("no GPU adapter for surface: {e}")))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("oxiui-render-wgpu windowed device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| UiError::Backend(format!("GPU device request failed: {e}")))?;

        // Choose the swap-chain format: honour `desired_format` when set,
        // otherwise pick the first supported format preferring sRGB variants.
        let surface_format = if let Some(fmt) = config.desired_format {
            fmt
        } else {
            let caps = surface.get_capabilities(&adapter);
            caps.formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or_else(|| {
                    *caps
                        .formats
                        .first()
                        .unwrap_or(&wgpu::TextureFormat::Bgra8Unorm)
                })
        };

        let sc_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: config.width.max(1),
            height: config.height.max(1),
            present_mode: config.present_mode,
            alpha_mode: config.alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &sc_config);

        Ok(Self {
            device,
            queue,
            surface,
            surface_format,
            config: sc_config,
        })
    }

    /// Resize the swap-chain to new dimensions.
    ///
    /// Dimensions are clamped to a minimum of 1×1 to avoid a wgpu validation error.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Acquire the next swap-chain frame.
    ///
    /// Returns `None` on surface timeout, occlusion, or any other transient
    /// condition — the caller should skip the frame and try again next tick.
    /// Permanent errors (surface lost / outdated) are logged to `stderr`; the
    /// caller can recreate the `SurfaceContext` if needed.
    pub fn acquire_frame(&self) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                // Suboptimal but still usable; caller should reconfigure soon.
                Some(frame)
            }
            wgpu::CurrentSurfaceTexture::Timeout => None,
            wgpu::CurrentSurfaceTexture::Occluded => None,
            other => {
                eprintln!("[oxiui-render-wgpu] surface error: {other:?} — frame skipped");
                None
            }
        }
    }

    /// Present a previously acquired frame to the display.
    pub fn present_frame(&self, frame: wgpu::SurfaceTexture) {
        frame.present();
    }

    /// Current swap-chain width in physical pixels.
    pub fn width(&self) -> u32 {
        self.config.width
    }

    /// Current swap-chain height in physical pixels.
    pub fn height(&self) -> u32 {
        self.config.height
    }

    /// The texture format used by the swap-chain.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_config_default_is_sane() {
        let c = SurfaceConfig::default();
        assert!(c.width > 0 && c.height > 0);
        assert_eq!(c.width, 800);
        assert_eq!(c.height, 600);
    }

    #[test]
    fn surface_config_clone_and_debug() {
        let c = SurfaceConfig::default();
        let c2 = c.clone();
        assert_eq!(c2.width, c.width);
        // Ensure Debug impl compiles.
        let _ = format!("{c:?}");
    }
}
