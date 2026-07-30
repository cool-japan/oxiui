# oxiui-compute-wgpu — Pure-Rust wgpu GPU-compute abstraction for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-compute-wgpu.svg)](https://crates.io/crates/oxiui-compute-wgpu)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-compute-wgpu` is the shared GPU-compute layer for the COOLJAPAN
ecosystem. It consolidates the repeated `Instance → Adapter → Device → Queue`
initialisation boilerplate that headless compute workloads (sparse solvers,
Lattice-Boltzmann, Monte-Carlo) each duplicated, and adds typed buffers, buffer
pooling and sub-allocation, pipeline caching, dispatch helpers, a WGSL
preprocessor and validator, and a set of validated built-in compute kernels.
`#![forbid(unsafe_code)]` is enforced crate-wide; `pollster` blocks on wgpu's
async adapter/device requests so the public API stays synchronous; `bytemuck`
handles zero-copy `Pod` casting between CPU and GPU.

Targets wgpu 29. See the [wgpu 29 Notes](#wgpu-29-notes) for the renamed APIs.

## Installation

```toml
[dependencies]
oxiui-compute-wgpu = "0.2.1"
```

`wgpu`, `bytemuck`, and `pollster` are re-exported, so a single dependency
declaration is enough.

## Quick Start

Configure a context with the builder, double every value in a buffer on the GPU
with a `DispatchBuilder`, and read the result back:

```rust,no_run
use oxiui_compute_wgpu::{
    bytemuck, compute_pipeline, read_back, storage_buffer_init, wgpu,
    ComputeContext, DispatchBuilder,
};

// 1. Configure and build a compute context (Err on hosts with no GPU adapter).
let Ok(ctx) = ComputeContext::builder()
    .with_power_preference(wgpu::PowerPreference::HighPerformance)
    .build()
else {
    return; // no GPU — skip gracefully
};

// 2. Upload input data to a storage buffer.
let input: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
let buffer = storage_buffer_init(&ctx.device, "values", bytemuck::cast_slice(&input));

// 3. Compile a WGSL compute shader (auto-layout from reflection).
const SHADER: &str = r#"
    @group(0) @binding(0) var<storage, read_write> data: array<f32>;

    @compute @workgroup_size(64)
    fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
        if gid.x < arrayLength(&data) {
            data[gid.x] = data[gid.x] * 2.0;
        }
    }
"#;
let pipeline = compute_pipeline(&ctx.device, SHADER, "main");

// 4. Bind, dispatch (ceil-div grid), and submit in one call.
let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("values-bind"),
    layout: &pipeline.get_bind_group_layout(0),
    entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: buffer.as_entire_binding(),
    }],
});

DispatchBuilder::new(&pipeline)
    .bind(0, &bind_group)
    .dispatch_1d(input.len() as u32, 64)
    .label("double")
    .submit(&ctx.device, &ctx.queue);

// 5. Read the results back to the CPU.
let output: Vec<f32> = read_back(&ctx.device, &ctx.queue, &buffer, input.len());
assert_eq!(output, vec![2.0, 4.0, 6.0, 8.0]);
```

## API Overview

| Module        | Key exports |
|---------------|-------------|
| `context`     | `ComputeContext`, `ContextBuilder` |
| `buffer`      | `storage_buffer_init`, `uniform_buffer`, `staging_buffer`, `read_back`, `read_back_range`, `read_back_async`, `TypedBuffer<T>`, `BufferPool`, `SubAllocator`, `mapped_storage_init` |
| `pipeline`    | `compute_pipeline`, `checked_compute_pipeline`, `PipelineCache`, `DispatchBuilder`, `dispatch_1d`, `dispatch_2d`, `dispatch_3d`, `validate_immediates`, `encode_indirect_dispatch` |
| `dispatch`    | `Dispatcher` — high-level, zero-boilerplate GPU ops (see below) |
| `wgsl`        | `preprocess`, `validate`, `SHADER_PREFIX_SUM`, `SHADER_REDUCTION_SUM`, `SHADER_HISTOGRAM`, `SHADER_MATMUL`, `SHADER_SPH_DENSITY`, `SHADER_BITONIC_SORT`, `SHADER_MAP_F32_TEMPLATE`, `SHADER_ZIP_MAP_F32_TEMPLATE` |
| `integration` | Bridges to `oxiui-render-soft`, `oxiui-render-wgpu`, `oxiui-text` (see below) |
| `error`       | `ComputeError` (`NoAdapter`, `DeviceRequest`, `OutOfMemory`, `ShaderCompilation`, `Operation`) |

### `Dispatcher` — one-call GPU operations

`Dispatcher::new(&ctx)` wraps a [`ComputeContext`] and compiles, uploads, dispatches, and reads back in a single synchronous call:

| Method | Description |
|--------|-------------|
| `map_f32(src, op)` | Element-wise transform via a WGSL expression (instantiates `SHADER_MAP_F32_TEMPLATE`) |
| `zip_map_f32(a, b, op)` | Binary element-wise transform via a WGSL expression (instantiates `SHADER_ZIP_MAP_F32_TEMPLATE`) |
| `reduce_sum_f32(data)` | Sum all elements (`SHADER_REDUCTION_SUM`) |
| `sph_density(positions, masses, h)` | SPH density via the cubic-spline kernel (`SHADER_SPH_DENSITY`) |
| `sort_f32(data)` | Ascending sort via bitonic sort (`SHADER_BITONIC_SORT`, driving all merge-network steps) |

### `integration` module — cross-crate GPU bridges

| Sub-module | Provides |
|------------|----------|
| `integration::render_soft` | `gpu_gaussian_blur_rgba`, `gpu_ordered_dither_rgba`, `gpu_linear_gradient_fill` (+ `GradientAxis`) — GPU compute replacements for `oxiui-render-soft` CPU paths |
| `integration::render_wgpu` | `SharedDevice`, `SharedComputeContext`, `extract_from_context` — share a `wgpu::Device`/`Queue` with `oxiui-render-wgpu` instead of opening a second one |
| `integration::text` | `GlyphRasterizer`, `rasterize_glyphs`, `cpu_rasterize_glyphs`, `GlyphEntry`, `GlyphAtlasParams` — GPU glyph-rasterization compute pass for `oxiui-text` |

## Built-in Kernels

Eight validated WGSL kernels ship as `pub const` source strings in `wgsl`, compiled with `compute_pipeline` (or `checked_compute_pipeline`). Four are complete shaders (entry point `main_cs`); the SPH, bitonic-sort, map, and zip-map kernels use their own named entry points, and the map/zip-map kernels are `%%OP%%` templates instantiated via `str::replace` before compilation (see [`Dispatcher::map_f32`](#dispatcher--one-call-gpu-operations) / `zip_map_f32` for the ergonomic wrapper).

| Constant | Entry point | Constraints |
|----------|-------------|-------------|
| `SHADER_PREFIX_SUM` | `main_cs` | Inclusive scan, `f32`. Single workgroup; input length ≤ 256. Dispatch one workgroup of size 256. |
| `SHADER_REDUCTION_SUM` | `main_cs` | Sum reduction, `f32 → f32`. Single workgroup; input length ≤ 256. Dispatch one workgroup of size 256. |
| `SHADER_HISTOGRAM` | `main_cs` | `u32` values → bin counts. Up to 256 bins; each element binned as `input[i] % num_bins`. Workgroup size 64; workgroup-local atomic histogram. |
| `SHADER_MATMUL` | `main_cs` | Tiled `f32` matmul, M×K · K×N → M×N (row-major). 16×16 shared-memory tiles; bind `MatDims { M, K, N }` uniform at `@binding(3)`. Dispatch `ceil(N/16) × ceil(M/16) × 1`. |
| `SHADER_SPH_DENSITY` | `main_sph` | SPH density `ρᵢ = Σⱼ mⱼ · W(\|rᵢ − rⱼ\|, h)` via the poly-6 cubic-spline kernel. Bindings: positions (`vec4<f32>`, xyz+pad), masses, output densities, `SphParams` uniform. Dispatch `ceil(n/64)` workgroups of 64. |
| `SHADER_BITONIC_SORT` | `main_bitonic` | One step of a bitonic merge network on `array<f32>`; the caller (see `Dispatcher::sort_f32`) invokes it once per `(k, j)` pair until sorted. Dispatch `ceil(n/64)` workgroups of 64 per step. |
| `SHADER_MAP_F32_TEMPLATE` | `main_map` | Template — replace `%%OP%%` with a WGSL expression over `x`. `src: array<f32>` → `dst: array<f32>`. Dispatch `ceil(n/64)` workgroups of 64. |
| `SHADER_ZIP_MAP_F32_TEMPLATE` | `main_zip_map` | Template — replace `%%OP%%` with a WGSL expression over `a`/`b`. `a_buf`, `b_buf: array<f32>` → `dst: array<f32>`. Dispatch `ceil(n/64)` workgroups of 64. |

## wgpu 29 Notes

This crate targets wgpu 29. Relevant API changes from earlier versions:

- Push constants are now **immediates**: `wgpu::Features::IMMEDIATES`,
  `ComputePass::set_immediates`, and `Limits::max_immediate_size`. Alignment is
  `wgpu::IMMEDIATE_DATA_ALIGNMENT` (4); validate with `validate_immediates`.
- `Instance::request_adapter` returns `Result` (not `Option`).
- `Adapter::request_device` takes a single `&DeviceDescriptor` argument.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `tracing` | no | Adds `#[tracing::instrument]` spans to `ComputeContext::new`, `read_back`, and `compute_pipeline` for span-based profiling |

All core functionality (contexts, buffers, pipelines, dispatch, WGSL utilities, built-in kernels) is available with `default = []` — no feature is required to use the crate. GPU access is optional at runtime:
`ComputeContext::try_new` returns `None` on headless hosts (CI, VMs without GPU
pass-through), and `ComputeContext::new` / `ContextBuilder::build` return
`ComputeError::NoAdapter`, so callers can skip gracefully rather than fail.

Shader hot-reload does **not** live in this crate: the `notify`-backed `ShaderWatcher` was moved to the separate [`oxiui-hot-reload-notify`](../oxiui-hot-reload-notify) crate so `oxiui-compute-wgpu` stays Pure Rust by default (`notify` pulls in `inotify-sys` / `fsevent-sys` on some platforms).

## Related Crates

- [`oxiui`](../oxiui) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `RenderBackend`, `DrawList`, geometry types, shared `require_gpu!` macro.
- [`oxiui-render-soft`](../oxiui-render-soft) — CPU render backend; `integration::render_soft` provides GPU-compute Gaussian blur, ordered dithering, and linear-gradient fill as drop-in replacements for its CPU paths.
- [`oxiui-render-wgpu`](../oxiui-render-wgpu) — GPU render backend; `integration::render_wgpu` shares its `wgpu::Device`/`Queue` with this crate instead of opening a second one.
- [`oxiui-text`](../oxiui-text) — text pipeline; `integration::text` provides a GPU glyph-rasterization compute pass.
- [`oxiui-theme`](../oxiui-theme) — design tokens and `ShadowSpec`.
- [`oxiui-hot-reload-notify`](../oxiui-hot-reload-notify) — the `notify`-backed WGSL `ShaderWatcher`, kept out of this crate to preserve its Pure-Rust default build.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
