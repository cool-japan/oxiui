# oxiui-render-wgpu TODO

## Status
Real headless GPU foundation landed (2026-05-30). On top of the CPU preparation engine (atlas/batch/clip/quality/resource/error), the crate now has a working `WgpuBackend` (`src/gpu/`) that implements `oxiui-core`'s `RenderBackend` on an offscreen wgpu target: solid `FillRect`, SDF `FillCircle`, and `PushClip`/`PopClip` scissor clipping, with byte-exact CPU pixel readback. Initialised headlessly (no window/surface); gracefully reports `UiError::Unsupported` when no GPU adapter is present. wgpu is pinned to 29.0.3 to unify with eframe's existing wgpu (no duplicate build). The legacy `WgpuRenderer` PhantomData stub is retained for compatibility. Still to come for full framework independence: textured/gradient/shadow/text fragment variants, MSAA, instancing, and a window-owning swap-chain surface + present-mode selection.

## Core Implementation
- [x] wgpu device/queue initialization (done 2026-05-30): `WgpuRenderer::init(window_handle)` acquiring adapter, device, queue, surface configuration, present mode selection (Fifo/Mailbox/Immediate) (~200 SLOC) (planned 2026-05-30)
  - **Landed:** `WgpuBackend::headless(w,h)` in `src/gpu/device.rs` — `Instance::default()` → `request_adapter` (via `pollster::block_on`; `Err`→`UiError::Unsupported` so callers skip) → `request_device`/`queue` → offscreen `Rgba8Unorm` (linear, byte-exact readback) texture with `RENDER_ATTACHMENT | COPY_SRC`. No window/surface (present-mode selection deferred to the window-owning milestone). wgpu pinned to 29.0.3 (unifies with eframe's wgpu, no duplicate build).
  - **Goal:** real (non-stub) GPU backend implementing oxiui-core RenderBackend for FillRect/FillCircle/clip; headless offscreen texture (no window); pixel-readback tests that actually run on this Mac's Metal backend (graceful skip if no adapter).
  - **Design:** add workspace deps wgpu/pollster/bytemuck (latest, pure Rust). WgpuBackend::headless(w,h): Instance→request_adapter (pollster::block_on)→device/queue→offscreen Rgba8UnormSrgb target. execute(&DrawList) replays FillRect, FillCircle (SDF in fragment), PushClip/PopClip→set_scissor_rect via the existing tested ClipStack. solid.wgsl: vertex [pos vec2, color vec4] + 2D ortho, fragment solid fill + circle SDF. readback_rgba() (texture→buffer→map) for tests. supports_text/images/blur/gradients=false this slice.
  - **Files:** crates/oxiui-render-wgpu/src/gpu/{device,renderer,pipeline,buffer}.rs, src/shaders/solid.wgsl, tests/headless_render_tests.rs; src/lib.rs exports the gpu module; Cargo.toml + root [workspace.dependencies] gain wgpu/pollster/bytemuck. Each file <2000 lines.
  - **Tests:** offscreen render → readback → assert RGBA at known coords (red rect @(10,10); circle center; clipped region unaffected). Adapter guard: `if adapter.is_none() { return }` so no-GPU CI passes cleanly; on this darwin/Metal host the tests must ACTUALLY execute — report ran-vs-skipped.
  - **Risk:** GPU availability (mitigated by skip + Metal here), async device init (pollster::block_on), vertex alignment (bytemuck::Pod + a size assertion), wgpu version drift (reconcile to one version if eframe/iced in the workspace pull a different wgpu). std-only. No unwrap in production paths.
- [x] Render pipeline setup (done 2026-05-30): vertex/fragment shader module creation, pipeline layout with bind groups, vertex buffer layout (position, UV, color), render pass configuration (~250 SLOC) — (see wgpu foundation plan above)
  - **Landed:** `src/gpu/pipeline.rs` (`SolidPipeline`) — `solid.wgsl` module, uniform bind-group layout (viewport `Globals`), pipeline layout, alpha-blended `RenderPipeline`, hand-derived vertex attribute layout matching the `Vertex` offsets; `src/gpu/buffer.rs` defines the `#[repr(C)]` `Pod`/`Zeroable` `Vertex`/`Globals` with compile-time size asserts. Textured/UV variants deferred (this slice is solid + circle SDF only).
- [x] WGSL shader for UI primitives (done 2026-05-30): vertex shader (2D transform + projection), fragment shader (solid color, textured, rounded-rect SDF, gradient), preprocessor-like variant selection (~300 SLOC) — (see wgpu foundation plan above)
  - **Landed:** `src/shaders/solid.wgsl` — vertex stage applies a 2-D orthographic projection (pixel coords → NDC); fragment emits solid colour for rects and a `smoothstep` circle-SDF coverage for circles, switched on a per-vertex `kind` discriminator. Textured/rounded-rect/gradient fragment variants remain deferred to later slices.
- [ ] Rounded rectangle SDF rendering: signed-distance-field fragment shader for anti-aliased rounded rectangles with per-corner radius, border width, border color (~100 SLOC)
- [x] **wgpu CPU foundations: texture atlas, draw-call batcher, clip stack, quality, resource handles, error mapping** (completed 2026-05-29)
  - **Goal:** turn the 32-line PhantomData stub into the GPU backend's full CPU-side preparation engine — everything a future GPU `RenderBackend` impl will compose, built and unit-tested with no GPU and no `wgpu` dependency.
  - **Design:** new modules in `crates/oxiui-render-wgpu/src/`: `atlas.rs` (shelf+skyline bin-packer, LRU eviction, utilization metric), `batch.rs` (DrawList batcher: BatchKey sort + merge into DrawBatch + visibility culling), `clip.rs` (nested clip-rect stack with intersection, integer-rect scissor), `quality.rs` (RenderQuality{msaa,shadow,text} enums + Low/Balanced/High presets), `resource.rs` (TextureHandle/ShaderHandle newtypes + Rc refcount ResourceRegistry + RAII Drop), `error.rs` (GpuErrorKind{DeviceLost,OOM,ShaderCompile,SurfaceLost} + map_gpu_error→UiError). `lib.rs` ties all together with WgpuPrep::prepare(&DrawList)->PreparedFrame. No RenderBackend impl yet (GPU tail). No new deps.
  - **Files:** new `src/{atlas.rs,batch.rs,clip.rs,quality.rs,resource.rs,error.rs}`; rewrite stub `src/lib.rs`. No Cargo.toml changes.
  - **Prerequisites:** none (oxiui_core::paint::{DrawList,DrawCommand,Rect} exist).
  - **Tests (~16):** atlas packs 100 random rects no-overlap + util>70%; LRU eviction invariants; batcher 1000 rects/5 textures→≤5 batches; stable order; clip push/pop/intersection; underflow saturates; culling drops off-screen; quality presets; resource refcount+RAII; map_gpu_error covers all; prepare(empty) is noop.
  - **Risk:** self-contained (no GPU, no external API). Batcher sort/merge is the subtle part — test order-stability. Defer all GPU-runtime items.
- [ ] Draw call batching: sort draw commands by texture/shader/blend-state, merge adjacent draws with the same state into single `draw_indexed` calls, reduce GPU state changes (~200 SLOC)
- [ ] Instanced rendering: instance buffer for repeated primitives (buttons, list items, table cells) sharing the same mesh but different transforms/colors (~150 SLOC)
- [ ] Anti-aliasing: MSAA (4x/8x configurable), edge-AA via SDF smoothstep in fragment shader, supersampling option for text (~100 SLOC)
- [x] Scissor/clip rectangles (done 2026-05-30): nested clip region stack, `set_scissor_rect` per draw call, clip region intersection for nested scroll areas (~80 SLOC) — (see wgpu foundation plan above)
  - **Landed:** `WgpuBackend::execute` segments the `DrawList` into per-clip draw runs by walking the existing tested `ClipStack` (intersection-on-push), then issues `set_scissor_rect` per segment before drawing its vertex range; out-of-bounds scissors are clamped and degenerate (fully off-screen) clips draw nothing. Verified by `clip_restricts_fill_to_clip_rect` and `nested_clip_intersects` headless tests.
- [ ] Shadow rendering: box shadow via multi-pass Gaussian blur (separable 2-pass), drop shadow offset, inset shadow, shadow spread, cached shadow textures for static widgets (~200 SLOC)
- [ ] Blur effects: backdrop-blur (frosted glass) via off-screen render target + Gaussian blur pass, blur radius parameter (~150 SLOC)
- [ ] Gradient rendering: linear gradient (angle + color stops), radial gradient (center + radius + color stops), conic/sweep gradient, gradient texture generation on GPU (~200 SLOC)
- [ ] Image rendering: load image data to GPU texture, aspect-ratio-preserving scaling (contain/cover/fill/none), nine-slice scaling for stretchable UI chrome (~150 SLOC)
- [ ] SDF text rendering: signed-distance-field glyph rendering for resolution-independent text, multi-channel SDF (MSDF) for sharp corners, subpixel positioning (~250 SLOC)
- [ ] Stencil buffer operations: stencil-based clipping for non-rectangular clip paths (rounded corners, arbitrary shapes) (~80 SLOC)
- [ ] Off-screen render targets: render-to-texture for cached widget subtrees, compositing cached layers, invalidation on content change (~150 SLOC)
- [ ] GPU-driven bezier curve rendering: quadratic and cubic bezier fill/stroke via tessellation or SDF, path rendering for icons and custom shapes (~200 SLOC)
- [ ] Blend modes: alpha compositing (source-over, multiply, screen, overlay, darken, lighten), configurable per-layer blend state (~80 SLOC)
- [ ] HDR and wide-gamut support: surface format selection (Rgba8Unorm vs Rgba16Float), color space handling (sRGB transfer function in shader) (~80 SLOC)
- [ ] Frame pacing: track GPU frame timing, adaptive present mode, frame time histogram for performance monitoring (~80 SLOC)
- [ ] Resize handling: recreate swap chain on window resize, handle DPI changes, logical-to-physical coordinate mapping (~60 SLOC)

## API Improvements
- [x] `RenderBackend` trait implementation (done 2026-05-30): implement the trait defined in `oxiui-core` for GPU rendering — (see wgpu foundation plan above)
  - **Landed:** `impl RenderBackend for WgpuBackend` in `src/gpu/renderer.rs` — `execute(&DrawList)` clears the target then replays `FillRect`/`FillCircle`/`PushClip`/`PopClip` in a single render pass (clip-leak-free); `surface_size()` returns the target size; `supports_text/images/blur/gradients/paths()` all return `false` this slice (TODO notes the deferred work). `readback_rgba()` copies texture→buffer, maps, and strips the 256-byte `COPY_BYTES_PER_ROW_ALIGNMENT` row padding to a tightly packed `w*h*4` buffer. 9 headless Metal tests pass with byte-exact pixel assertions.
- [ ] Command buffer API: `DrawList` builder with `push_rect()`, `push_text()`, `push_image()`, `push_clip()`, `push_shadow()` methods
- [ ] Configurable quality settings: `RenderQuality { msaa: u32, shadow_quality: ShadowQuality, text_quality: TextQuality }` enum
- [ ] Resource handle API: `TextureHandle`, `ShaderHandle` with RAII cleanup, reference counting for shared resources
- [ ] Error handling: GPU-specific error variants (device lost, out-of-memory, shader compilation failure) mapped to `UiError`

## Testing
- [ ] Device initialization test (headless via wgpu's `Backends::GL` or `Backends::VULKAN` with headless surface) (~60 SLOC)
- [ ] Render pipeline creation test: verify pipeline compiles without errors (~40 SLOC)
- [ ] Shader compilation tests: each WGSL shader variant compiles without diagnostics (~60 SLOC)
- [ ] Texture atlas packing tests: pack 100 random-size rects, verify no overlap, verify atlas utilization > 70% (~80 SLOC)
- [ ] Draw batching tests: 1000 rects with 5 different textures → verify batch count <= 5 (~40 SLOC)
- [ ] Scissor rect tests: nested clips produce correct intersection rects (~40 SLOC)
- [ ] Snapshot / golden-image tests: render reference scenes, compare against saved PNG baselines (pixel-diff threshold) (~150 SLOC)
- [ ] Resize handling tests: resize from 800x600 to 1920x1080, verify surface reconfigured without panic (~30 SLOC)
- [ ] Benchmark: render 10,000 rounded rectangles, measure frame time and draw call count

## Performance
- [ ] Draw call batching: target < 100 draw calls for a typical 500-widget UI
- [ ] Texture atlas fragmentation monitoring: log atlas utilization, trigger defrag/rebuild when utilization drops below 50%
- [ ] GPU upload streaming: ring buffer for vertex/index data, avoid per-frame buffer creation
- [ ] Render layer caching: cache rendered subtrees as textures, only re-render when dirty
- [ ] Compute shader for blur: use compute pipeline for Gaussian blur instead of fragment-shader multi-pass (better occupancy)
- [ ] Visibility culling: skip draw commands for widgets outside the viewport clip rect before submitting to GPU
- [ ] Pipeline state caching: avoid redundant `set_pipeline`/`set_bind_group` calls when state is unchanged between draws

## Integration
- [ ] `oxiui-core` integration: implement `RenderBackend` trait; accept `DrawList` from layout/paint phase
- [ ] `oxiui-text` integration: receive glyph atlas textures from `TextPipeline`, upload to GPU texture atlas, render SDF text
- [ ] `oxiui-theme` integration: consume `ShadowSpec`, `BorderSpec`, design tokens for rendering parameters
- [ ] `oxiui-render-soft` integration: shared `DrawList` command format so both backends accept the same paint commands
- [ ] `oxiui-web` integration: wgpu WebGPU backend for browser rendering, shared shader code
- [ ] `oxiui-accessibility` integration: ensure render output matches a11y tree structure (focus ring rendering, high-contrast mode)
- [ ] COOLJAPAN ecosystem: all shaders in WGSL (Pure Rust toolchain, no GLSL/HLSL/SPIR-V external compilers); texture compression via Pure Rust decoders (no stb_image); blur/FFT convolution via OxiFFT if applicable; asset bundling via oxiarc-* (no zip/flate2)

## Proposed follow-ups
- Deferred: GPU device/pipeline/WGSL/SDF/MSAA/shadow/blur/gradient/image/stencil/offscreen/instanced — GPU-hardware-blocked. CPU foundations landed in Slice A.
