# Contributing to oxiui-compute-wgpu

## Prerequisites

- Rust toolchain: see `rust-version` in the workspace `Cargo.toml` (currently 1.89+).
- A GPU-capable machine (Metal on macOS, Vulkan on Linux/Windows) for running
  GPU-gated tests.
- `cargo-nextest` for running the test suite: `cargo install cargo-nextest`.

## GPU CI setup

### How tests are gated

GPU tests use the `require_gpu!` macro from `oxiui-core`:

```rust
oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
// ↑ If no GPU is available, the test returns early (graceful skip).
```

Tests that do **not** use this macro must run on headless machines without
modification.

### Running GPU tests locally

```shell
# Run all tests including GPU-gated ones:
OXIUI_GPU_TESTS=1 cargo nextest run -p oxiui-compute-wgpu --all-features

# Run only CPU-safe tests (no GPU required):
cargo nextest run -p oxiui-compute-wgpu --all-features
```

`require_gpu!` checks `ComputeContext::try_new()`.  When no GPU adapter is
present the test body is skipped — the test passes rather than failing.

### CI runner requirements

| Runner type | GPU tests | Notes |
|-------------|-----------|-------|
| Headless (container/VM, no GPU) | Skipped | All `require_gpu!`-gated tests return early. |
| GPU-enabled runner | Run | Requires at least one wgpu-supported adapter (Metal, Vulkan, DX12, or GLES). |

To enable GPU tests on a CI runner, set `OXIUI_GPU_TESTS=1` in the job
environment.  Without this flag the test suite still runs; GPU tests are
simply skipped gracefully.

## Feature flags

| Feature | Purpose |
|---------|---------|
| `tracing` | Adds `#[tracing::instrument]` spans to `ComputeContext::new`, `read_back`, and `compute_pipeline` for span-based profiling. |
| `hot-reload` | Adds `hot_reload::ShaderWatcher` for live WGSL file watching via `notify`. |

## Running benchmarks

```shell
# Read-back throughput (1 MiB, 16 MiB, 256 MiB):
cargo bench -p oxiui-compute-wgpu --bench readback

# Pipeline compilation — cold vs warm PipelineCache:
cargo bench -p oxiui-compute-wgpu --bench pipeline_cache
```

Benchmark output is written to `target/criterion/`.

## Code style

- No `unwrap()` or `expect()` in production code — use `?` and error
  propagation.
- All source files must stay under 2 000 lines.
- GPU/CPU data interchange must use `bytemuck` `Pod`/`Zeroable` types only.
  No `bincode` or other serializers on the GPU data path.

## Commit guidelines

- Never commit directly to `main`; use version branches (`0.x.y`).
- Do not run `cargo publish` — releases are handled by the maintainers.
