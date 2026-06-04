# oxiui-compute-wgpu TODO

Roadmap for the Pure-Rust wgpu GPU-compute abstraction.

## Status (2026-06-02)

Round 1 implementation is complete. Shipped in this round:

- `require_gpu!` macro (lives in `oxiui-core`; shared CI-skip macro for every crate).
- `ContextBuilder` with `with_limits` / `with_features` / `with_power_preference`,
  plus `adapter_info()` and `ComputeContext::new_async()`.
- `TypedBuffer<T>`, `BufferPool`, `SubAllocator` (with `SubRegion`), and
  `mapped_storage_init` for the integrated-GPU zero-copy upload path.
- `read_back_range` (partial readback by byte offset) and `read_back_async`
  (non-blocking readback future).
- `dispatch_1d` / `dispatch_2d` / `dispatch_3d`, `PipelineCache`,
  `DispatchBuilder`, and `checked_compute_pipeline`.
- `validate_immediates` / `supports_immediates` (wgpu 29 renamed push constants
  to "immediates").
- `encode_indirect_dispatch` for GPU-driven indirect dispatch.
- `wgsl::preprocess` (`#include` stitching with cycle detection and a depth cap)
  and `wgsl::validate` returning structured `WgslDiagnostic` values.
- Built-in WGSL kernels: `SHADER_PREFIX_SUM`, `SHADER_REDUCTION_SUM`,
  `SHADER_HISTOGRAM`, `SHADER_MATMUL`.
- `ComputeError` extended with `OutOfMemory`, `ShaderCompilation`, and
  `Operation { op, detail }`.
- Runnable `lib.rs` doc-test (executes the full dispatch, GPU-optional).
- `examples/prefix_sum.rs` and `examples/matrix_mul.rs`.
- 65 tests + 18 doc-tests, 0 warnings.

The sections below keep the completed (`[x]`) items as a record and list the
remaining (`[ ]`) backlog with its planning notes.

## Core Implementation

- [x] `ComputeContext::with_limits(limits: wgpu::Limits) -> Result<Self, ComputeError>` — let callers configure `max_buffer_size`, `max_storage_buffer_binding_size`, `max_texture_dimension_2d`, and related caps instead of forcing `DeviceDescriptor::default()`.
- [x] `ComputeContext::with_features(features: wgpu::Features) -> Result<Self, ComputeError>` — opt in to optional GPU features (`TIMESTAMP_QUERY`, `SHADER_F16`, `IMMEDIATES`, `INDIRECT_FIRST_INSTANCE`); fail with a clear error when the adapter does not support a requested feature.
- [x] `ComputeContext::with_power_preference(pref: wgpu::PowerPreference) -> Result<Self, ComputeError>` — expose `LowPower` / `HighPerformance` selection rather than hard-coding `HighPerformance`.
- [x] `ComputeContext::adapter_info(&self) -> &wgpu::AdapterInfo` — store the adapter handle at construction time and expose vendor, device, backend, and driver metadata for logging and capability gating.
- [x] Multi-queue support — request and expose separate transfer and compute queues on adapters that advertise more than one queue family; fall back to a single shared queue otherwise.
- [x] `async fn new_async() -> Result<Self, ComputeError>` — await `request_adapter` / `request_device` directly for use inside async runtimes (Tokio, async-std), removing the `pollster::block_on` wrapper on the async path.
- [x] Builder entry point — `ComputeContext::builder()` returning a `ContextBuilder` that composes limits, features, and power preference in one fluent chain.

## Buffer Management

- [x] `TypedBuffer<T: Pod>` — typed wrapper around `wgpu::Buffer` with `upload`, `download`, `len`, `byte_len`, and `as_entire_binding`; tracks element count so callers never compute byte sizes manually.
- [x] Buffer pool / reuse — `BufferPool` keyed by `(size, BufferUsages)` that recycles freed buffers across dispatches to avoid per-frame reallocation.
- [x] Mapped buffer streaming — zero-copy upload path using `mapped_at_creation` (`mapped_storage_init`) for unified-memory (integrated) GPUs, selectable via `adapter_info()`.
- [x] Buffer suballocation — `SubAllocator` that bump-allocates aligned sub-regions from one large `wgpu::Buffer` and hands out `SubRegion` views with offset/size bookkeeping.
- [x] `read_back_async<T: Pod>(...)` — non-blocking readback that returns a future instead of polling with `PollType::wait_indefinitely()`.
- [x] Partial readback — `read_back_range<T>(device, queue, buf, byte_offset, len)` to copy a sub-range of a buffer rather than always starting at offset 0.

## Pipeline & Dispatch

- [x] `DispatchBuilder` — fluent builder that accumulates bind groups and dispatch dimensions and records-and-submits the compute pass in one call.
- [x] `ComputePass` abstraction — `DispatchBuilder::submit` encodes, begins, sets, dispatches, ends, and submits in a single call, returning a `DispatchResult` submission index.
- [x] Pipeline caching — `PipelineCache` keyed by a hash of the WGSL source plus entry-point name, returning a cloned `Arc<ComputePipeline>` on a hit to avoid recompilation across frames.
- [x] Workgroup-size helpers — `dispatch_1d`, `dispatch_2d`, `dispatch_3d` that compute ceil-div grid sizes from a per-axis workgroup size and clamp to `MAX_WORKGROUPS`.
- [x] Push constants (immediates) — `validate_immediates` / `supports_immediates` for the wgpu 29 `Features::IMMEDIATES` path (renamed from push constants); `IMMEDIATE_DATA_ALIGNMENT == 4` alignment enforced.
- [x] Indirect dispatch — `encode_indirect_dispatch(pass, buffer, offset)` via `ComputePass::dispatch_workgroups_indirect` for variable-size workloads.

## WGSL Utilities

- [x] WGSL preprocessor — `preprocess(source, resolver)` with `#include "path"`-style file stitching, cycle detection, and a depth cap so shaders can be assembled from modular fragments before compilation.
- [x] Shader hot-reload — watch WGSL source files (via `notify`) and recompile the affected pipelines without restarting the process; expose a `reload()` hook for editor integration.
- [x] Built-in shader library — ship validated WGSL kernels for prefix sum (scan), reduction (sum), histogram, and tiled matrix multiply: `SHADER_PREFIX_SUM`, `SHADER_REDUCTION_SUM`, `SHADER_HISTOGRAM`, `SHADER_MATMUL`.
- [x] WGSL validation errors — `validate(device, source)` captures `wgpu::CompilationInfo` from `get_compilation_info()` and surfaces `WgslDiagnostic` values with line and column numbers instead of panicking inside `create_compute_pipeline`.

## Integration

- [x] `oxiui-render-soft` integration — replace CPU paths for Gaussian blur, ordered dithering, and gradient fill with compute shaders, falling back to the CPU implementation when `ComputeContext::try_new()` returns `None`.
- [x] `oxiui-render-wgpu` integration — accept an externally owned `wgpu::Device` + `wgpu::Queue` so the compute layer shares the render backend's device instead of initialising a second one.
- [x] `oxiui-text` integration — add a GPU glyph-rasterization compute pass that writes coverage into an atlas buffer for upload to the text renderer.
- [x] COOLJAPAN ecosystem serialization — keep all CPU/GPU data interchange on `bytemuck` `Pod`/`Zeroable` types; no `bincode` or other serializers on the GPU data path.

## Testing & Benchmarks

- [x] Extract the `require_gpu!` macro from the per-module test blocks in `buffer.rs` / `pipeline.rs` into `oxiui-core` so every crate shares one CI-skip macro.
- [x] Golden-value tests — verify prefix-sum and reduction kernels against a CPU reference for a range of input lengths, including non-power-of-two sizes.
- [x] Benchmark — `read_back` throughput for 1 MB, 16 MB, and 256 MB buffers, reported as GiB/s.
- [x] Benchmark — pipeline compilation time, cold versus warm `PipelineCache`.
- [x] CI — skip GPU tests on headless runners via `require_gpu!`; run them on GPU-enabled runners gated behind an `OXIUI_GPU_TESTS=1` env flag.

## Error Handling & Observability

- [x] `ComputeError::OutOfMemory` variant — returned when device allocation fails or `wgpu::Error::OutOfMemory` is reported through the device error scope.
- [x] `ComputeError::ShaderCompilation(String)` variant — carries the WGSL diagnostic with line/column, replacing the panic in `compute_pipeline` (via `checked_compute_pipeline`).
- [x] Structured error context — `ComputeError::Operation { op, detail }` attaches the failing operation name and its parameters (buffer label, size, entry point) to errors.
- [x] Tracing spans — add `#[tracing::instrument]` to `ComputeContext::new`, `read_back`, and `compute_pipeline` for span-based profiling, behind an optional `tracing` feature.

## Documentation

- [x] Make the `lib.rs` quick-start example a runnable `rust` doc-test (not `rust,no_run`) by including a full dispatch and asserting the doubled result, skipping gracefully when no GPU is present.
- [x] Add WGSL examples under `examples/` — `prefix_sum.rs` and `matrix_mul.rs` — each demonstrating context setup, dispatch, and readback end to end.
- [x] Add `CONTRIBUTING.md` with GPU CI setup instructions (runner requirements, the `OXIUI_GPU_TESTS` flag, and how `require_gpu!` gates execution).
