#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! softbuffer CPU framebuffer backend — headless / ffi-audit / embedded path.
//!
//! Use `--no-default-features --features software` for the GPU-free audit build.
//!
//! # Headless rendering
//!
//! The [`headless`] module provides [`headless::render_headless_once`] and
//! [`headless::RgbaBuffer`] for CI/ffi-audit usage without any display.

use oxiui_core::{Color, UiError};

/// Headless rendering helpers: [`RgbaBuffer`], [`render_headless_once`], and PNG save.
pub mod headless;
pub use headless::{
    render_headless_once, render_headless_scene, PixelFormat, RgbaBuffer, HEADLESS_BG_COLOR,
};

/// CPU pixel framebuffer (`0xAARRGGBB`) with blending and RGBA export.
pub mod framebuffer;
pub use framebuffer::Framebuffer;

/// Rectangular clip-region stack.
pub mod clip;
pub use clip::{ClipRect, ClipStack};

/// Drawing primitives: rectangles, lines, circles, rounded rects, blits.
pub mod draw;
pub use draw::{Canvas, SrcImage};

/// Linear and radial gradient fills with sRGB colour interpolation.
pub mod gradient;
pub use gradient::{lerp_color, GradientStop, LinearGradient, RadialGradient};

/// Extended blend modes (multiply / screen / overlay / darken / lighten) and
/// premultiplied-alpha helpers — additive over `Framebuffer::blend`.
pub mod blend;
pub use blend::{blend_mode, blend_pixel, composite_into, BlendMode, RgbaUnit};

/// Active-Edge-Table scanline polygon / triangle fill with vertical-
/// supersample coverage AA. Supports even-odd and non-zero winding.
pub mod scanline;
pub use scanline::{fill_polygon, fill_triangle, FillRule};

/// 2D paths with Bezier flattening, fill (via [`scanline`]), and stroke.
pub mod path;
pub use path::{Cap, Join, Path, PathBuilder, StrokeStyle};

/// Box shadow via separable 1-D Gaussian blur with a kernel cache.
pub mod shadow;
pub use shadow::{box_shadow, gaussian_blur_alpha, GaussianCache};

/// Bayer-matrix ordered dithering for reduced-bit output paths.
pub mod dither;
pub use dither::{ordered_dither_rgba, BayerMatrix};

/// 64×64 render-tile iterator (rayon-ready; serial driver only this run).
pub mod tile;
#[cfg(feature = "parallel")]
pub use tile::render_parallel;
pub use tile::{
    collect_tiles, render_tiles, tiles_for, DirtyRegion, Tile, TileIter, DEFAULT_TILE_SIZE,
};

/// CPU framebuffer backend implementing [`oxiui_core::paint::RenderBackend`].
pub mod backend;
pub use backend::{blit_glyph_bitmap, SoftBackend};

/// Re-export [`oxiui_theme::ShadowSpec`] when the `theme` feature is active so
/// integration tests can address it as `oxiui_render_soft::ShadowSpec`.
#[cfg(feature = "theme")]
pub use oxiui_theme::ShadowSpec;

/// Errors specific to the soft-render backend.
#[derive(Debug)]
pub enum SoftRenderError {
    /// An I/O error (e.g. cannot create or write the output file).
    Io(String),
    /// A PNG encoding / decoding error.
    Png(String),
}

impl std::fmt::Display for SoftRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SoftRenderError::Io(s) => write!(f, "soft-render I/O error: {s}"),
            SoftRenderError::Png(s) => write!(f, "soft-render PNG error: {s}"),
        }
    }
}

impl std::error::Error for SoftRenderError {}

/// CPU-based software renderer using a raw pixel framebuffer.
///
/// In the full implementation this would wrap a `softbuffer::Surface`.
/// For M1 it is a pure-Rust pixel-buffer helper that does not require
/// a display at build time.
pub struct SoftRenderer {
    _marker: std::marker::PhantomData<()>,
}

impl SoftRenderer {
    /// Construct a new [`SoftRenderer`].
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Fill a framebuffer with a solid colour.
    ///
    /// Returns a `Vec<u32>` of length `width * height` in `0xAARRGGBB` format,
    /// every pixel set to `color`.
    ///
    /// # Errors
    /// Currently infallible; returns `Err` only if a future implementation
    /// encounters a resource constraint.
    pub fn clear_frame(&self, width: u32, height: u32, color: Color) -> Result<Vec<u32>, UiError> {
        let fb = Framebuffer::with_fill(width, height, color);
        Ok(fb.pixels().to_vec())
    }

    /// Render a scene into a fresh [`Framebuffer`] of the given size.
    ///
    /// The framebuffer is pre-filled with `background`; `draw_fn` receives a
    /// clipped [`Canvas`] to paint the scene. Returns the finished framebuffer.
    pub fn render<F>(&self, width: u32, height: u32, background: Color, draw_fn: F) -> Framebuffer
    where
        F: FnOnce(&mut Canvas<'_>),
    {
        let mut fb = Framebuffer::with_fill(width, height, background);
        {
            let mut canvas = Canvas::new(&mut fb);
            draw_fn(&mut canvas);
        }
        fb
    }
}

impl Default for SoftRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Quality configuration ────────────────────────────────────────────────────

/// Anti-aliasing mode for the software renderer.
///
/// Controls the AA strategy used by polygon / path fill operations.
/// `Msaa4x` is listed for forward-compatibility but maps to `Supersampling`
/// in the current scanline implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AaMode {
    /// No anti-aliasing — all edges are aliased (fastest).
    None,
    /// 4× multi-sample anti-aliasing (currently maps to the supersample path).
    Msaa4x,
    /// Vertical-supersample coverage AA via the Active Edge Table (default quality).
    Supersampling,
}

/// Shadow render quality.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowQuality {
    /// Shadow rendering disabled — `BoxShadow` commands produce a hard-edge fill.
    Off,
    /// Low-quality shadow: small Gaussian kernel (faster).
    Low,
    /// High-quality shadow: full Gaussian kernel with kernel caching.
    High,
}

/// Combined quality preset for [`SoftBackend::with_quality`].
///
/// Use the [`low`](SoftRenderQuality::low), [`balanced`](SoftRenderQuality::balanced),
/// and [`high`](SoftRenderQuality::high) constructors to get sensible presets,
/// or build a custom config with struct syntax.
#[derive(Clone, Debug, PartialEq)]
pub struct SoftRenderQuality {
    /// Anti-aliasing strategy.
    pub aa_mode: AaMode,
    /// Shadow rendering quality.
    pub shadow_quality: ShadowQuality,
}

impl SoftRenderQuality {
    /// Fastest preset: no AA, no shadows.
    pub fn low() -> Self {
        Self {
            aa_mode: AaMode::None,
            shadow_quality: ShadowQuality::Off,
        }
    }

    /// Balanced preset: supersampling AA, low-quality shadows.
    pub fn balanced() -> Self {
        Self {
            aa_mode: AaMode::Supersampling,
            shadow_quality: ShadowQuality::Low,
        }
    }

    /// Highest-quality preset: supersampling AA, high-quality shadows.
    pub fn high() -> Self {
        Self {
            aa_mode: AaMode::Supersampling,
            shadow_quality: ShadowQuality::High,
        }
    }
}

// ── SoftRenderer additional constructors ────────────────────────────────────

impl SoftRenderer {
    /// Pre-allocate the framebuffer for the given pixel dimensions.
    ///
    /// Equivalent to `SoftRenderer::new()` — the renderer itself is stateless;
    /// the framebuffer is created on demand inside [`SoftRenderer::render`].
    /// Provided for API parity with [`SoftBackend::with_quality`].
    pub fn with_size(_width: u32, _height: u32) -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

// ── SoftBackend additional constructors ─────────────────────────────────────

impl SoftBackend {
    /// Create a backend pre-configured with a [`SoftRenderQuality`] preset.
    ///
    /// The `quality` is stored and used to drive AA mode selection during
    /// subsequent [`execute`](oxiui_core::paint::RenderBackend::execute) calls.
    ///
    /// [`AaMode::Supersampling`] / [`AaMode::Msaa4x`] enables the vertical-
    /// supersample coverage path in the scanline polygon rasterizer.
    /// [`AaMode::None`] disables AA (faster, aliased edges).
    ///
    /// [`ShadowQuality::Off`] skips Gaussian convolution for box-shadow commands.
    /// [`ShadowQuality::Low`] and [`ShadowQuality::High`] use the existing
    /// [`GaussianCache`] with a small or full kernel radius respectively.
    pub fn with_quality(width: u32, height: u32, quality: SoftRenderQuality) -> Self {
        let mut backend = Self::new(width, height);
        backend.set_quality(quality);
        backend
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod lib_tests {
    use super::*;

    /// Test 4: SoftRenderQuality::low().aa_mode == AaMode::None
    #[test]
    fn quality_low_aa_mode_is_none() {
        let q = SoftRenderQuality::low();
        assert_eq!(q.aa_mode, AaMode::None);
        assert_eq!(q.shadow_quality, ShadowQuality::Off);
    }

    /// Test 5: SoftRenderQuality::high().aa_mode == AaMode::Supersampling
    #[test]
    fn quality_high_aa_mode_is_supersampling() {
        let q = SoftRenderQuality::high();
        assert_eq!(q.aa_mode, AaMode::Supersampling);
        assert_eq!(q.shadow_quality, ShadowQuality::High);
    }

    /// balanced preset.
    #[test]
    fn quality_balanced_preset() {
        let q = SoftRenderQuality::balanced();
        assert_eq!(q.aa_mode, AaMode::Supersampling);
        assert_eq!(q.shadow_quality, ShadowQuality::Low);
    }

    /// Test 6: SoftRenderer::with_size(100,100) constructs without panic.
    #[test]
    fn soft_renderer_with_size_constructs() {
        let _r = SoftRenderer::with_size(100, 100);
    }

    /// Test 7: SoftBackend::with_quality(50,50,low()) constructs without panic.
    #[test]
    fn soft_backend_with_quality_low_constructs() {
        let _b = SoftBackend::with_quality(50, 50, SoftRenderQuality::low());
    }

    /// SoftBackend::with_quality(high) constructs without panic.
    #[test]
    fn soft_backend_with_quality_high_constructs() {
        let _b = SoftBackend::with_quality(50, 50, SoftRenderQuality::high());
    }

    /// Quality struct supports Clone and Debug.
    #[test]
    fn quality_clone_debug() {
        let q = SoftRenderQuality::balanced();
        let q2 = q.clone();
        assert_eq!(q, q2);
        let _ = format!("{:?}", q);
    }
}
