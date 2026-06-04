//! Compute pipeline builder helpers.
//!
//! [`compute_pipeline`] compiles a WGSL compute shader and wires it into a
//! `wgpu::ComputePipeline` in a single call.  The pipeline uses an auto-layout
//! (wgpu derives the bind-group layout from the shader reflection), which is
//! correct for the majority of workloads.  Callers that need explicit bind-group
//! layouts can use wgpu directly after acquiring a [`crate::ComputeContext`].
//!
//! Additional helpers in this module:
//!
//! * [`dispatch_1d`], [`dispatch_2d`], [`dispatch_3d`] — ceil-div workgroup
//!   grid calculators clamped to [`MAX_WORKGROUPS`].
//! * [`PipelineCache`] — in-memory Rust-level pipeline cache keyed by a hash
//!   of (WGSL source, entry-point name).  Returns `Arc<wgpu::ComputePipeline>`
//!   so pipelines can be shared cheaply across dispatches.
//! * [`DispatchBuilder`] — fluent builder that binds bind-groups, sets
//!   dispatch dimensions, and submits the compute pass in one call.
//! * [`DispatchResult`] — submission index returned by [`DispatchBuilder::submit`].
//! * [`validate_immediates`] / [`supports_immediates`] — wgpu 29 "immediates"
//!   (formerly push-constants) alignment validation helpers.
//! * [`encode_indirect_dispatch`] — GPU-driven indirect dispatch helper.
//! * [`checked_compute_pipeline`] — like [`compute_pipeline`] but surfaces
//!   WGSL compilation errors via [`crate::ComputeError::ShaderCompilation`]
//!   rather than panicking.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum safe workgroups per dimension (conservatively capped to the Vulkan /
/// Metal common baseline of 65 535).
pub const MAX_WORKGROUPS: u32 = 65535;

// ── compute_pipeline ──────────────────────────────────────────────────────────

/// Compile a WGSL compute shader and return a ready-to-dispatch
/// [`wgpu::ComputePipeline`].
///
/// # Parameters
/// - `device`       — the logical wgpu device.
/// - `wgsl_source`  — the full WGSL shader source text.
/// - `entry_point`  — the name of the `@compute` entry point in the shader.
///
/// The pipeline uses `wgpu::PipelineCompilationOptions::default()` and an
/// auto-layout (`layout: None`), so bind-group layouts are reflected from the
/// shader at compile time.
///
/// # Panics
/// Panics if the WGSL source fails to compile (shader syntax errors, missing
/// entry point, …).  This mirrors wgpu's own behaviour for
/// `create_compute_pipeline`.
///
/// # Example
/// ```rust,no_run
/// use oxiui_compute_wgpu::{ComputeContext, compute_pipeline};
///
/// const SHADER: &str = r#"
///     @group(0) @binding(0) var<storage, read_write> data: array<f32>;
///
///     @compute @workgroup_size(64)
///     fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
///         data[gid.x] *= 2.0;
///     }
/// "#;
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let pipeline = compute_pipeline(&ctx.device, SHADER, "main");
///     let _ = pipeline;
/// }
/// ```
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "debug", skip(device, wgsl_source))
)]
pub fn compute_pipeline(
    device: &wgpu::Device,
    wgsl_source: &str,
    entry_point: &str,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("oxiui-compute-wgpu shader"),
        source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
    });

    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("oxiui-compute-wgpu pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

// ── Workgroup-size helpers ────────────────────────────────────────────────────

/// Compute a 1-D dispatch grid: `ceil(n / workgroup_size)`, clamped to
/// [`MAX_WORKGROUPS`].
///
/// # Example
/// ```
/// use oxiui_compute_wgpu::dispatch_1d;
/// assert_eq!(dispatch_1d(100, 64), 2);
/// assert_eq!(dispatch_1d(128, 64), 2);
/// ```
pub fn dispatch_1d(n: u32, workgroup_size: u32) -> u32 {
    n.div_ceil(workgroup_size).min(MAX_WORKGROUPS)
}

/// Compute a 2-D dispatch grid: `(ceil(w/wx), ceil(h/wy))`, each axis clamped
/// to [`MAX_WORKGROUPS`].
///
/// # Example
/// ```
/// use oxiui_compute_wgpu::dispatch_2d;
/// assert_eq!(dispatch_2d(100, 200, 16, 16), (7, 13));
/// ```
pub fn dispatch_2d(width: u32, height: u32, wg_x: u32, wg_y: u32) -> (u32, u32) {
    (
        width.div_ceil(wg_x).min(MAX_WORKGROUPS),
        height.div_ceil(wg_y).min(MAX_WORKGROUPS),
    )
}

/// Compute a 3-D dispatch grid, each axis clamped to [`MAX_WORKGROUPS`].
///
/// # Example
/// ```
/// use oxiui_compute_wgpu::dispatch_3d;
/// assert_eq!(dispatch_3d(10, 10, 10, 4, 4, 4), (3, 3, 3));
/// ```
pub fn dispatch_3d(x: u32, y: u32, z: u32, wg_x: u32, wg_y: u32, wg_z: u32) -> (u32, u32, u32) {
    (
        x.div_ceil(wg_x).min(MAX_WORKGROUPS),
        y.div_ceil(wg_y).min(MAX_WORKGROUPS),
        z.div_ceil(wg_z).min(MAX_WORKGROUPS),
    )
}

// ── PipelineCache ─────────────────────────────────────────────────────────────

/// Compute a deterministic hash for a (WGSL source, entry-point) pair.
fn wgsl_hash(source: &str, entry_point: &str) -> u64 {
    let mut h = DefaultHasher::new();
    source.hash(&mut h);
    entry_point.hash(&mut h);
    h.finish()
}

/// In-memory Rust-level pipeline cache keyed by a hash of the WGSL source
/// plus entry-point name.
///
/// This is **not** wgpu's own on-disk binary pipeline cache
/// (`wgpu::PipelineCache`).  It is a Rust `HashMap` that avoids calling
/// `wgpu::Device::create_compute_pipeline` more than once for the same shader.
/// Pipelines are returned as `Arc<wgpu::ComputePipeline>` so they can be
/// cheaply cloned and shared across frames.
///
/// # Example
/// ```rust,no_run
/// use oxiui_compute_wgpu::{ComputeContext, PipelineCache};
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let mut cache = PipelineCache::new();
///     let wgsl = "@compute @workgroup_size(1) fn noop() {}";
///     let p1 = cache.get_or_compile(&ctx.device, wgsl, "noop");
///     let p2 = cache.get_or_compile(&ctx.device, wgsl, "noop");
///     assert_eq!(cache.compile_count(), 1); // compiled only once
///     assert!(std::sync::Arc::ptr_eq(&p1, &p2));
/// }
/// ```
pub struct PipelineCache {
    cache: HashMap<u64, Arc<wgpu::ComputePipeline>>,
    compile_count: usize,
}

impl PipelineCache {
    /// Create a new, empty `PipelineCache`.
    pub fn new() -> Self {
        PipelineCache {
            cache: HashMap::new(),
            compile_count: 0,
        }
    }

    /// Return a cached pipeline if the (source, entry_point) pair was seen
    /// before, or compile and cache a new one.
    pub fn get_or_compile(
        &mut self,
        device: &wgpu::Device,
        wgsl_source: &str,
        entry_point: &str,
    ) -> Arc<wgpu::ComputePipeline> {
        let key = wgsl_hash(wgsl_source, entry_point);
        if let Some(p) = self.cache.get(&key) {
            return Arc::clone(p);
        }
        let pipeline = Arc::new(compute_pipeline(device, wgsl_source, entry_point));
        self.cache.insert(key, Arc::clone(&pipeline));
        self.compile_count += 1;
        pipeline
    }

    /// Number of compilations performed (cache misses since construction).
    pub fn compile_count(&self) -> usize {
        self.compile_count
    }

    /// Number of unique (source, entry-point) pairs currently cached.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Returns `true` when no pipelines have been cached yet.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── DispatchBuilder + DispatchResult ──────────────────────────────────────────

/// Result of a dispatched compute pass — wraps the wgpu submission index for
/// later polling via `device.poll(wgpu::PollType::WaitForSubmissionIndex(…))`.
pub struct DispatchResult {
    /// The wgpu submission index returned by `queue.submit(…)`.
    pub submission_index: wgpu::SubmissionIndex,
}

/// Fluent builder that encodes and submits a compute dispatch in one call.
///
/// # Example
/// ```rust,no_run
/// use oxiui_compute_wgpu::{ComputeContext, compute_pipeline, DispatchBuilder};
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let pipeline = compute_pipeline(&ctx.device, "@compute @workgroup_size(1) fn noop() {}", "noop");
///     let result = DispatchBuilder::new(&pipeline)
///         .dispatch_1d(1024, 64)
///         .label("my-pass")
///         .submit(&ctx.device, &ctx.queue);
///     let _ = result.submission_index;
/// }
/// ```
pub struct DispatchBuilder<'a> {
    pipeline: &'a wgpu::ComputePipeline,
    bind_groups: Vec<(u32, &'a wgpu::BindGroup)>,
    dispatch: (u32, u32, u32),
    label: Option<&'a str>,
}

impl<'a> DispatchBuilder<'a> {
    /// Create a new `DispatchBuilder` for the given pipeline.
    pub fn new(pipeline: &'a wgpu::ComputePipeline) -> Self {
        DispatchBuilder {
            pipeline,
            bind_groups: Vec::new(),
            dispatch: (1, 1, 1),
            label: None,
        }
    }

    /// Bind a `wgpu::BindGroup` at the specified index.
    pub fn bind(mut self, index: u32, bind_group: &'a wgpu::BindGroup) -> Self {
        self.bind_groups.push((index, bind_group));
        self
    }

    /// Set a 1-D dispatch using [`dispatch_1d`] to compute the grid size.
    pub fn dispatch_1d(mut self, n: u32, workgroup_size: u32) -> Self {
        self.dispatch = (crate::pipeline::dispatch_1d(n, workgroup_size), 1, 1);
        self
    }

    /// Set the dispatch grid directly (x, y, z workgroups).
    pub fn dispatch_xyz(mut self, x: u32, y: u32, z: u32) -> Self {
        self.dispatch = (x, y, z);
        self
    }

    /// Attach a debug label to the command encoder and compute pass.
    pub fn label(mut self, l: &'a str) -> Self {
        self.label = Some(l);
        self
    }

    /// Encode and submit the compute dispatch.
    ///
    /// Creates a command encoder, begins a compute pass, sets the pipeline and
    /// all bound bind-groups, dispatches the workgroups, ends the pass, and
    /// submits the encoded commands to the queue.
    ///
    /// Returns a [`DispatchResult`] carrying the submission index.
    pub fn submit(self, device: &wgpu::Device, queue: &wgpu::Queue) -> DispatchResult {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: self.label });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: self.label,
                timestamp_writes: None,
            });
            pass.set_pipeline(self.pipeline);
            for (idx, bg) in &self.bind_groups {
                pass.set_bind_group(*idx, *bg, &[]);
            }
            let (x, y, z) = self.dispatch;
            pass.dispatch_workgroups(x, y, z);
        }
        let submission_index = queue.submit(std::iter::once(encoder.finish()));
        DispatchResult { submission_index }
    }
}

// ── Immediates (wgpu 29 — renamed from push constants) ───────────────────────

/// Validates immediates (wgpu 29's renamed push constants) alignment
/// requirements.
///
/// Returns `Err` with a human-readable message when:
/// * `offset` is not a multiple of [`wgpu::IMMEDIATE_DATA_ALIGNMENT`], or
/// * `data_len` is not a multiple of [`wgpu::IMMEDIATE_DATA_ALIGNMENT`].
///
/// # Example
/// ```
/// use oxiui_compute_wgpu::validate_immediates;
/// assert!(validate_immediates(0, 16).is_ok());
/// assert!(validate_immediates(1, 4).is_err());
/// assert!(validate_immediates(0, 3).is_err());
/// ```
pub fn validate_immediates(offset: u32, data_len: usize) -> Result<(), String> {
    let align = wgpu::IMMEDIATE_DATA_ALIGNMENT;
    if !offset.is_multiple_of(align) {
        return Err(format!(
            "immediates offset {offset} must be aligned to {align}"
        ));
    }
    if !(data_len as u32).is_multiple_of(align) {
        return Err(format!(
            "immediates data length {data_len} must be aligned to {align}"
        ));
    }
    Ok(())
}

/// Returns `true` when the given device feature set includes
/// `wgpu::Features::IMMEDIATES` (the wgpu 29 rename of push constants).
pub fn supports_immediates(features: wgpu::Features) -> bool {
    features.contains(wgpu::Features::IMMEDIATES)
}

// ── Indirect dispatch helper ──────────────────────────────────────────────────

/// Encode an indirect compute dispatch.
///
/// The `indirect_buffer` must contain the workgroup counts encoded as three
/// consecutive `u32` values `[x, y, z]` at the given `offset`.  Use
/// `wgpu::util::DispatchIndirectArgs` (or write raw `u32` bytes) to prepare
/// the buffer before submitting.
pub fn encode_indirect_dispatch<'enc>(
    pass: &mut wgpu::ComputePass<'enc>,
    indirect_buffer: &'enc wgpu::Buffer,
    offset: u64,
) {
    pass.dispatch_workgroups_indirect(indirect_buffer, offset);
}

// ── checked_compute_pipeline ──────────────────────────────────────────────────

/// Compile a WGSL compute shader and surface any compilation errors as a
/// [`crate::ComputeError::ShaderCompilation`] rather than panicking.
///
/// Diagnostics include line number and column position as reported by
/// `wgpu::CompilationInfo::messages`.
///
/// # Errors
///
/// Returns [`crate::ComputeError::ShaderCompilation`] when the shader contains
/// one or more `Error`-level compilation messages.
///
/// # Example
/// ```rust,no_run
/// use oxiui_compute_wgpu::{ComputeContext, checked_compute_pipeline};
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let wgsl = r#"
///         @group(0) @binding(0) var<storage, read_write> buf: array<f32>;
///         @compute @workgroup_size(64)
///         fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
///             buf[gid.x] *= 2.0;
///         }
///     "#;
///     let pipeline = checked_compute_pipeline(&ctx.device, wgsl, "main").unwrap();
///     let _ = pipeline;
/// }
/// ```
pub fn checked_compute_pipeline(
    device: &wgpu::Device,
    wgsl_source: &str,
    entry_point: &str,
) -> Result<wgpu::ComputePipeline, crate::ComputeError> {
    use wgpu::CompilationMessageType;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
    });

    let info = pollster::block_on(shader.get_compilation_info());
    let errors: Vec<String> = info
        .messages
        .iter()
        .filter(|m| m.message_type == CompilationMessageType::Error)
        .map(|m| {
            if let Some(loc) = &m.location {
                format!("{}:{}: {}", loc.line_number, loc.line_position, m.message)
            } else {
                m.message.clone()
            }
        })
        .collect();

    if !errors.is_empty() {
        return Err(crate::ComputeError::ShaderCompilation(errors.join("; ")));
    }

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    Ok(pipeline)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ComputeContext;

    /// Minimal WGSL compute shader used for pipeline compilation tests.
    const PASSTHROUGH_WGSL: &str = r#"
        @group(0) @binding(0) var<storage, read_write> buf: array<f32>;

        @compute @workgroup_size(1)
        fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
            buf[gid.x] = buf[gid.x];
        }
    "#;

    // ── Existing tests (preserved) ───────────────────────────────────────────

    #[test]
    fn compute_pipeline_compiles() {
        let Some(ctx) = ComputeContext::try_new() else {
            return; // no GPU on this host — graceful skip
        };
        // Must not panic.
        let _pipeline = compute_pipeline(&ctx.device, PASSTHROUGH_WGSL, "main");
    }

    #[test]
    fn compute_pipeline_double_shader() {
        let Some(ctx) = ComputeContext::try_new() else {
            return; // graceful skip
        };

        const DOUBLE_WGSL: &str = r#"
            @group(0) @binding(0) var<storage, read_write> data: array<f32>;

            @compute @workgroup_size(64)
            fn double_all(@builtin(global_invocation_id) gid: vec3<u32>) {
                data[gid.x] *= 2.0;
            }
        "#;

        let _pipeline = compute_pipeline(&ctx.device, DOUBLE_WGSL, "double_all");
    }

    // ── Workgroup-size helper tests (non-GPU) ────────────────────────────────

    #[test]
    fn dispatch_1d_100_wg64() {
        assert_eq!(dispatch_1d(100, 64), 2);
    }

    #[test]
    fn dispatch_1d_exact_multiple() {
        assert_eq!(dispatch_1d(128, 64), 2);
    }

    #[test]
    fn dispatch_1d_one_element() {
        assert_eq!(dispatch_1d(1, 64), 1);
    }

    #[test]
    fn dispatch_1d_clamp_at_max() {
        assert_eq!(dispatch_1d(u32::MAX, 1), MAX_WORKGROUPS);
    }

    #[test]
    fn dispatch_2d_smoke() {
        assert_eq!(dispatch_2d(100, 200, 16, 16), (7, 13));
    }

    #[test]
    fn dispatch_3d_smoke() {
        assert_eq!(dispatch_3d(10, 10, 10, 4, 4, 4), (3, 3, 3));
    }

    // ── PipelineCache tests (non-GPU) ────────────────────────────────────────

    #[test]
    fn pipeline_cache_default_empty() {
        let cache = PipelineCache::new();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    // ── Immediates validation tests (non-GPU) ────────────────────────────────

    #[test]
    fn validate_immediates_aligned_ok() {
        assert!(validate_immediates(0, 16).is_ok());
        assert!(validate_immediates(4, 8).is_ok());
    }

    #[test]
    fn validate_immediates_offset_unaligned() {
        assert!(validate_immediates(1, 4).is_err());
        assert!(validate_immediates(3, 4).is_err());
    }

    #[test]
    fn validate_immediates_data_unaligned() {
        assert!(validate_immediates(0, 3).is_err());
        assert!(validate_immediates(0, 5).is_err());
    }

    /// Non-GPU: verify the internal hash function distinguishes different inputs.
    #[test]
    fn wgsl_hash_distinguishes_sources() {
        assert_ne!(wgsl_hash("a", "main"), wgsl_hash("b", "main"));
        assert_ne!(wgsl_hash("src", "entry_a"), wgsl_hash("src", "entry_b"));
        assert_eq!(wgsl_hash("same", "ep"), wgsl_hash("same", "ep"));
    }

    // ── GPU-gated tests ──────────────────────────────────────────────────────

    #[test]
    fn pipeline_cache_hit_reuses_arc() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let mut cache = PipelineCache::new();
        let p1 = cache.get_or_compile(&ctx.device, PASSTHROUGH_WGSL, "main");
        let p2 = cache.get_or_compile(&ctx.device, PASSTHROUGH_WGSL, "main");
        assert!(Arc::ptr_eq(&p1, &p2), "cache hit must return the same Arc");
        assert_eq!(cache.compile_count(), 1, "compiled only once");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn pipeline_cache_miss_compiles_twice() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        const WGSL_B: &str = r#"
            @group(0) @binding(0) var<storage, read_write> buf: array<u32>;
            @compute @workgroup_size(1)
            fn alt(@builtin(global_invocation_id) gid: vec3<u32>) {
                buf[gid.x] = gid.x;
            }
        "#;
        let mut cache = PipelineCache::new();
        let _p1 = cache.get_or_compile(&ctx.device, PASSTHROUGH_WGSL, "main");
        let _p2 = cache.get_or_compile(&ctx.device, WGSL_B, "alt");
        assert_eq!(
            cache.compile_count(),
            2,
            "two different shaders → two compilations"
        );
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn dispatch_builder_doubling() {
        use bytemuck::{Pod, Zeroable};

        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());

        const DOUBLE_WGSL: &str = r#"
            @group(0) @binding(0) var<storage, read_write> data: array<f32>;

            @compute @workgroup_size(64)
            fn double_all(@builtin(global_invocation_id) gid: vec3<u32>) {
                let i = gid.x;
                if i < arrayLength(&data) {
                    data[i] = data[i] * 2.0;
                }
            }
        "#;

        const N: usize = 128;
        let input: Vec<f32> = (0..N as u32).map(|i| i as f32).collect();

        // Upload data.
        let buf_size = (N * std::mem::size_of::<f32>()) as u64;
        let storage_buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("storage"),
            size: buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });
        {
            let mut view = storage_buf.slice(..).get_mapped_range_mut();
            view.copy_from_slice(bytemuck::cast_slice(&input));
        }
        storage_buf.unmap();

        // Bind-group layout + bind group.
        let layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: storage_buf.as_entire_binding(),
            }],
        });

        // Compile pipeline with explicit layout so bind group matches.
        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(DOUBLE_WGSL.into()),
            });
        let pipeline = ctx
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("double_all"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        // Dispatch via DispatchBuilder.
        let result = DispatchBuilder::new(&pipeline)
            .bind(0, &bind_group)
            .dispatch_1d(N as u32, 64)
            .label("doubling-pass")
            .submit(&ctx.device, &ctx.queue);

        // Readback via staging buffer.
        let staging = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: buf_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy-back"),
            });
        encoder.copy_buffer_to_buffer(&storage_buf, 0, &staging, 0, buf_size);
        ctx.queue.submit(std::iter::once(encoder.finish()));
        let _ = ctx.device.poll(wgpu::PollType::Wait {
            submission_index: Some(result.submission_index),
            timeout: None,
        });

        let (tx, rx) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().unwrap().unwrap();

        let output: Vec<f32> = {
            let view = staging.slice(..).get_mapped_range();
            bytemuck::cast_slice::<u8, f32>(&view).to_vec()
        };

        for (i, (&got, expected)) in output.iter().zip(input.iter().map(|v| v * 2.0)).enumerate() {
            assert!(
                (got - expected).abs() < 1e-6,
                "index {i}: got {got}, expected {expected}"
            );
        }

        // Suppress unused-import warning for Pod/Zeroable brought in by bytemuck.
        let _: () = {
            #[allow(dead_code)]
            fn _use_pod<T: Pod + Zeroable>() {}
        };
    }

    #[test]
    fn checked_pipeline_valid_shader_ok() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let result = checked_compute_pipeline(&ctx.device, PASSTHROUGH_WGSL, "main");
        assert!(result.is_ok(), "valid shader must compile without error");
    }
}
