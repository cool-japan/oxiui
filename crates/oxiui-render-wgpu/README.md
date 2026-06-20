# oxiui-render-wgpu — wgpu GPU render backend for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-render-wgpu.svg)](https://crates.io/crates/oxiui-render-wgpu)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-render-wgpu` is the **GPU render backend** for OxiUI, built on [`wgpu`]. It provides both the CPU-side preparation stack (texture atlas, draw-call batching, clip/scissor management, resource tracking) and a real headless GPU backend, [`WgpuBackend`], that initialises an offscreen device, compiles the `solid.wgsl` / `gradient.wgsl` pipelines, rasterizes a [`oxiui_core::paint::DrawList`], and reads pixels back to CPU memory.

`wgpu` is the **Rust graphics boundary** for this ecosystem: the crate itself is Rust and links no graphics C/C++ libraries at build time. At *runtime*, `wgpu` dispatches to the platform's GPU API — Vulkan, Metal, DX12, or WebGPU — which is provided by the operating system's installed drivers. [`gpu::WgpuBackend::headless`] acquires an adapter at runtime and gracefully returns [`UiError::Unsupported`] when no usable GPU is available, so headless CI on machines without a GPU can *skip* rather than fail. `#![forbid(unsafe_code)]` is enforced crate-wide.

## Installation

```toml
[dependencies]
oxiui-render-wgpu = "0.1.3"
```

## Quick Start

### CPU-side frame preparation

Batch a `DrawList` into pipeline-sorted draw batches without touching a GPU:

```rust
use oxiui_core::{geometry::Rect, paint::DrawList, Color};
use oxiui_render_wgpu::{WgpuPrep, RenderQuality, ClipRect};

let mut prep = WgpuPrep::new(512, RenderQuality::balanced());
prep.clip.push(ClipRect::new(0.0, 0.0, 100.0, 100.0));

let mut list = DrawList::new();
list.push_rect(Rect::new(10.0, 10.0, 20.0, 20.0), Color(255, 0, 0, 255));

let frame = prep.prepare(&list);
assert_eq!(frame.batches.len(), 1);
assert_eq!(frame.culled_count, 0);
```

### Headless GPU rendering (runtime adapter required)

```rust,no_run
use oxiui_core::{paint::{DrawList, RenderBackend}, geometry::Rect, Color, UiError};
use oxiui_render_wgpu::WgpuBackend;

# fn main() -> Result<(), UiError> {
// Returns UiError::Unsupported if no GPU adapter is available — skip-friendly.
let mut backend = WgpuBackend::headless(256, 256)?;
backend.set_clear_color(Color(26, 27, 38, 255));

let mut list = DrawList::new();
list.push_rect(Rect::new(32.0, 32.0, 64.0, 64.0), Color(255, 0, 0, 255));
backend.execute(&list)?;

let rgba = backend.readback_rgba()?; // CPU pixel readback
assert_eq!(rgba.len(), 256 * 256 * 4);
# Ok(())
# }
```

## API Overview

### `WgpuPrep` (crate root)

The CPU-side preparation state for the wgpu pipeline.

| Item | Description |
|------|-------------|
| `WgpuPrep` | Owns a `TextureAtlas`, a `ClipStack`, and a `RenderQuality` (public fields `atlas`, `clip`, `quality`) |
| `WgpuPrep::new(atlas_size, quality)` | Construct with a square atlas and quality preset |
| `prepare(&list) -> PreparedFrame` | Batch a `DrawList`, forwarding the active clip as the cull region |
| `WgpuRenderer` | Legacy placeholder kept for binary compatibility; prefer `WgpuPrep` |

### `gpu` module — real headless GPU backend

| Item | Description |
|------|-------------|
| `WgpuBackend` | Headless GPU [`RenderBackend`] over an offscreen target |
| `WgpuBackend::headless(w, h)` | Initialise device + offscreen target; `UiError::Unsupported` if no GPU |
| `set_clear_color(color)` / `clear_color()` | Set / get the per-frame clear colour |
| `width()` / `height()` | Target size in physical pixels |
| `execute(&list)` | (`RenderBackend`) rasterize solid rects, SDF circles, scissor clips |
| `readback_rgba() -> Result<Vec<u8>, UiError>` | Read the offscreen colour texture back to CPU memory |
| `GpuContext` | Initialised device/queue + offscreen colour texture (`headless(w, h)`) |
| `SolidPipeline` / `GradientPipeline` | Compiled `solid.wgsl` / `gradient.wgsl` pipelines |
| `Vertex` / `GradientVertex` / `Globals` | `#[repr(C)]` `Pod` vertex / uniform layouts |
| `TARGET_FORMAT` | The non-sRGB offscreen format (`wgpu::TextureFormat::Rgba8Unorm`) |

### `atlas` module

Dynamic shelf-based texture atlas with LRU eviction.

| Item | Description |
|------|-------------|
| `TextureAtlas` | Shelf bin-packing atlas (`width`, `height` public) |
| `TextureAtlas::new(w, h)` | Construct an empty atlas |
| `insert(w, h) -> Option<AtlasHandle>` | Allocate a region (evicts LRU on overflow) |
| `get(handle) -> Option<AtlasRect>` | Look up an allocation rectangle |
| `utilization() -> f32` | Fraction of atlas area in use |
| `resize(new_w, new_h)` | Resize (invalidates handles) |
| `AtlasRect` | Allocated rectangle (`x`, `y`, `w`, `h`) |
| `AtlasHandle` | Generation-based opaque allocation handle |

### `batch` module

Draw-call batcher grouping commands by pipeline state.

| Item | Description |
|------|-------------|
| `batch(&list, active_clip) -> PreparedFrame` | Classify, cull, stable-sort, and merge draw commands |
| `PreparedFrame` | Output: `batches: Vec<DrawBatch>`, `culled_count: usize` |
| `DrawBatch` | A run sharing a `BatchKey` (`key`, `command_range`, `instance_count`) |
| `BatchKey` | Draw-call boundary state (`texture_id`, `pipeline`, `blend`) |
| `PipelineKind` | `SolidColor`, `Textured`, `Gradient`, `Path` |
| `BlendMode` | `Normal`, `Multiply`, `Screen`, `Overlay` |

### `clip` module

| Item | Description |
|------|-------------|
| `ClipRect` | Floating-point clip rect; `new(x, y, w, h)`, `intersect` |
| `ClipStack` | Nested clip stack; `new`, `push`, `pop`, `current`, `as_scissor() -> Option<[u32; 4]>` |

### `resource` module

Generation-checked GPU resource handles with reference counting.

| Item | Description |
|------|-------------|
| `ResourceRegistry` | Ref-counted registry with slot recycling (`new`, `alloc_texture`/`alloc_shader`, `retain_*`, `release_*`, `get_*`) |
| `ResourceId` | Generation-checked id (`gen`, `idx`) |
| `TextureHandle` / `ShaderHandle` | Opaque generation-checked handle newtypes |
| `TextureGuard` / `ShaderGuard` | RAII guards that decrement the ref-count on drop (`new(handle, registry)`) |

### `quality` module

| Item | Description |
|------|-------------|
| `RenderQuality` | Aggregated settings (`msaa`, `shadow`, `text`); presets `low()`, `balanced()`, `high()` |
| `ShadowQuality` | `Off`, `Low`, `High` |
| `TextQuality` | `Grayscale`, `Subpixel`, `Sdf` |

### `error` module

| Item | Description |
|------|-------------|
| `GpuErrorKind` | GPU error class (see below) |
| `map_gpu_error(kind, detail) -> UiError` | Normalise a GPU error into [`oxiui_core::UiError`] |

## Error Mapping

`GpuErrorKind` classifies hardware errors; `map_gpu_error` maps them onto [`oxiui_core::UiError`]:

| `GpuErrorKind` | Description | Maps to |
|----------------|-------------|---------|
| `DeviceLost` | Driver crash, unplug, or TDR | `UiError::Render` |
| `OutOfMemory` | GPU ran out of memory | `UiError::Render` |
| `SurfaceLost` | Swap-chain surface lost (window closed, resize race) | `UiError::Render` |
| `ShaderCompile` | A shader module failed to compile | `UiError::Unsupported` |

`WgpuBackend::headless` returns `UiError::Unsupported` when no adapter is available and `UiError::Backend` when device creation fails.

## Feature Flags

This crate exposes no Cargo features; `default` is empty.

## Pure-Rust Status

The crate links no graphics C/C++/Fortran libraries at build time. GPU drivers (Vulkan / Metal / DX12 / WebGPU) are supplied by the operating system at runtime through `wgpu`, the Rust graphics boundary for the OxiUI ecosystem.

## Related Crates

- [`oxiui`](../../) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `RenderBackend`, `DrawList`, `DrawCommand`, `Color`, `UiError`, geometry types.
- [`oxiui-render-soft`](../oxiui-render-soft) — the Pure-Rust CPU software renderer (display-free reference path).
- [`oxiui-egui`](../oxiui-egui) — egui/eframe adapter that consumes this crate's wgpu render path.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
