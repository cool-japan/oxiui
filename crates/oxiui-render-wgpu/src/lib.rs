#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! wgpu GPU render surface — CPU-side preparation engine for OxiUI.
//!
//! This crate provides the full CPU-side rendering preparation stack that a
//! future GPU [`RenderBackend`] implementation will compose:
//!
//! - [`atlas`] — Dynamic shelf-based texture atlas with LRU eviction.
//! - [`batch`] — Draw-call batcher: sorts [`DrawList`] commands by pipeline
//!   state, merges adjacent same-key runs, and culls off-screen commands.
//! - [`clip`] — Nested clip-rect stack with outward-rounded integer scissor.
//! - [`quality`] — [`RenderQuality`] presets (low / balanced / high).
//! - [`resource`] — Generation-checked [`TextureHandle`]/[`ShaderHandle`]
//!   newtypes with a reference-counted [`ResourceRegistry`] and RAII guards.
//! - [`error`] — [`GpuErrorKind`] → [`UiError`] mapping.
//! - [`gpu`] — the real headless GPU backend ([`WgpuBackend`]) built on
//!   [`wgpu`]: offscreen device init, the `solid.wgsl` pipeline, and a
//!   [`RenderBackend`] implementation for solid rectangles, SDF circles, and
//!   scissor-based clipping, with CPU pixel readback.
//!
//! GPU drivers (Vulkan/Metal/DX12/WebGPU) are OS-provided at runtime;
//! they are NOT linked at build time.  [`gpu::WgpuBackend::headless`] acquires
//! an adapter at runtime and gracefully reports [`UiError::Unsupported`] when
//! none is available.
//!
//! [`RenderBackend`]: oxiui_core::paint::RenderBackend
//! [`DrawList`]: oxiui_core::paint::DrawList
//! [`UiError`]: oxiui_core::UiError
//! [`UiError::Unsupported`]: oxiui_core::UiError::Unsupported

pub mod atlas;
pub mod batch;
pub mod clip;
pub mod error;
pub mod gpu;
pub mod quality;
pub mod resource;

// ── Convenience re-exports ────────────────────────────────────────────────────

pub use atlas::{AtlasHandle, AtlasRect, TextureAtlas};
pub use batch::{BatchKey, BlendMode, DrawBatch, PipelineKind, PreparedFrame};
pub use clip::{ClipRect, ClipStack};
pub use error::{map_gpu_error, GpuErrorKind};
pub use gpu::{GpuContext, SolidPipeline, WgpuBackend};
pub use quality::{RenderQuality, ShadowQuality, TextQuality};
pub use resource::{
    ResourceId, ResourceRegistry, ShaderGuard, ShaderHandle, TextureGuard, TextureHandle,
};

// ── WgpuPrep ─────────────────────────────────────────────────────────────────

/// The CPU-side preparation state for the wgpu render pipeline.
///
/// `WgpuPrep` owns a [`TextureAtlas`], a [`ClipStack`], and a
/// [`RenderQuality`] configuration.  Call [`prepare`] once per frame to batch
/// a [`oxiui_core::paint::DrawList`] into a [`PreparedFrame`] that a GPU consumer can execute.
///
/// [`prepare`]: WgpuPrep::prepare
pub struct WgpuPrep {
    /// The texture atlas for this render target.
    pub atlas: atlas::TextureAtlas,
    /// The active clip-rect stack.
    pub clip: clip::ClipStack,
    /// Current render-quality configuration.
    pub quality: quality::RenderQuality,
}

impl WgpuPrep {
    /// Construct a new [`WgpuPrep`] with the given atlas size and quality preset.
    pub fn new(atlas_size: u32, quality: quality::RenderQuality) -> Self {
        Self {
            atlas: atlas::TextureAtlas::new(atlas_size, atlas_size),
            clip: clip::ClipStack::new(),
            quality,
        }
    }

    /// Batch `list` into a [`PreparedFrame`] using the current clip state.
    ///
    /// The active clip rect (top of [`clip`]) is forwarded to the batcher as
    /// the visibility-culling region.
    ///
    /// [`clip`]: WgpuPrep::clip
    pub fn prepare(&mut self, list: &oxiui_core::paint::DrawList) -> batch::PreparedFrame {
        let active_clip = self.clip.current().map(|c| [c.x, c.y, c.w, c.h]);
        batch::batch(list, active_clip)
    }
}

// ── Legacy stub (kept for binary compatibility) ───────────────────────────────

/// Placeholder GPU renderer kept for backward compatibility.
///
/// This was the original M1 stub.  New code should use [`WgpuPrep`] instead.
/// The struct will be removed once all consumers are migrated.
pub struct WgpuRenderer {
    _marker: std::marker::PhantomData<()>,
}

impl WgpuRenderer {
    /// Construct a new [`WgpuRenderer`] stub.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for WgpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Integration tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::{geometry::Rect, paint::DrawList, Color};

    fn red() -> Color {
        Color(255, 0, 0, 255)
    }

    #[test]
    fn prepare_empty_drawlist_is_noop() {
        let mut prep = WgpuPrep::new(512, RenderQuality::low());
        let list = DrawList::new();
        let frame = prep.prepare(&list);
        assert_eq!(
            frame.batches.len(),
            0,
            "empty list must produce zero batches"
        );
        assert_eq!(
            frame.culled_count, 0,
            "empty list must have zero culled commands"
        );
    }

    #[test]
    fn prepare_integrates_atlas_and_clip() {
        let mut prep = WgpuPrep::new(512, RenderQuality::balanced());
        // Push a clip rect and add a rect that falls inside it.
        prep.clip.push(ClipRect::new(0.0, 0.0, 100.0, 100.0));
        let mut list = DrawList::new();
        list.push_rect(Rect::new(10.0, 10.0, 20.0, 20.0), red());
        let frame = prep.prepare(&list);
        // One SolidColor batch, no culling.
        assert_eq!(frame.batches.len(), 1);
        assert_eq!(frame.culled_count, 0);
    }
}
