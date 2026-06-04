//! Real (non-stub) headless GPU rendering for OxiUI, built on [`wgpu`].
//!
//! This module implements [`oxiui_core::paint::RenderBackend`] on a GPU target
//! that is created *headlessly* — there is no window or swap-chain surface.
//! Draw commands are rasterised into an offscreen colour texture which can be
//! read back to CPU memory for testing or off-screen export.
//!
//! Sub-modules:
//!
//! - [`device`]        — headless `Instance`/adapter/device/queue + offscreen texture.
//! - [`pipeline`]      — `solid.wgsl`, `gradient.wgsl`, `textured.wgsl`, `blur.wgsl`,
//!   and `composite.wgsl` compiled pipelines.
//! - [`buffer`]        — `#[repr(C)]` `Pod` vertex / uniform layouts + quad emitters.
//! - [`tessellator`]   — CPU path flattening and stroke/fill tessellation.
//! - [`texture`]       — image upload utilities and `TexturedDraw`.
//! - [`shadow`]        — ping-pong Gaussian blur + composite for `BoxShadow`.
//! - [`geometry`]      — CPU geometry builder, visibility culling helpers, and
//!   draw-segment / gradient-draw data structures.
//! - [`exec`]          — GPU pass execution helpers and [`FrameStats`].
//! - [`renderer`]      — [`WgpuBackend`] and the `RenderBackend` implementation.
//! - [`instance`]      — instanced rectangle rendering pipeline and renderer.
//! - [`render_target`] — off-screen render targets for cached subtrees.
//! - [`layer_cache`]   — GPU-backed render-layer cache with dirty tracking.
//! - [`ring_buffer`]   — streaming vertex/index ring buffer.

pub mod blend;
pub mod buffer;
pub mod compute_blur;
pub mod device;
pub mod earcut;
pub mod exec;
pub mod frame_pacing;
pub mod geometry;
pub mod hdr;
pub mod instance;
pub mod layer_cache;
pub mod pipeline;
pub mod render_target;
pub mod renderer;
pub mod ring_buffer;
pub mod shadow;
pub mod stencil;
pub mod tessellator;
pub mod texture;

pub use blend::{blend_state_for_mode, BlendPipelineSet};
pub use buffer::{Globals, GradientVertex, TexVertex, Vertex};
pub use compute_blur::ComputeBlurPipeline;
pub use device::{GpuContext, TARGET_FORMAT};
pub use exec::FrameStats;
pub use frame_pacing::{FrameHistogram, FrameTimer, FrameTimerMode, PresentModeRecommendation};
pub use hdr::{select_surface_format, HdrGpuContext, SurfaceColorFormat, HDR_FORMAT};
pub use instance::{InstanceRect, InstancedRectPipeline, InstancedRectRenderer};
pub use layer_cache::LayerCache;
pub use pipeline::{
    BlurPipeline, CompositePipeline, GradientPipeline, SolidPipeline, TexturedPipeline,
};
pub use render_target::RenderTarget;
pub use renderer::WgpuBackend;
pub use ring_buffer::{RingAllocation, RingBuffer, RingBufferStats};
pub use stencil::{StencilClipState, StencilTarget, StencilWritePipeline, DEPTH_STENCIL_FORMAT};
