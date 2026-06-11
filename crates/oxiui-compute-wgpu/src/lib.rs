//! # `oxiui-compute-wgpu`
//!
//! Pure-Rust wgpu GPU-compute abstraction for the COOLJAPAN ecosystem.
//!
//! This crate consolidates the repeated `Instance → Adapter → Device → Queue`
//! initialisation boilerplate that `oxiaero-cfd`, `oxiaero-mcdc`, and similar
//! crates each duplicated for pure GPU compute workloads (sparse linear
//! solvers, Lattice-Boltzmann, Monte-Carlo simulations, …).
//!
//! ## Quick start
//!
//! ```rust
//! use oxiui_compute_wgpu::{bytemuck, compute_pipeline, read_back, storage_buffer_init, wgpu, ComputeContext};
//!
//! let Some(ctx) = ComputeContext::try_new() else {
//!     return; // no GPU — skip gracefully
//! };
//!
//! let input: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
//! let buffer = storage_buffer_init(&ctx.device, "values", bytemuck::cast_slice(&input));
//!
//! const SHADER: &str = r#"
//!     @group(0) @binding(0) var<storage, read_write> data: array<f32>;
//!     @compute @workgroup_size(64)
//!     fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
//!         if gid.x < arrayLength(&data) {
//!             data[gid.x] = data[gid.x] * 2.0;
//!         }
//!     }
//! "#;
//! let pipeline = compute_pipeline(&ctx.device, SHADER, "main");
//!
//! let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
//!     label: Some("values-bind"),
//!     layout: &pipeline.get_bind_group_layout(0),
//!     entries: &[wgpu::BindGroupEntry {
//!         binding: 0,
//!         resource: buffer.as_entire_binding(),
//!     }],
//! });
//!
//! let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
//! {
//!     let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
//!         label: None,
//!         timestamp_writes: None,
//!     });
//!     pass.set_pipeline(&pipeline);
//!     pass.set_bind_group(0, &bind_group, &[]);
//!     pass.dispatch_workgroups((input.len() as u32 + 63) / 64, 1, 1);
//! }
//! ctx.queue.submit(std::iter::once(encoder.finish()));
//!
//! let output: Vec<f32> = read_back(&ctx.device, &ctx.queue, &buffer, input.len());
//! assert_eq!(output, vec![2.0, 4.0, 6.0, 8.0]);
//! ```
//!
//! ## Module structure
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`context`] | [`ComputeContext`] — headless `Device` + `Queue` init, multi-queue, `from_device` |
//! | [`buffer`]  | Storage / uniform / staging buffer helpers + [`read_back`] |
//! | [`pipeline`] | [`compute_pipeline`] builder |
//! | [`error`]   | [`ComputeError`] type |
//! | [`wgsl`]    | WGSL preprocessor, validation, and built-in compute kernels |
//! | [`dispatch`] | [`Dispatcher`] — high-level GPU compute helpers |
//! | [`integration`] | Bridges to `oxiui-render-soft`, `oxiui-render-wgpu`, `oxiui-text` |
//! | `hot_reload` | WGSL hot-reload via `notify` (behind the `hot-reload` feature) |
//!
//! ## Dependency re-exports
//!
//! `oxiui-compute-wgpu` re-exports [`wgpu`], [`bytemuck`], and [`pollster`] so
//! that consumers need only a single dependency declaration in their
//! `Cargo.toml`.
//!
//! ## Feature flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `tracing` | Adds `#[tracing::instrument]` spans to key functions for span-based profiling. |
//! | `hot-reload` | Adds `hot_reload::ShaderWatcher` for live WGSL file watching via `notify`. |

pub mod buffer;
pub mod context;
pub mod dispatch;
pub mod error;
pub mod integration;
pub mod pipeline;
pub mod wgsl;

/// Live WGSL hot-reload via the `notify` file watcher.
///
/// Enable with `features = ["hot-reload"]` in `Cargo.toml`.
#[cfg(feature = "hot-reload")]
pub mod hot_reload;

// ── Flat re-exports ────────────────────────────────────────────────────────────

// From buffer
pub use buffer::{
    mapped_storage_init, read_back, read_back_async, read_back_range, staging_buffer,
    storage_buffer_init, uniform_buffer, BufferPool, SubAllocator, SubRegion, TypedBuffer,
};

// From context
pub use context::{ComputeContext, ContextBuilder};

// From dispatch
pub use dispatch::Dispatcher;

// From error
pub use error::ComputeError;

// From pipeline
pub use pipeline::{
    checked_compute_pipeline, compute_pipeline, dispatch_1d, dispatch_2d, dispatch_3d,
    encode_indirect_dispatch, supports_immediates, validate_immediates, DispatchBuilder,
    DispatchResult, PipelineCache, MAX_WORKGROUPS,
};

// From wgsl
pub use wgsl::{
    preprocess, validate, WgslDiagnostic, WgslError, SHADER_BITONIC_SORT, SHADER_HISTOGRAM,
    SHADER_MAP_F32_TEMPLATE, SHADER_MATMUL, SHADER_PREFIX_SUM, SHADER_REDUCTION_SUM,
    SHADER_SPH_DENSITY, SHADER_ZIP_MAP_F32_TEMPLATE,
};

// ── Underlying crate re-exports ───────────────────────────────────────────────

/// Re-export of [`wgpu`] so consumers need only declare `oxiui-compute-wgpu`.
pub use wgpu;

/// Re-export of [`bytemuck`] for `Pod`/`Zeroable` derives and casting helpers.
pub use bytemuck;

/// Re-export of [`pollster`] for blocking on async wgpu operations.
pub use pollster;

// ── Top-level tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_context_try_new_does_not_panic() {
        // Gracefully skips if no GPU adapter — must never panic.
        let _ = ComputeContext::try_new();
    }

    #[test]
    fn compute_context_new_returns_result() {
        match ComputeContext::new() {
            Ok(_ctx) => { /* GPU available — context created successfully */ }
            Err(ComputeError::NoAdapter) => {
                // No GPU on this host (CI, headless VM) — acceptable skip.
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
