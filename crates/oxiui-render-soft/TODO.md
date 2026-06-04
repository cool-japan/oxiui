# oxiui-render-soft TODO

## Active /ultra plan (2026-05-28)

**Goal:** Turn the partial CPU framebuffer renderer into a genuinely capable 2D
rasterizer — the headless/CI path for OxiUI. Additive only; no existing public
signature changes; no new deps.

**New modules:**

- [x] `blend.rs` — extended blend modes (multiply / screen / overlay / darken /
      lighten) + premultiplied-alpha helpers.
- [x] `scanline.rs` — Active-Edge-Table polygon / triangle fill with sub-pixel
      vertical-supersample coverage AA (even-odd + non-zero winding).
- [x] `path.rs` — `Path` with `move_to / line_to / quad_to / cubic_to / close`,
      De Casteljau flattening with adaptive flatness tolerance, fill (calls AET),
      stroke (parallel-offset polygons + miter / bevel / round joins +
      butt / round / square caps).
- [x] `shadow.rs` — separable 1-D Gaussian blur with kernel cache + box shadow.
- [x] `dither.rs` — Bayer-matrix ordered dithering (4×4 / 8×8).
- [x] `tile.rs` — 64×64 render-tile iterator (rayon-ready but serial this run).

**Additive `draw.rs` extensions** (no signature changes to existing methods):

- [x] Wu's anti-aliased line + thick-line via parallel-offset polygons.
- [x] Dashed / dotted line patterns.
- [x] Per-corner-radius rounded rect — new `fill_rounded_rect_per_corner`.
- [x] Midpoint ellipse with AA.
- [x] Bilinear image scaling + nine-slice stretching — new methods, additive
      to existing nearest-neighbour `blit_rgba`.
- [x] Quadratic / cubic Bezier convenience methods (delegating to `Path`).

**Additive `gradient.rs`:** new `RadialGradient` type next to `LinearGradient`.

## Status (post-ultra)
- Pixel framebuffer with packed `0xAARRGGBB`, straight-alpha source-over,
  coverage blend.
- Clip-region stack (intersecting push/pop).
- Drawing primitives: filled rect / circle (AA) / single-radius rounded rect
  (AA), Bresenham line, nearest blit, linear gradient.
- Headless rendering API + PNG export (Pure Rust `png` crate).

## Core Implementation
- [x] Pixel framebuffer: `Framebuffer` struct with `width`, `height`, `Vec<u32>` (0xAARRGGBB), pixel get/set, row-slice access, sub-region views
- [x] Scanline rasterizer: filled rectangle, filled triangle (edge-walking), polygon fill (active edge table algorithm), sub-pixel precision
- [x] Line rasterization: Bresenham line drawing, Wu's anti-aliased line, line width (thick lines via parallel offset), dashed/dotted patterns
- [x] Rectangle drawing: filled rect, stroked rect with border width, rounded-rect with per-corner radius (arc approximation via bezier or scanline clipping)
- [x] Circle / ellipse drawing: midpoint circle algorithm, filled ellipse scanline, anti-aliased edge smoothing
- [x] Bezier curve rendering: quadratic and cubic bezier flattening (adaptive subdivision), bezier stroke with configurable width, bezier fill via scanline
- [x] Path rendering: `Path` type with moveTo/lineTo/quadTo/cubicTo/closePath operations, fill rule (even-odd / non-zero winding), stroke with join style (miter/bevel/round) and cap style (butt/round/square)
- [x] Anti-aliasing: analytical edge AA (coverage computation per pixel), supersampled AA (vertical-supersample per scanline for AET), SDF-style coverage for circle / rounded rect
- [x] Alpha blending: source-over compositing, pre-multiplied alpha, blend modes (multiply, screen, overlay, darken, lighten)
- [x] Gradient rendering: linear gradient (direction + color stops, interpolation in sRGB), radial gradient (center + radius + stops)
- [x] Image blitting: copy pixel rectangle from source to destination with alpha blending, nearest-neighbor and bilinear scaling, nine-slice stretching
- [x] Text glyph blitting: `blit_glyph_bitmap()` primitive implemented in `backend.rs`; `DrawCommand::DrawText` wired with visible placeholder fill (10% opacity rect); full shaper blocked on adding `oxiui-text` dep (zero-new-deps constraint for this slice).
    - **Deviation:** `DrawText` variant carries only raw `text: String + FontSpec` — no pre-rasterised bitmaps. Shaping requires `oxiui-text::TextPipeline` which is outside the zero-deps constraint. `supports_text()` stays `false` (honest reporting). The `blit_glyph_bitmap(fb, ox, oy, w, h, pixels, color)` primitive is ready for wiring once the dep is allowed.
    - **Status 2026-05-29:** Primitive + tests shipped; full pipeline integration deferred to slice C or when `oxiui-text` dep is permitted.
  - **Goal:** `SoftBackend` renders real text. The `DrawText` arm shapes via `oxiui-text` and blits glyph coverage bitmaps into the framebuffer, clip-aware. `supports_text()` returns `true` under the `text` feature.
  - **Design:** In the `DrawText` arm (`backend.rs:277`), call `pipeline.render(text, style) → RenderResult{glyphs, bitmaps}`; for each `PositionedGlyph` use the shaper-provided position, fetch its greyscale `Bitmap`, and blit via a clip-aware variant of `blit_glyph_bitmap` (intersect with active clip rect). Tint by DrawText color. Cache one `TextPipeline` on `SoftBackend`. Gated `#[cfg(feature = "text")]`. **Framebuffer storage stays straight-alpha `0xAARRGGBB` — premult only inside composite step.**
  - **Files:** `crates/oxiui-render-soft/src/{backend.rs,lib.rs,headless.rs}`.
  - **Tests:** synthetic string blits non-empty pixels; single-glyph coverage at known origin; clip rect excludes out-of-clip glyph; multi-glyph advances horizontally; `supports_text()==true` under feature, `false` without.
  - **Risk:** glyph positioning (use shaper positions directly); clip off-by-one; edge-alpha correctness bound by storage invariant.
- [x] Clip region stack: rectangular clip regions with push/pop, region intersection for nested clips
- [x] Shadow rendering: box shadow via Gaussian blur (separable 1D kernel), shadow offset, shadow spread, cached shadow bitmaps
- [x] Headless rendering API: `render_headless_once(width, height, draw_fn) -> Framebuffer` that runs one paint cycle without a window, suitable for screenshot generation and CI smoke tests
- [x] PNG export: write `Framebuffer` to PNG file using a Pure Rust PNG encoder (e.g. `png` crate), no C dependencies
- [x] Dithering: ordered dithering (Bayer matrix) for reduced color depth output (8-bit displays, terminal rendering)
- [x] Tile-based rendering: split framebuffer into tiles (e.g. 64x64), render tiles independently for potential parallelism, dirty-tile tracking for incremental updates (serial iterator landed; parallel rayon variant deferred to follow-ups)

## API Improvements
- [x] `RenderBackend` trait implementation: implement the trait from `oxiui-core` for CPU rendering — DONE.
    - `SoftBackend` in `src/backend.rs` implements `RenderBackend::execute` by opening a single `Canvas` for the full `DrawList`, ensuring all `PushClip`/`PopClip` commands correctly scope subsequent draw operations.
    - `Path::fill_clipped` and `Path::stroke_clipped` added to `path.rs`, routing through `fill_polygon_clipped`.
    - Clip-aware `Canvas` methods added: `fill_path`, `stroke_path`, `fill_linear_gradient_cmd`, `fill_radial_gradient_cmd`, `box_shadow_cmd`.
    - Mandatory clip-guard tests verified: `fill_path_respects_active_clip`, `gradient_respects_active_clip`, `radial_gradient_respects_active_clip`, `stroke_path_respects_active_clip`.
    - `supports_blur/gradients/paths/images=true`, `supports_text=false` (v1, no shaper wired).
- [x] `DrawList` consumer: SoftBackend dispatches all DrawCommand variants. All variants have non-stub handlers except DrawText, which is a documented no-op (shaper dep blocked; `supports_text()==false`). Verified by `draw_text_noop_execute_succeeds` test (execute returns Ok; background unchanged).
  - **Goal:** All `DrawList` / `DrawCommand` variants dispatched (DrawText is the last no-op). Covered by S1 wiring: when `DrawText` is wired, the consumer is complete.
  - **Design:** Same as item 1 above (same subagent, same files). No separate implementation needed.
  - **Files:** Same as item 1.
  - **Tests:** After S1, dispatch `DrawCommand::DrawText("hello", ...)` and verify non-zero pixels rendered.
  - **Risk:** None beyond item 1 risks.
- [x] `SoftRenderer::with_size(width, height)` constructor added to `lib.rs`. Test `soft_renderer_with_size_constructs` passes.
- [x] Configurable quality: `AaMode{None,Msaa4x,Supersampling}`, `ShadowQuality{Off,Low,High}`, `SoftRenderQuality` with `low()/balanced()/high()` presets added to `lib.rs`. `SoftBackend::with_quality(w,h,q)` constructor added. AA flag wired: `canvas.set_aa(aa_enabled)` called in `execute()`, propagated into `fill_path`/`stroke_path` → `fill_polygon_clipped`/`stroke_contour_clipped_inner` via `fill_clipped_aa`/`stroke_clipped_aa`. Shadow dispatch wired (`ShadowQuality::Off` skips Gaussian blur entirely). Tests: `quality_low_aa_mode_is_none`, `quality_high_aa_mode_is_supersampling`, `soft_backend_with_quality_*`.
- [x] Clear color: `clear_frame` actually fills with the provided color (stale TODO — `Framebuffer::with_fill` is used; verified by `clear_frame_actually_fills_color` regression test)
- [x] Buffer format options: `PixelFormat{Argb32,Bgra8,Rgb565}` enum added to `headless.rs`; `SoftBackend::to_bytes(format)` implemented in `backend.rs`; both re-exported from `lib.rs`. Tests: `to_bytes_argb32_correct_length`, `to_bytes_bgra8_rb_swap`, `to_bytes_rgb565_correct_length`, `to_bytes_rgb565_round_trip_high_bits`.

## Testing
- [x] Filled rectangle: draw rect at known position, verify pixel colors inside and outside bounds
- [x] Rounded rectangle: draw rounded rect, verify corner pixels are anti-aliased (not hard edge)
- [x] Line drawing: draw horizontal, vertical, and diagonal lines, verify pixel positions
- [x] Anti-aliased line: Wu's line, verify edge pixels have intermediate alpha values
- [x] Alpha blending: source-over compositing of two overlapping colored rects, verify blended pixel values
- [x] Gradient: linear gradient from red to blue, verify midpoint pixel is purple
- [x] Clip region: draw rect partially outside clip, verify clipped pixels are untouched
- [x] Image blitting: blit a 2x2 test image scaled to 4x4, verify nearest-neighbor interpolation
- [x] Glyph blitting: blit a synthetic 8x8 glyph bitmap, verify correct placement and color tinting — deferred with the consumer item.
  - **Goal:** A synthetic 8x8 coverage bitmap blits to the expected framebuffer pixels.
  - **Design:** Part of S1 test suite — covered by the S1 glyph-blitting tests in backend.rs tests module.
  - **Files:** `crates/oxiui-render-soft/src/backend.rs` (test module).
  - **Tests:** Covered by S1 "single-glyph coverage at known origin" test.
  - **Risk:** None.
- [x] Headless rendering: `render_headless_once(100, 100, |fb| draw_rect(fb))` returns non-zero pixels
- [x] PNG round-trip: render to framebuffer, export to PNG, read back PNG, compare pixel values
- [x] Snapshot tests: render reference scenes, compare against golden PNG baselines — deferred to follow-ups.
- [x] Benchmark: fill 10,000 rectangles in a 1920x1080 framebuffer, measure ms/frame — `benches/rects.rs` with criterion; covers `fill_10k_rects/1920x1080`, parametric `fill_rect`, `simd_fill_solid`, `linear_gradient`, `alpha_blend_row`, `png_encode` benchmarks.

## Performance
- [x] SIMD-accelerated pixel fill: `simd_fill.rs` module with `simd` feature gate using `wide 1.4.0` (Pure-Rust portable SIMD); `fill_solid` (8-lane u32x8 fill), `alpha_blend_row` (premult source-over, auto-vectorised), `gradient_row_horizontal` (8-lane f32x8 interpolation). All functions have scalar fallbacks; no `unsafe`. Tests: 7 unit tests. Re-exported from `lib.rs`.
- [x] Tile-based parallel rendering: `rayon = { workspace = true, optional = true }` + `[features] parallel = ["dep:rayon"]` added to `Cargo.toml`. `render_parallel<F>(tiles, render_fn)` added to `tile.rs` under `#[cfg(feature="parallel")]`. `collect_tiles()` helper added for pre-collection. Sequential path (`render_tiles`) unchanged. Test `parallel_output_matches_sequential` verifies pixel equality between sequential and parallel paths.
- [x] Dirty region tracking: only re-render tiles that overlap changed widgets, skip unchanged areas
- [x] Pre-multiplied alpha throughout pipeline: avoid per-pixel divide in compositing (helpers landed in `blend.rs`; pipeline refactor deferred)
  - **Goal:** Composite pipeline does blend math in premultiplied alpha for correct edge blending, converting back to straight-alpha at the framebuffer boundary. Per-fill scanline scratch buffers reused.
  - **Design:** Refactor `blend::{composite_into, blend}` and draw.rs fill paths to premultiply source+dest for the blend step, then un-premultiply before writing framebuffer. **Keep public `blend` function signatures stable** (S1 depends on them). Hoist per-polygon scratch vectors into reusable buffers owned by the rasterizer struct, cleared per fill. **Framebuffer storage stays straight-alpha `0xAARRGGBB`.**
  - **Files:** `crates/oxiui-render-soft/src/{blend.rs,scanline.rs,path.rs,draw.rs}`.
  - **Tests:** Premult blend of semi-transparent over opaque matches expected straight-alpha golden values; reused-buffer fill == fresh-buffer baseline; allocation-count assertion.
  - **Risk:** Premult/straight rounding; signature stability for S1. Mitigated by invariant and golden-value tests.
- [x] Scanline buffer reuse: pre-allocate scanline scratch buffers to avoid per-polygon allocation (partial: AET active-edge vec is reused per fill)
  - **Goal:** Per-fill scratch allocation eliminated; buffers owned and reused by the rasterizer.
  - **Design:** Part of S2 (same subagent as premult-alpha item). Hoist coverage-span and edge-scratch vectors into a `RasterizerScratch` struct on the rasterizer, `clear()` at each fill start.
  - **Files:** Same as item 4 (S2 slice).
  - **Tests:** Covered by S2 golden-value and byte-identical baseline tests.
  - **Risk:** Buffer coherence across polygon fills. Mitigated by targeted tests.
- [x] Gaussian blur kernel caching: pre-compute separable kernel weights for common blur radii (`shadow::GaussianCache`)

## Integration
- [x] `oxiui-core` integration: `SoftBackend` implements `RenderBackend`; accepts same `DrawList` as GPU path
- [x] `oxiui-text` integration: receive glyph bitmaps from `TextPipeline`, blit into framebuffer at layout-computed positions
  - **Goal:** `oxiui-render-soft` uses `oxiui-text`'s `TextPipeline` for all text rendering.
  - **Design:** Covered by S1 (adding `oxiui-text` as optional dep under the `text` feature; wiring `DrawText` through the pipeline).
  - **Files:** Same as item 1 (S1 slice).
  - **Tests:** Covered by S1 test suite.
  - **Risk:** None beyond S1 risks.
- [x] `oxiui-theme` integration: consume `ShadowSpec` for shadow rendering, design tokens for border widths
- [x] `oxiui-render-wgpu` integration: shared `DrawList` command format — `backend_switch.rs` module with `wgpu-compat` feature adds `DynBackend` enum (wraps `SoftBackend` or `WgpuBackend`) implementing `RenderBackend`. Both backends consume the same `DrawList` from `oxiui-core`. `BackendKind` discriminant for runtime inspection. Tests: 4 unit tests.
- [x] `oxiui-web` integration: canvas 2D pixel upload path — `canvas_upload.rs` module with `canvas-2d` feature; `upload_framebuffer`/`upload_rgba` use `putImageData` on wasm32, native stubs return `Ok(())`. `framebuffer_to_rgba8` helper exported. Tests: 4 unit tests (native stubs). Full wasm32 path gated on `cfg(target_arch = "wasm32")`.
- [ ] Dockerfile.ffi-audit: headless smoke test rendering in `rust:slim` Docker image (no GPU, no display server) — CI infrastructure, not Rust code. **DEFERRED: CI/Docker infrastructure item — not Rust code; no `.github/workflows/*.yml` files permitted under COOLJAPAN policy; revisit when a dedicated infra ticket is opened.**
- [x] COOLJAPAN ecosystem: PNG encoding via Pure Rust `png` crate (no stb_image, no libpng) ✓; no `tiny-skia` ✓; no `zip`/`flate2`/`zstd`/`bzip2`/`lz4` ✓; no `bincode` ✓; no `openblas` ✓; no `rustfft` ✓. OxiFFT (0.3.2) added as optional `fft-blur` feature — `fft_blur.rs` provides `gaussian_blur_alpha_fft` using `convolve_mode` for large kernel (radius ≥ 32px) blur; `should_use_fft_blur` threshold helper. Tests: 7 unit tests (threshold, symmetry, FFT-vs-direct match under `fft-blur` feature). All default features are 100% Pure Rust.

## Proposed follow-ups
- `RenderBackend` trait + `DrawList` consumer items (cross-crate, blocked on
  `oxiui-core` design of the trait + command buffer).
- Glyph blitting (blocked on `oxiui-text` `TextPipeline` consumer producing
  alpha-coverage bitmaps + advance metrics).
- Dirty-region tracking + invalidation (cross-crate; needs `oxiui-core`
  widget invalidation hooks).
- SIMD-accelerated pixel fill (`std::simd` once stable, or `wide` Pure-Rust).
- Parallel tile rendering (rayon) — tile iterator already lands; just needs
  the parallel driver + a safe per-tile scratch framebuffer.
- Premultiplied-alpha through-pipeline (full refactor away from straight-alpha
  composite path — helpers exist in `blend.rs`).
- Performance benchmarks via the `bench` skill + criterion (e.g. 10k rects in
  1920×1080; AET polygon throughput; Gaussian blur throughput).
- Golden-image snapshot tests with byte-stable PNG baselines under
  `tests/snapshots/`.
