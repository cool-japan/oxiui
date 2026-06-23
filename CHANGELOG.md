# Changelog

All notable changes to OxiUI are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
OxiUI adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.1] - Unreleased

### Added

### Changed

### Fixed

### Removed

---

## [0.2.0] - 2026-06-23

### Removed (BREAKING)

- **`oxiui` facade: `tray` feature removed** — system-tray support moved to the new
  `oxiui-tray` quarantine crate. Apps that need a system tray now depend on
  `oxiui-tray` directly and enable its `tray` feature. Rationale: `tray-icon` drags
  the entire GTK/GLib C stack (gtk-sys, gdk-sys, gio-sys, glib-sys, gobject-sys,
  atk-sys, cairo-sys-rs, pango-sys, libappindicator-sys) plus dirs-sys on Linux,
  which must not leak into the facade's closure.

- **`oxiui` facade: `slint` feature removed** — use the `oxiui-slint` adapter crate
  directly instead. `oxiui-slint` is a KNOWN-NON-PURE adapter: enabling its `slint`
  feature pulls `slint` -> parley/fontique -> `yeslogic-fontconfig-sys` (a C
  fontconfig binding) on Linux, which is slint-upstream font discovery with no pure
  opt-out today. The facade can no longer aggregate it and stay pure.

- **`oxiui-compute-wgpu`: `hot-reload` feature removed** — WGSL shader hot-reload
  moved to the new `oxiui-hot-reload-notify` quarantine crate. Apps that want live
  shader reload depend on `oxiui-hot-reload-notify` directly and construct
  `ShaderWatcher::new()` themselves. Rationale: `notify` unconditionally pulls
  `inotify-sys` (Linux) / `fsevent-sys` (macOS) / `kqueue-sys` (BSD) C FFI backends
  with no pure opt-out.

### Added

- **`oxiui-tray` crate** — §5 quarantine crate housing the `tray-icon`-backed system
  tray adapter (feature `tray`, default off).

- **`oxiui-hot-reload-notify` crate** — §5 quarantine crate housing the `notify`-backed
  WGSL hot-reload file watcher.

### Changed

- **Workspace version → 0.2.0** — breaking release. Internal `oxiui-*` path-dep ranges
  in `[workspace.dependencies]` advanced to `"0.2"`.

Motivation: **COOLJAPAN Pure Rust Policy v2 (L1)** — the `oxiui` facade and
`oxiui-compute-wgpu` `--all-features` closures are now free of non-allowlisted FFI
(GTK/fontconfig/inotify-sys/fsevent-sys/kqueue-sys). The non-pure adapters
(`oxiui-tray`, `oxiui-hot-reload-notify`, `oxiui-slint`) are quarantined as
direct-opt-in crates outside the L1 pure-set.

---

## [0.1.3] - 2026-06-20

### Changed

- **Workspace version bump** — all 13 sub-crates (`oxiui-core`, `oxiui-text`, `oxiui-theme`,
  `oxiui-render-wgpu`, `oxiui-render-soft`, `oxiui-compute-wgpu`, `oxiui-egui`, `oxiui-iced`,
  `oxiui-table`, `oxiui-accessibility`, `oxiui-web`, `oxiui-slint`, `oxiui-dioxus`) advanced
  to 0.1.3 in `[workspace.dependencies]` to keep the workspace version uniform after the 0.1.2
  publish.  No source-code or public-API changes relative to 0.1.2.

---

## [0.1.2] - 2026-06-10

### Added

- **`oxiui-render-wgpu`: `TextBridge::expand_draw_text_commands`** — new method that
  pre-expands all `DrawText` commands in a `DrawList` into per-glyph `Image` blits before
  the batcher is called; shaping/rasterization errors are silently skipped per-glyph so
  a single bad string cannot abort a frame.

- **`oxiui-render-wgpu`: `geometry.rs` tests** — five new unit tests:
  `solid_rect_produces_6_vertices`, `n_solid_rects_produce_n_times_6_vertices`,
  `image_produces_one_textured_draw_with_6_vertices`, `clip_pushpop_produces_correct_segments`,
  `scissor_culls_offscreen_rects`.

- **`oxiui`: `AppConfig`** — new window configuration builder (`title`, `size`, `resizable`,
  `min_size`, `max_size`, `decorations`, `transparent`, `always_on_top`, `icon`, `position`,
  `extra_fonts`, `design_tokens`, `typography`).

- **`oxiui`: `CommandPalette` + `Command`** — searchable command registry with fuzzy-match
  search; `register`, `register_with_shortcut`, `search` APIs.

- **`oxiui`: `NotificationQueue`** — notification queuing with deduplication, priority, and
  timeout support.

- **`oxiui`: PNG icon decoding** — `DrawText` handling and PNG icon decoding integrated into
  the egui backend via `app_config.icon`.

### Changed

- **`oxiui-render-soft`: `DynBackend`** — `Soft` and `Wgpu` variants now hold `Box<…>`
  instead of inline values, reducing stack footprint and eliminating the large-enum-variant
  clippy lint.  `as_soft()` / `as_soft_mut()` updated accordingly.

- **`oxiui-render-wgpu`: `batch.rs` `DrawText` classification** — with the `text` feature
  enabled, `DrawText` is pre-expanded before the batcher is called and any residual is
  classified as `Textured`; without the feature it falls back to `SolidColor`.

- **`oxiui`: `lib.rs` refactored** — `AppConfig`, `CommandPalette`, and `NotificationQueue`
  extracted to dedicated modules (`app_config.rs`, `command.rs`, `notification.rs`);
  `lib.rs` SLoC reduced from ~357 to focused facade re-exports.

### Fixed

- **`oxiui-render-soft`: `blit_glyph_clipped`** — now correctly `#[cfg(feature = "text")]`-
  gated; eliminates dead-code warning under `--no-default-features` builds.

---

---

## [0.1.1] - 2026-06-04

### Added

- **New crate `oxiui-compute-wgpu`** — headless wgpu GPU-compute abstraction for the COOLJAPAN
  ecosystem. Provides `ComputeContext` / `ContextBuilder`, `Dispatcher`, `PipelineCache`,
  `storage_buffer_init`, `read_back` / `read_back_async`, `BufferPool`, `SubAllocator`, a WGSL
  preprocessor/validator, and built-in kernels (`SHADER_MATMUL`, `SHADER_PREFIX_SUM`,
  `SHADER_BITONIC_SORT`, `SHADER_HISTOGRAM`, `SHADER_SPH_DENSITY`, and template variants).
  Optional `hot-reload` feature adds `ShaderWatcher` for live WGSL file watching via `notify`.
  Re-exports `wgpu`, `bytemuck`, and `pollster` so consumers need only a single dependency entry.

- **`oxiui-core`: `WindowManager` + `WindowChannel` + `WindowConfig` + `WindowId` + `WindowEvent`**
  — new `window` module provides a multi-window management layer decoupled from any concrete backend.

- **`oxiui-core`: `A11yRole` enum** — 28-variant semantic accessibility role type for use in
  `Widget::a11y_role()`.  Maps to the full `accesskit::Role` set via `oxiui-accessibility`.

- **`oxiui-core`: `Widget::a11y_role`, `Widget::a11y_label`, `Widget::a11y_description`** — three
  new default methods on the `Widget` trait that allow widgets to self-describe without depending on
  `oxiui-accessibility`.

- **`oxiui-core`: `UiCtx::text_area`** — new multi-line text-area method on `UiCtx`; returns
  `TextAreaResponse` (also new).

- **`oxiui-core`: `BlendMode`** — re-exported from `oxiui_core::paint`; enables per-draw-call blend
  mode selection in draw lists.

- **`oxiui-core`: `SpacingTokens` and `BorderTokens`** — design-token structs for semantic spacing
  (xs/sm/md/lg/xl in logical pixels) and border widths / radii.

- **`oxiui-core`: `layout_subtrees_parallel` + `LayoutTask`** — parallel layout API for
  multi-subtree layout on `rayon` thread pools.

- **`oxiui-core`: `Palette::default()`** — light-mode neutral palette (white background,
  indigo-500 accent); `Palette` now derives `PartialEq`.

- **`oxiui-accessibility`: `widget_bridge` module** — `widget_to_a11y_node`, `build_a11y_tree`,
  `A11yWidgetNode`, `NodeIdAllocator`, `core_role_to_widget_role`; bridges `oxiui_core::Widget`
  directly to the a11y tree without extra dependencies.

- **`oxiui-accessibility`: `text_bridge` module** (feature `text-bridge`) — `text_input_to_a11y`,
  `text_area_to_a11y`, `TextInputA11yParams`; converts `oxiui_text::TextInput` / `TextArea` to
  `A11yNode` values ready for the AccessKit pipeline.

- **`oxiui-accessibility`: `focus` module** — keyboard focus order tracking, added alongside new
  focus/text integration tests.

- **`oxiui-render-wgpu`**: major expansion of the `gpu` sub-module — new files:
  `blend.rs` (`BlendPipelineSet`, `blend_state_for_mode`),
  `buffer.rs` (typed GPU buffers, ring buffer),
  `compute_blur.rs` (GPU compute-based Gaussian blur),
  `earcut.rs` (pure-Rust polygon tessellator for GPU fill),
  `exec.rs` (`run_solid_pass`, `run_gradient_pass_batched`, `run_textured_pass`, `FrameStats`),
  `frame_pacing.rs` (`FrameTimer`, `FrameHistogram`, `PresentModeRecommendation`),
  `geometry.rs` (`build_geometry` CPU geometry builder),
  `hdr.rs` (HDR / tone-mapping pipeline),
  `instance.rs` (instanced-draw helpers),
  `layer_cache.rs` (dirty-region layer caching),
  `render_target.rs` (offscreen render target management),
  `ring_buffer.rs` (GPU ring buffer for streaming vertex data),
  `shadow.rs` (drop shadow blur pipeline),
  `stencil.rs` (stencil-based clip paths).

- **`oxiui-render-wgpu`: `WgpuBackend::headless_with_quality`** — new constructor that takes a
  `RenderQuality` to select the MSAA sample count; screen, shadow-mask, and blur pipelines are
  created with the correct sample counts. `WgpuBackend::headless` is preserved as a backward-
  compatible alias at `RenderQuality::low()`.

- **`oxiui-render-wgpu`: `WgpuBackend::ctx()`** — accessor for the underlying `GpuContext`.

- **`oxiui-render-wgpu`: `SdfTextPipeline`** (feature `text`) — GPU SDF text rendering pipeline
  backed by `oxitext-sdf`; uploads `SdfTile` glyph data to an R8Unorm atlas and renders resolution-
  independent text via a WGSL smoothstep shader.

- **`oxiui-render-wgpu`: `a11y_bridge` module** — maps the wgpu render tree to `A11yNode` values.

- **`oxiui-render-wgpu`: `theme_bridge` module** — converts `oxiui-theme` tokens to wgpu pipeline
  colour parameters at runtime.

- **`oxiui-render-wgpu`: `atlas.rs`** — texture atlas shelf-packer for glyph and image uploads.

- **`oxiui-render-wgpu`: `surface.rs`** — wgpu surface / swapchain wrapper for windowed rendering.

- **`oxiui-render-wgpu`: `text_bridge.rs`** — bridge between `oxiui-text` shaped output and the
  wgpu vertex pipeline.

- **`oxiui-render-wgpu`: WGSL shaders** — new shader files: `blur.wgsl`, `blur_compute.wgsl`,
  `composite.wgsl`, `instanced.wgsl`, `textured.wgsl`; golden-image regression test suite added
  (`tests/golden_image_tests.rs`).

- **`oxiui-render-soft`**: new modules:
  `backend_switch.rs` — runtime switch between software and wgpu backends;
  `canvas_upload.rs` — `softbuffer` canvas upload helpers;
  `fft_blur.rs` (feature `fft-blur`) — FFT-accelerated Gaussian blur via `oxifft` for kernels
    with radius ≥ 32 px (`gaussian_blur_alpha_fft`, `should_use_fft_blur`, `FFT_BLUR_MIN_RADIUS`);
  `simd_fill.rs` — portable SIMD (via `wide`) scanline fill helpers.

- **`oxiui-table`: `AsyncRowSource` trait + `PrefetchBuffer`** — async data source support with
  LRU cache and background prefetch for IO-bound backends (REST APIs, databases).

- **`oxiui-table`: `persistence.rs`** — `TableState` serialisation/deserialisation (column widths,
  sort keys, filter state) using `oxicode`.

- **`oxiui-table`: `accessibility.rs`** — ARIA table/grid accessibility tree builder; emits
  `A11yNode` rows/cells/headers via `oxiui-accessibility`.

- **`oxiui-table`: `text_integration.rs`** — rich text cell rendering via `oxiui-text`.

- **`oxiui-table`: `theme_integration.rs`** — per-table theme customisation mapping tokens to row
  stripe, header, and selection colours.

- **`oxiui-text`: `emoji` module** (feature `emoji`) — `EmojiSegmenter`, `EmojiRenderer`,
  `is_emoji_codepoint`; splits text into plain/emoji runs and routes emoji to colour glyph paths.

- **`oxiui-theme`: `serial.rs`** — `oxicode`-based theme serialisation: `ThemeSnapshot` captures
  the full set of design tokens and can be round-tripped to bytes.

- **`oxiui-web`**: new modules for wasm32 targets:
  `clipboard.rs`, `css.rs`, `drag_drop.rs`, `error_handling.rs`, `events.rs`,
  `font_loading.rs`, `fullscreen.rs`, `ime.rs`, `performance.rs`, `responsive.rs`,
  `service_worker.rs`.

- **`oxiui-web`: `GpuCapability` + `detect_gpu_capability()`** — runtime WebGPU / WebGL capability
  detection (probes `navigator.gpu` → WebGL2 → WebGL1 → software fallback).

- **`oxiui-web`: `cursor_css` + `apply_cursor`** — CSS cursor helpers that map `CursorShape` to
  the appropriate CSS cursor value and apply it to the canvas element.

- **`oxiui` facade**: new modules — `multiwindow` (`WindowRegistry`, `SecondaryWindow`,
  `App::open_window` / `App::close_window`), `dialog` (`DialogQueue`, `DialogKind`,
  `DialogResponse`, `DialogId`), `menu` (`MenuBar`, `MenuBarBuilder`, `Menu`, `MenuItem`),
  `logging` (feature `tracing`: `init_logging`, `LogLevel`), `tray` (feature `tray`:
  `TrayConfig`, `TrayHandle`, `TrayMenuItem`), `native_dialog` (feature `dialogs`:
  `open_file_dialog`, `save_file_dialog`, `message_dialog`, `confirm_dialog` via `rfd`).

- **`oxiui` facade**: `process_rss_bytes()` — reads RSS from `/proc/self/status` (Linux) or
  `task_info` (macOS stub) for startup memory profiling.

- **Workspace dependencies**: added `oxicode 0.2.4` (replaces ad-hoc serialisation), `oxitext-sdf
  0.1.0`, `oxifft 0.3.2` (replaces rustfft), `raw-window-handle 0.6`, `tracing 0.1.41`,
  `tracing-subscriber 0.3.19`, `criterion 0.8.2`, `wide 1.4.0`, `tray-icon 0.24.0`, `rfd
  0.17.2`; expanded `web-sys` feature set for clipboard, drag-drop, IME, resize observer, service
  worker, font loading, and error events.

### Changed

- **`oxiui-core`: `Color` and `Palette`** now derive `oxicode::Encode` + `oxicode::Decode`.
  `FontStyle`, `FontFeature`, `FontSpec` likewise derive encode/decode.

- **`oxiui-render-wgpu`: renderer refactored** — geometry building extracted to `geometry.rs`
  (`build_geometry`), render passes to `exec.rs`; `WgpuBackend` gains a persistent solid vertex
  buffer (reused across frames, grown on demand) and per-frame `FrameStats`.

- **`oxiui-iced`: `a11y_bridge` module added** — accessibility tree integration for the iced
  adapter; `IcedUiCtx` now exposes a11y node emission.

- **`oxiui-egui`**: accessibility integration tests (`tests/a11y_integration_tests.rs`),
  snapshot tests, styled-text and table integration tests added.

- Workspace `version` bumped from `0.1.0` to `0.1.1`; `oxiui-compute-wgpu` added as the 14th
  workspace member.

---

## [0.1.0] — 2026-06-01

Initial release of the OxiUI workspace — 13 crates, ~37 000 Rust SLOC,
zero FFI under default features.  No GTK, no Qt, no SDL, no AppKit, no Win32.

### New Crates

| Crate | Description |
|---|---|
| `oxiui-core` | Core trait surface: `Widget`, `UiCtx`, `Theme`, `Layout`, `EventSink`, `RenderBackend`; reactive primitives (`Signal`, `Computed`, `ReactiveRuntime`); constraint solver; event system; paint/draw-list; geometry |
| `oxiui-text` | OxiText + OxiFont bridge — `TextPipeline` shapes and rasterises text via the COOLJAPAN text stack; truncation, word-wrap, rich-text, IME preedit |
| `oxiui-theme` | COOLJAPAN dark/light palettes (Tokyo Night), high-contrast WCAG-AAA palette; `DesignTokens`, `TypographyScale`; `theme_picker` helper |
| `oxiui-render-wgpu` | wgpu GPU render-surface — texture atlas, draw-call batcher, clip-stack, quality presets, headless device init |
| `oxiui-render-soft` | Software CPU framebuffer backend — scanline rasteriser with AA, Bézier/path rendering, blend modes, PNG export, headless smoke path |
| `oxiui-egui` | egui + eframe adapter — palette→`egui::Visuals`, `StatefulEguiAdapter`, `tokens_to_egui_style`, OxiFont byte injection |
| `oxiui-iced` | iced 0.14 Elm-architecture adapter — `IcedUiCtx`, palette→`iced::Theme`, button/label/input widgets, IME stub |
| `oxiui-table` | Virtualized table widget — `RowSource` trait, viewport-windowed rendering, egui + iced backends, sorting/filtering |
| `oxiui-accessibility` | accesskit a11y tree builder — `A11yNode`, `A11yTree`, `A11yNodeBuilder`; widget-graph → `accesskit::TreeUpdate`; headless unit-testable |
| `oxiui-web` | wasm32 entry point — `mount()` on `<canvas>`, `WebHandle`, key mapping, non-wasm stubs; IME via web events |
| `oxiui-slint` | slint 1.16.1 optional adapter — `SlintCtx`, palette mapping (docs-only for M5), headless collection mode |
| `oxiui-dioxus` | dioxus 0.7 optional adapter — `DioxusCtx` reactive bridge, headless collection mode |
| `oxiui` | Facade crate — `App` builder (`title`, `theme`, `content`, `backend`, `min_size`); `Backend::{Egui,Iced,Slint,Dioxus}`; `reactive` module re-exports |

### Milestones delivered

- **M0** — Workspace skeleton, `oxiui-core` trait surface, `deny.toml`, `Dockerfile.ffi-audit`, scripts.
- **M1** — egui adapter + wgpu render + COOLJAPAN theme + OxiText/OxiFont integration.  Hello-world facade works on Linux/macOS/Windows.
- **M2** — iced adapter + theme bridge; both adapters share palette.
- **M3** — `oxiui-table` virtualized rows + iced facade `App::run()` fully wired.  Charts deferred (no plotting backend).
- **M4** — `oxiui-accessibility` (accesskit) + `oxiui-web` (wasm32) + IME CJK events (`UiEvent::ImePreedit`/`ImeCommit`).
- **M5** — softbuffer headless stable, high-contrast WCAG-AAA theme, `oxiui-slint` + `oxiui-dioxus` optional adapters, `Dockerfile.ffi-audit` smoke layer.

### Test coverage

1 204 unit tests across 13 crates — all pass.

### Policy compliance

- Pure Rust default features: zero `*-sys` crates under `cargo tree --edges normal`.
- MSRV: 1.89 (cascaded from oxitext M5 via `wide 1.4.0`).
- License: Apache-2.0.
- No `unwrap()` in production code.

[0.2.0]: https://github.com/cool-japan/oxiui/releases/tag/v0.2.0
[0.1.3]: https://github.com/cool-japan/oxiui/releases/tag/v0.1.3
[0.1.2]: https://github.com/cool-japan/oxiui/releases/tag/v0.1.2
[0.1.1]: https://github.com/cool-japan/oxiui/releases/tag/v0.1.1
