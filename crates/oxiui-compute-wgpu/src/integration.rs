//! Integration helpers that connect `oxiui-compute-wgpu` with the rest of the
//! COOLJAPAN UI ecosystem.
//!
//! # Sub-modules
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`render_soft`] | GPU acceleration for CPU render-soft operations (blur, dither, gradient) with automatic CPU fallback when no GPU is available. |
//! | [`render_wgpu`] | Helpers for sharing an externally-owned `wgpu::Device` + `wgpu::Queue` from `oxiui-render-wgpu` with the compute layer. |
//! | [`text`] | GPU glyph-rasterization compute pass that writes coverage into an atlas buffer. |
//!
//! # COOLJAPAN serialization policy
//!
//! All CPU/GPU data interchange in this module uses [`bytemuck`] `Pod`/`Zeroable`
//! types exclusively.  No `bincode`, `serde`, or other serializers are used on
//! the GPU data path — the raw byte representation produced by `bytemuck` is
//! the wire format for all buffer uploads and readbacks.

pub mod render_soft;
pub mod render_wgpu;
pub mod text;
