# Changelog

All notable changes to OxiUI are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
OxiUI adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.2] - 2026-08-06

### Added

- **The facade's multi-window registry and menu bar are now consumed by the
  backend layer** (they used to be stored and read by nobody, so
  `App::open_window` and `App::menu_bar` were silent no-ops).
  - New `oxiui::shell::ShellConfig` bundles the registered secondary windows,
    their content closures, the menu bar and the runtime `WindowHandle`.
    `App::run()` builds it and hands it to the backend through a new
    **`BackendRunner::set_shell`** trait method whose default implementation
    accepts an empty shell and returns `UiError::Unsupported` for a non-empty
    one — a runner can no longer swallow a window or a menu bar in silence.
  - New `oxiui::menu::render_menu_bar` + `MenuBarState` draw a `MenuBar`
    through any `UiCtx`, track the open menu/submenu chain across frames, and
    invoke the selected `MenuItem::Action`. The egui backend renders it above
    the primary frame, iced above the primary view, and the headless /
    accessibility paths (`run_headless_once`, `build_a11y_snapshot`) render it
    too. Unsupported containers degrade (`menu_bar` → `horizontal` → inline,
    `popup` → `vertical` → inline) rather than dropping the bar.
  - The native egui backend opens one `egui::Context::show_viewport_deferred`
    viewport (a real OS window) per registered secondary window, translating
    `WindowConfig` into a `ViewportBuilder`, and tears it down when the user
    closes it.
  - New `App::open_window_with(config, F)` attaches a per-window content
    closure; new `App::window_handle()` returns a cloneable, thread-safe
    `WindowHandle` with `open` / `close` / `focus`, drained each frame into a
    `WindowSession` state machine (typed `UiError::Window` for an unregistered
    id or a focus request on a closed window).
  - New `App::run_headless_frame(&mut dyn UiCtx) -> usize` drives one complete
    frame (menu bar → init → content → per-frame hooks) through a
    caller-supplied context and reports how many menu actions fired — the
    display-free way to test the whole frame path.
  - `IcedRunner::set_shell` accepts the menu bar but rejects secondary windows
    with `UiError::Unsupported` (iced 0.14's `application` runtime is
    single-window; `iced::daemon` would be required), and `Backend::Dioxus`
    rejects any non-empty shell before `App::run()` does anything else.
- **`oxiui_egui::EguiUiCtx::with_id_base` / `oxiui_iced::adapter::IcedUiCtx::with_id_base`**
  (plus `IcedUiCtx::next_widget_id`) let two contexts drawing into the same
  frame reserve disjoint widget-id ranges. Both backends use this to render the
  menu bar in a reserved high range: the bar's widget count changes as its
  drop-down opens and closes, and without the split every content widget id
  would shift with it — silently rerouting iced clicks and invalidating egui's
  persistent widget state (dropdown selection, popup position, grid widths).

- **`fuzz/` cargo-fuzz harness** for `oxiui-render-soft`: `fuzz_fill_polygon` /
  `fuzz_composite_into` (regression coverage generalizing the 0.2.1 Security
  fixes below to arbitrary finite input) and `fuzz_bezier_flatten`
  (`path.rs`'s adaptive bezier flattening). `cargo +nightly fuzz build` is
  green. The harness already found two new, unfixed crash bugs on its first
  run — see the Known Issues section below.
- **`crates/oxiui/examples/hello_headless.rs`** — a headless render smoke
  test (no window/GPU/display) that exits non-zero if the rendered frame has
  no visible content, gated behind a new `required-features = ["software"]`
  example entry. `Dockerfile.ffi-audit` now runs it as a final smoke-test
  layer, and `crates/oxiui/tests/example_compilation.rs` gained matching
  `example_hello_headless_compiles` / `example_hello_headless_runs_and_exits_ok`
  regression tests.
- **`rustfmt.toml` / `clippy.toml`** at the workspace root (stable-only keys;
  `clippy.toml`'s `msrv` matches `Cargo.toml`'s `rust-version = "1.89"`).

### Changed

- **`oxiui-slint::run_slint` / `oxiui-dioxus::run_dioxus` no longer fabricate
  a successful run.** Both previously executed the content closure in
  headless collection mode and returned `Ok(())` even though no window ever
  opened — indistinguishable from a real window that opened and was closed.
  They now return `Err(UiError::Unsupported(..))` until their native
  event-loop integration actually lands (`slint::run_event_loop` for slint; a
  Pure-Rust `dioxus-native` launch path for dioxus); `App::run()` with
  `Backend::Dioxus` propagates the same. Headless widget collection via
  `SlintCtx`/`DioxusCtx` directly is unaffected and remains the supported
  headless-testing path.
- **`OxiIcedWidget::draw` now renders.** The custom iced widget previously had
  an empty `draw` body (a placed `WidgetSpec` was a correctly-sized invisible
  hole); it now materializes the spec into a real iced element (the same
  `build_one` pipeline `IcedUiCtx::into_iced_element` uses) and delegates
  every `Widget` method — layout, draw, events, children, overlay — to it.
- **`oxiui-web::set_theme` now actually changes the theme.** It previously
  encoded an inert `"__theme:<name>"` sentinel through `inject_event` that no
  code decoded (a fabricated `ImeCommit`, which risked inserting that literal
  string into a focused text field). It now applies dark/light directly via
  `egui::Context::set_theme`, and high-contrast via the COOLJAPAN WCAG-AAA
  palette (`oxiui_theme::cooljapan_high_contrast()`) mapped through
  `oxiui_egui::palette_to_egui_visuals` onto the `Dark` style slot.
- **`oxiui-compute-wgpu`: GPU device-loss/OOM no longer panics a published
  library.** `buffer::read_back` / `read_back_range` / `TypedBuffer::download`
  now return `Result<_, ComputeError>` instead of `.expect()`-ing the device
  poll and buffer-mapping outcomes; all five `Dispatcher` methods (`map_f32`,
  `zip_map_f32`, `reduce_sum_f32`, `sph_density`, `sort_f32`) propagate it.
  `integration::render_soft`'s GPU blur/dither/gradient-fill helpers and
  `integration::text::rasterize_glyphs` keep their existing infallible public
  signatures but now gracefully fall back to their CPU sibling implementation
  on a GPU-runtime failure — the same fallback they already used for
  "no GPU available" — instead of panicking.
- `notify` (used only by the `oxiui-hot-reload-notify` quarantine crate)
  moved from an inline per-crate version pin to `[workspace.dependencies]`.
- **`pollster` updated from `0.4.0` to `1.0.1` (major version).** `oxiui`
  re-exports `pollster` alongside `wgpu`/`bytemuck` for consumers driving the
  async GPU device/adapter request calls, so this is a downstream-visible
  bump, not just an internal dependency refresh.
- `oxifft` updated from `0.4.1` to `0.4.2`.
- `oxicode` updated from `0.2.5` to `0.2.6`.
- `oxifont` updated from `0.2.1` to `0.2.2`.
- `oxitext` updated from `0.2.1` to `0.2.2`; `oxitext-sdf` updated from
  `0.2.1` to `0.2.2`.

### Fixed

- **`oxiui-web` compiles for `wasm32-unknown-unknown` again.** Two web-sys
  features were missing from the workspace list (`HtmlHeadElement`,
  `FontFaceDescriptors`), and several call sites had drifted from the web-sys
  0.3.103 API shape: `document.exec_command` now goes through a
  `web_sys::HtmlDocument` cast (added `HtmlDocument` feature);
  `FontFace::new_with_str_and_descriptors` replaces the removed
  `..str_and_str_sequence_or_descriptors` constructor, using the typed
  `FontFaceDescriptors` dictionary instead of manual `js_sys::Reflect::set`
  calls; `request_fullscreen` / `exit_fullscreen` are now called as the
  synchronous `Result<(), JsValue>` / `()` functions web-sys 0.3.103 exposes
  (previously assumed a `Promise` return and wrapped the call in
  `spawn_local`); `ServiceWorkerRegistration::unregister` now handles its
  `Result<Promise, JsValue>` return (a synchronous `Err` — no active worker —
  now counts as "nothing unregistered" instead of being unreachable code).

### Known Issues

- `cargo fuzz run` on the new harness reproduces two pre-existing crashes in
  `oxiui-render-soft` (not introduced by 0.2.2, not yet fixed): a
  subtract-overflow panic in `scanline.rs`'s fast-forward loop and a
  stack-overflow in `path.rs`'s `flatten_quad`/`flatten_cubic` from a
  finite-input float overflow inside the private `mid()` helper. Repro inputs
  are checked into `fuzz/regressions/` (unlike `fuzz/corpus/`/`fuzz/artifacts/`,
  this directory is not gitignored) with decoded parameters in
  `fuzz/regressions/README.md`. See `TODO.md`'s Production-Readiness Backlog
  for the full writeup.

## [0.2.1] - 2026-07-30

### Added

- **Lifecycle hooks are now live on both backends.** `App::on_close` / `on_resize`
  / `on_focus` fire from the real event loops: egui polls the viewport size and
  focus each frame and fires `on_close` from `eframe::App::on_exit`; iced drives
  them from an `iced::event::listen_with` subscription
  (`Resized`/`Focused`/`Unfocused`/`CloseRequested`). A new
  `runner::LifecycleTracker` deduplicates raw snapshots into
  `runner::LifecycleEvent`s so `on_resize`/`on_focus` fire only on real changes
  and `on_close` fires at most once. Hooks receive a shared no-op `UiCtx`
  (lifecycle events fire outside a live drawing frame).
- **`EguiRunner` / `IcedRunner` are now live `BackendRunner`s.** They carry the
  app's theme, hooks, and plugins and own the `eframe::run_native` /
  `iced::application` event loop; `App::run()` moves its state into the matching
  runner and delegates. `EguiRunner::new()` / `IcedRunner::new()` (and a `theme`
  setter) construct them for dependency injection / testing.
- **iced backend honours the configured window size.** `IcedRunner` chains
  `.window_size(...)` (the `width`/`height` were previously silently discarded)
  and `.exit_on_close_request(false)` so `on_close` hooks run before the window
  closes.
- **`oxiui-egui`: IME preedit cursor position is now forwarded to egui.** egui
  0.35 added a cursor field to `ImeEvent::Preedit`
  (`active_range_chars: Option<Range<usize>>`); `forward_event_to_egui` converts
  `UiEvent::ImePreedit`'s byte-offset `cursor` range to egui's char-offset range
  (the same conversion `egui-winit` performs for raw OS IME events), using
  `str::get` so a range that lands off a UTF-8 char boundary degrades to `None`
  instead of panicking. Previously always dropped, since egui <= 0.34's
  `ImeEvent::Preedit` was a bare `String` with nowhere to put it.

### Changed

- **`runner::LifecycleConfig` fields are now `Vec<HookFn>`** (`on_close` /
  `on_resize` / `on_focus`) instead of the previous mismatched single closures,
  so they carry the app's real hook vectors. This is a visible change to the
  feature-gated public `LifecycleConfig` type.
- **`with_persistent_state` now actually persists on close.** State is shared via
  `Arc<Mutex<_>>` between the content closure and the `on_close` hook, which
  encodes it with `oxicode` and writes it to the storage path (replacing the old
  `let _ = path` no-op). `run_headless_once` now fires `on_close` after its single
  frame, so headless runs persist deterministically.
- `egui` / `eframe` updated from 0.34.3 to 0.35.0 (source of the `ImeEvent::Preedit`
  struct-variant change described above).
- `slint` updated from 1.16.1 to 1.17.0.
- `oxifft` updated from 0.3.2 to 0.4.1.
- `wasm-bindgen` updated from 0.2.125 to 0.2.126; `web-sys` updated from 0.3.102 to
  0.3.103.
- `oxicode` updated from 0.2.4 to 0.2.5.
- `oxifont` updated from 0.2.0 to 0.2.1.
- `oxitext` updated from 0.2.0 to 0.2.1; `oxitext-sdf` updated from 0.2.0 to 0.2.1.

### Fixed

- **`oxiui-render-soft`: `gaussian_blur_alpha_fft` no longer silently no-ops when
  the `fft-blur` feature is disabled** — it now forwards to the direct
  `shadow::gaussian_blur_alpha` convolution, so the blur effect still applies. The
  `fft-blur` feature now only affects performance, not correctness.
- Replaced every `#[allow(unused_variables)]` across the workspace with explicit
  `let _ = (...)` discards in the inactive-cfg branch, so the lint no longer masks
  genuine dead code.

### Security

- **`oxiui-render-soft`: fixed a `u32` overflow in `composite_into`'s bounds
  check.** The required source length was computed as `w * h * 4` in `u32`,
  which wraps for large `w`/`h` (e.g. `w = h = 65_536` wraps to exactly `0`),
  producing a too-small guard value that would defeat the `src.len()` check and
  let the pixel-copy loop index past the end of `src`. The length is now
  computed with checked `usize` arithmetic; an overflow is treated as
  unsatisfiable and the call safely returns `0` instead of reading out of bounds.
- **`oxiui-render-soft`: fixed unbounded scanline iteration in `fill_polygon`,
  `fill_polygon_clipped`, and `paint_span`.** A polygon or span with extreme
  vertex/edge coordinates (e.g. attacker-influenced input far outside the
  framebuffer) could previously drive billions of empty loop iterations — a
  denial-of-service. Row and span ranges are now clamped to the framebuffer's
  own dimensions before iterating, with an edge fast-forward so shapes that are
  only partially off-screen still render correctly for their visible rows.

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

[0.2.1]: https://github.com/cool-japan/oxiui/releases/tag/v0.2.1
[0.2.0]: https://github.com/cool-japan/oxiui/releases/tag/v0.2.0
[0.1.3]: https://github.com/cool-japan/oxiui/releases/tag/v0.1.3
[0.1.2]: https://github.com/cool-japan/oxiui/releases/tag/v0.1.2
[0.1.1]: https://github.com/cool-japan/oxiui/releases/tag/v0.1.1
