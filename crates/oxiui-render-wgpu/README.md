# oxiui-render-wgpu — wgpu GPU render backend for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-render-wgpu.svg)](https://crates.io/crates/oxiui-render-wgpu)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-render-wgpu` is the **GPU render backend** for OxiUI, built on [`wgpu`]. It provides both the CPU-side preparation stack (texture atlas, draw-call batching, clip/scissor management, resource tracking) and a real headless GPU backend, [`WgpuBackend`], that initialises an offscreen device, compiles the `solid` / `gradient` / `textured` / `instanced` / `blur` / `blur_compute` / `composite` WGSL pipelines, rasterizes a [`oxiui_core::paint::DrawList`] (solid/gradient/textured fills, images, nine-slices, box shadows, backdrop blur, stencil-clipped and blended draws, tessellated fill/stroke paths), and reads pixels back to CPU memory.

`wgpu` is the **Rust graphics boundary** for this ecosystem: the crate itself is Rust and links no graphics C/C++ libraries at build time. At *runtime*, `wgpu` dispatches to the platform's GPU API — Vulkan, Metal, DX12, or WebGPU — which is provided by the operating system's installed drivers. [`gpu::WgpuBackend::headless`] acquires an adapter at runtime and gracefully returns [`UiError::Unsupported`] when no usable GPU is available, so headless CI on machines without a GPU can *skip* rather than fail. The crate contains a single `unsafe` block, in [`surface::SurfaceContext::from_raw_handles`], required to build a windowed swap-chain surface from raw window/display handles; every other module (including the entire headless `gpu` backend) is free of `unsafe`.

## Installation

```toml
[dependencies]
oxiui-render-wgpu = "0.2.3"
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
| `headless_with_quality` / `headless_with_sample_count` | Construct with an explicit `RenderQuality` preset / MSAA sample count |
| `set_clear_color(color)` / `clear_color()` | Set / get the per-frame clear colour |
| `width()` / `height()` | Target size in physical pixels |
| `resize(w, h)` | Recreate colour/MSAA textures in place; `UiError::Unsupported` on a zero dimension |
| `execute(&list)` | (`RenderBackend`) rasterize solid/gradient/textured fills, images, nine-slices, box shadows, backdrop blur, blended and stencil-clipped draws, tessellated paths |
| `frame_stats() -> FrameStats` | Draw-call / render-pass counters for the last `execute()` |
| `readback_rgba() -> Result<Vec<u8>, UiError>` | Read the offscreen colour texture back to CPU memory |
| `GpuContext` | Initialised device/queue + offscreen colour texture (`headless(w, h)`) |
| `SolidPipeline` / `GradientPipeline` / `TexturedPipeline` / `BlurPipeline` / `CompositePipeline` | Compiled `solid` / `gradient` / `textured` / `blur` / `composite` WGSL pipelines |
| `InstanceRect` / `InstancedRectPipeline` / `InstancedRectRenderer` | Instanced rounded-rect rendering path (one `draw_indexed` call per batch) |
| `ComputeBlurPipeline` | Compute-shader (16×16 workgroup) separable Gaussian blur, alternative to the fragment-shader blur path |
| `StencilTarget` / `StencilWritePipeline` / `StencilClipState` | `Depth24PlusStencil8` stencil-buffer clip regions |
| `HdrGpuContext` / `SurfaceColorFormat` / `select_surface_format()` | HDR / wide-gamut offscreen target (`Rgba16Float`) and format-selection heuristic |
| `FrameHistogram` / `FrameTimer` / `PresentModeRecommendation` | GPU-timestamp (falls back to CPU `Instant`) frame-time tracking and Fifo/Mailbox/Immediate present-mode heuristics |
| `RingBuffer` / `RingAllocation` / `RingBufferStats` | Streaming vertex/index ring buffer with next-power-of-two growth |
| `LayerCache` | LRU pool of off-screen `RenderTarget`s keyed by layer id, for cached subtrees |
| `RenderTarget` | Off-screen colour target with optional MSAA, `readback_rgba()`, `resize()` |
| `blend_state_for_mode()` / `BlendPipelineSet` | Maps `oxiui_core::paint::BlendMode` (Normal/Multiply/Screen/Overlay/Darken/Lighten/Copy/Destination) to a `wgpu::BlendState`; modes with no fixed-function equivalent fall back to Normal |
| `earcut::triangulate()` | Ear-clipping polygon triangulation for `FillPath` (`FillRule::NonZero` / `EvenOdd`, holes via bridging) |
| `Vertex` / `GradientVertex` / `TexVertex` / `Globals` | `#[repr(C)]` `Pod` vertex / uniform layouts |
| `TARGET_FORMAT` / `HDR_FORMAT` / `DEPTH_STENCIL_FORMAT` | Offscreen texture formats: `Rgba8Unorm` / `Rgba16Float` / `Depth24PlusStencil8` |

### `atlas` module

Dynamic shelf-based texture atlas with LRU eviction.

| Item | Description |
|------|-------------|
| `TextureAtlas` | Shelf bin-packing atlas (`width`, `height` public) |
| `TextureAtlas::new(w, h)` | Construct an empty atlas |
| `insert(w, h) -> Option<AtlasHandle>` | Allocate a region (evicts LRU on overflow) |
| `get(handle) -> Option<AtlasRect>` | Look up an allocation rectangle |
| `utilization() -> f32` | Fraction of atlas area in use |
| `allocation_count() -> usize` | Number of live allocations |
| `is_fragmented(threshold) -> bool` | Heuristic fragmentation check |
| `defrag() -> Vec<(AtlasHandle, AtlasRect)>` | Rebuild the layout in place, returning relocations |
| `defrag_if_fragmented(threshold)` | `defrag()` only when `is_fragmented(threshold)` |
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

### `surface` module — windowed swap-chain (real-time presentation)

| Item | Description |
|------|-------------|
| `SurfaceConfig` | Swap-chain configuration (size, present mode, format) |
| `SurfaceContext` | Wraps a `wgpu::Surface` built from raw window/display handles alongside its `Device`/`Queue`/config |
| `SurfaceContext::from_raw_handles(..)` | `unsafe` — the caller must guarantee the raw handles outlive the surface; this is the crate's only `unsafe` code |

### Feature-gated bridge modules

| Feature | Module | Provides |
|---------|--------|----------|
| `theme` | `theme_bridge` | Pure converters from `oxiui-theme` design tokens (`ShadowSpec`, `BorderSpec`/`BorderSpecs`, `ExtendedPalette`) to `DrawList` commands: `push_shadow_spec`, `push_border_spec(s)`, `push_theme_gradient`, `push_elevation_shadows`, and gradient-stop helpers (`primary_gradient_stops`, `surface_gradient_stops`, `status_gradient_stops`, `outline_gradient_stops`) |
| `accessibility` | `a11y_bridge` | `push_focus_ring(list, rect, &FocusRing)` renders a focus outline as a rounded stroke path; `is_high_contrast_active()` reads the OS high-contrast preference via `oxiui-accessibility`'s `OsA11yPrefs` |
| `text` | `text_bridge` | `TextBridge::expand_draw_text` shapes text via `oxiui-text::TextPipeline`, rasterizes glyphs through `GlyphAtlas`, and emits per-glyph `DrawCommand::Image` quads for the textured pipeline |
| `text` | `sdf_text` | `SdfTextPipeline` uploads `oxitext_sdf::SdfTile` glyph SDFs to an R8Unorm GPU atlas and renders them with a dedicated WGSL SDF shader for sharp text at any scale |

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

`default` is empty — the CPU preparation stack and the headless `gpu`/`surface` backends build with zero optional dependencies.

| Feature | Enables |
|---------|---------|
| `theme` | `theme_bridge` module; pulls in `oxiui-theme` |
| `accessibility` | `a11y_bridge` module; pulls in `oxiui-accessibility` |
| `text` | `text_bridge` + `sdf_text` modules and the `SdfTextPipeline`; pulls in `oxiui-text` and `oxitext-sdf` |

## Pure-Rust Status

The crate links no graphics C/C++/Fortran libraries at build time. GPU drivers (Vulkan / Metal / DX12 / WebGPU) are supplied by the operating system at runtime through `wgpu`, the Rust graphics boundary for the OxiUI ecosystem.

## Related Crates

- [`oxiui`](../../) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `RenderBackend`, `DrawList`, `DrawCommand`, `Color`, `UiError`, geometry types.
- [`oxiui-render-soft`](../oxiui-render-soft) — the Pure-Rust CPU software renderer (display-free reference path).
- [`oxiui-egui`](../oxiui-egui) — egui/eframe adapter that consumes this crate's wgpu render path.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
