//! Runtime CPU/GPU backend switching via the shared [`oxiui_core::paint::DrawList`] format.
//!
//! Both [`SoftBackend`] and the GPU path (`oxiui-render-wgpu::WgpuBackend`) consume
//! the same [`oxiui_core::paint::DrawList`] from [`oxiui_core`].  This module provides a thin
//! [`DynBackend`] enum that wraps either backend so application code can choose
//! the rendering path at runtime — for example, falling back to the CPU path in
//! CI / headless environments where a GPU adapter is unavailable.
//!
//! # Feature gate
//!
//! This module is only compiled when the `wgpu-compat` feature is enabled.
//! Without the feature, no runtime switching overhead is incurred.
//!
//! # Example
//!
//! ```ignore
//! use oxiui_render_soft::backend_switch::{DynBackend, BackendKind};
//!
//! // Prefer GPU, fall back to CPU.
//! let backend = DynBackend::prefer_gpu(1920, 1080)
//!     .unwrap_or_else(|| DynBackend::soft(1920, 1080));
//!
//! let kind = backend.kind();
//! println!("active backend: {kind:?}");
//! ```

use oxiui_core::{
    geometry::Size,
    paint::{DrawList, RenderBackend},
    UiError,
};

use crate::SoftBackend;

#[cfg(feature = "wgpu-compat")]
use oxiui_render_wgpu::WgpuBackend;

// ---------------------------------------------------------------------------
// BackendKind discriminant
// ---------------------------------------------------------------------------

/// Identifies which backend variant is active inside a [`DynBackend`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    /// CPU framebuffer — headless / ffi-audit / no-GPU path.
    Soft,
    /// GPU path via wgpu (Vulkan / Metal / DX12 / WebGPU).
    #[cfg(feature = "wgpu-compat")]
    Wgpu,
}

// ---------------------------------------------------------------------------
// DynBackend
// ---------------------------------------------------------------------------

/// A runtime-polymorphic render backend that implements [`RenderBackend`].
///
/// Wraps either [`SoftBackend`] or [`WgpuBackend`] and forwards all
/// [`RenderBackend`] method calls to the active variant.  The CPU and GPU
/// paths share the same [`DrawList`] command format (defined in
/// [`oxiui_core`]), so callers do not need to change anything when the active
/// backend changes.
///
/// Construct via [`DynBackend::soft`] or — when the `wgpu-compat` feature is
/// active — via [`DynBackend::wgpu`].
pub enum DynBackend {
    /// CPU software rasteriser.
    Soft(SoftBackend),
    /// wgpu GPU rasteriser.
    #[cfg(feature = "wgpu-compat")]
    Wgpu(WgpuBackend),
}

impl DynBackend {
    /// Wrap a [`SoftBackend`] as the CPU path.
    pub fn soft(width: u32, height: u32) -> Self {
        DynBackend::Soft(SoftBackend::new(width, height))
    }

    /// Wrap a [`WgpuBackend`] as the GPU path.
    ///
    /// Only available when the `wgpu-compat` feature is enabled.
    #[cfg(feature = "wgpu-compat")]
    pub fn wgpu(backend: WgpuBackend) -> Self {
        DynBackend::Wgpu(backend)
    }

    /// Return the active [`BackendKind`].
    pub fn kind(&self) -> BackendKind {
        match self {
            DynBackend::Soft(_) => BackendKind::Soft,
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(_) => BackendKind::Wgpu,
        }
    }

    /// Return `true` if the active backend is the CPU soft rasteriser.
    pub fn is_soft(&self) -> bool {
        matches!(self, DynBackend::Soft(_))
    }

    /// Return `true` if the active backend is the wgpu GPU rasteriser.
    #[cfg(feature = "wgpu-compat")]
    pub fn is_wgpu(&self) -> bool {
        matches!(self, DynBackend::Wgpu(_))
    }

    /// Borrow the inner [`SoftBackend`], or `None` if the GPU path is active.
    pub fn as_soft(&self) -> Option<&SoftBackend> {
        match self {
            DynBackend::Soft(b) => Some(b),
            #[cfg(feature = "wgpu-compat")]
            _ => None,
        }
    }

    /// Mutably borrow the inner [`SoftBackend`], or `None` if the GPU path is active.
    pub fn as_soft_mut(&mut self) -> Option<&mut SoftBackend> {
        match self {
            DynBackend::Soft(b) => Some(b),
            #[cfg(feature = "wgpu-compat")]
            _ => None,
        }
    }
}

impl RenderBackend for DynBackend {
    fn execute(&mut self, list: &DrawList) -> Result<(), UiError> {
        match self {
            DynBackend::Soft(b) => b.execute(list),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.execute(list),
        }
    }

    fn surface_size(&self) -> Size {
        match self {
            DynBackend::Soft(b) => b.surface_size(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.surface_size(),
        }
    }

    fn supports_blur(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_blur(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_blur(),
        }
    }

    fn supports_gradients(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_gradients(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_gradients(),
        }
    }

    fn supports_paths(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_paths(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_paths(),
        }
    }

    fn supports_images(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_images(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_images(),
        }
    }

    fn supports_text(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_text(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_text(),
        }
    }

    fn supports_blend_modes(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_blend_modes(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_blend_modes(),
        }
    }

    fn supports_backdrop_blur(&self) -> bool {
        match self {
            DynBackend::Soft(b) => b.supports_backdrop_blur(),
            #[cfg(feature = "wgpu-compat")]
            DynBackend::Wgpu(b) => b.supports_backdrop_blur(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dyn_backend_soft_kind() {
        let b = DynBackend::soft(64, 64);
        assert_eq!(b.kind(), BackendKind::Soft);
        assert!(b.is_soft());
        assert_eq!(b.surface_size(), Size::new(64.0, 64.0));
    }

    #[test]
    fn dyn_backend_soft_supports_capabilities() {
        let b = DynBackend::soft(32, 32);
        // SoftBackend always reports true for these.
        assert!(b.supports_blur());
        assert!(b.supports_gradients());
        assert!(b.supports_paths());
        assert!(b.supports_images());
    }

    #[test]
    fn dyn_backend_soft_execute_fill_rect() {
        use oxiui_core::geometry::{Rect, Size};
        use oxiui_core::paint::DrawList;
        use oxiui_core::Color;

        let mut b = DynBackend::soft(100, 100);
        let mut list = DrawList::new();
        list.push_rect(Rect::new(10.0, 10.0, 20.0, 20.0), Color(255, 0, 0, 255));
        let result = b.execute(&list);
        assert!(result.is_ok(), "execute must not fail: {result:?}");
        // Verify that the center pixel is red.
        let fb_pixel = b.as_soft().and_then(|s| s.frame().get_rgba(20, 20));
        if let Some((r, _, _, _)) = fb_pixel {
            assert!(r > 200, "center pixel should be red: r={r}");
        }
        // Suppress unused import warning.
        let _ = Size::new(0.0, 0.0);
    }

    #[test]
    fn dyn_backend_as_soft_mut() {
        let mut b = DynBackend::soft(10, 10);
        let soft = b.as_soft_mut();
        assert!(soft.is_some());
    }
}
