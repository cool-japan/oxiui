//! Real (non-stub) headless GPU rendering for OxiUI, built on [`wgpu`].
//!
//! This module implements [`oxiui_core::paint::RenderBackend`] on a GPU target
//! that is created *headlessly* — there is no window or swap-chain surface.
//! Draw commands are rasterised into an offscreen colour texture which can be
//! read back to CPU memory for testing or off-screen export.
//!
//! Sub-modules:
//!
//! - [`device`]       — headless `Instance`/adapter/device/queue + offscreen texture.
//! - [`pipeline`]     — `solid.wgsl` and `gradient.wgsl` compiled pipelines.
//! - [`buffer`]       — `#[repr(C)]` `Pod` vertex / uniform layouts + quad emitters.
//! - [`tessellator`]  — CPU path flattening and stroke/fill tessellation.
//! - [`renderer`]     — [`WgpuBackend`] and the `RenderBackend` implementation.

pub mod buffer;
pub mod device;
pub mod pipeline;
pub mod renderer;
pub mod tessellator;

pub use buffer::{Globals, GradientVertex, Vertex};
pub use device::{GpuContext, TARGET_FORMAT};
pub use pipeline::{GradientPipeline, SolidPipeline};
pub use renderer::WgpuBackend;
