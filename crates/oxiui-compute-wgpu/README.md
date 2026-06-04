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
oxiui-compute-wgpu = "0.1"
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

| Module     | Key exports |
|------------|-------------|
| `context`  | `ComputeContext`, `ContextBuilder` |
| `buffer`   | `storage_buffer_init`, `uniform_buffer`, `staging_buffer`, `read_back`, `read_back_range`, `read_back_async`, `TypedBuffer<T>`, `BufferPool`, `SubAllocator`, `mapped_storage_init` |
| `pipeline` | `compute_pipeline`, `checked_compute_pipeline`, `PipelineCache`, `DispatchBuilder`, `dispatch_1d`, `dispatch_2d`, `dispatch_3d`, `validate_immediates`, `encode_indirect_dispatch` |
| `wgsl`     | `preprocess`, `validate`, `SHADER_PREFIX_SUM`, `SHADER_REDUCTION_SUM`, `SHADER_HISTOGRAM`, `SHADER_MATMUL` |
| `error`    | `ComputeError` (`NoAdapter`, `DeviceRequest`, `OutOfMemory`, `ShaderCompilation`, `Operation`) |

## Built-in Kernels

Four validated WGSL kernels ship as `pub const` source strings in `wgsl`. Each
uses the entry point `main_cs` and is compiled with `compute_pipeline` (or
`checked_compute_pipeline`).

| Constant | Entry point | Constraints |
|----------|-------------|-------------|
| `SHADER_PREFIX_SUM` | `main_cs` | Inclusive scan, `f32`. Single workgroup; input length ≤ 256. Dispatch one workgroup of size 256. |
| `SHADER_REDUCTION_SUM` | `main_cs` | Sum reduction, `f32 → f32`. Single workgroup; input length ≤ 256. Dispatch one workgroup of size 256. |
| `SHADER_HISTOGRAM` | `main_cs` | `u32` values → bin counts. Up to 256 bins; each element binned as `input[i] % num_bins`. Workgroup size 64; workgroup-local atomic histogram. |
| `SHADER_MATMUL` | `main_cs` | Tiled `f32` matmul, M×K · K×N → M×N (row-major). 16×16 shared-memory tiles; bind `MatDims { M, K, N }` uniform at `@binding(3)`. Dispatch `ceil(N/16) × ceil(M/16) × 1`. |

## wgpu 29 Notes

This crate targets wgpu 29. Relevant API changes from earlier versions:

- Push constants are now **immediates**: `wgpu::Features::IMMEDIATES`,
  `ComputePass::set_immediates`, and `Limits::max_immediate_size`. Alignment is
  `wgpu::IMMEDIATE_DATA_ALIGNMENT` (4); validate with `validate_immediates`.
- `Instance::request_adapter` returns `Result` (not `Option`).
- `Adapter::request_device` takes a single `&DeviceDescriptor` argument.

## Feature Flags

None. All functionality is on by default. GPU access is optional at runtime:
`ComputeContext::try_new` returns `None` on headless hosts (CI, VMs without GPU
pass-through), and `ComputeContext::new` / `ContextBuilder::build` return
`ComputeError::NoAdapter`, so callers can skip gracefully rather than fail.

## Related Crates

- [`oxiui`](../oxiui) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `RenderBackend`, `DrawList`, geometry types, shared `require_gpu!` macro.
- [`oxiui-render-soft`](../oxiui-render-soft) — CPU render backend; will use compute shaders for blur/dithering.
- [`oxiui-render-wgpu`](../oxiui-render-wgpu) — GPU render backend; shares the device/queue with this crate.
- [`oxiui-text`](../oxiui-text) — text pipeline; GPU glyph rasterization is a planned consumer.
- [`oxiui-theme`](../oxiui-theme) — design tokens and `ShadowSpec`.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
