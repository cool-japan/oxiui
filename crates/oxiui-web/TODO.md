# oxiui-web TODO

## Status
Comprehensive wasm32 entry point with full event handling, IME, clipboard, drag-and-drop, fullscreen, CSS injection, responsive design, performance monitoring, error handling, service worker support, and font loading. eframe WebRunner integration live. DirtyFlag + visibilitychange dirty rendering, tree-shaking feature flags. 127 tests passing (2026-06-03).

## Core Implementation
- [x] eframe WebRunner integration: wire `eframe::WebRunner::new().start(canvas_id, options, app_creator).await` to boot the egui event loop inside the browser, async entry point with `wasm_bindgen_futures::spawn_local` (done in wasm.rs, 2026-06-03)
- [x] Canvas sizing: `WebHandle::resize()` calls `egui::Context::request_repaint()` to trigger a layout pass; eframe's WebRunner internally installs a ResizeObserver to track canvas dimensions (done 2026-06-03)
- [x] Web event handling: `events::bind_events(canvas_id, callback)` wires `addEventListener` for mouse (down/up/move), wheel, keyboard (keydown/keyup), and touch (start/move/end) events; pure translation helpers in `events` module fully tested natively (done 2026-06-03)
- [x] IME composition events: `ime::bind_ime_events(callback)` wires `compositionstart/update/end`; `ime_preedit_event` / `ime_commit_event` pure constructors; `ime_full_cycle` for test simulation (done 2026-06-03)
- [x] Clipboard API: `clipboard::write_to_clipboard` (async Clipboard API), `write_to_clipboard_exec_command` (legacy fallback), `read_from_clipboard` (async with callback); all with native stubs (done 2026-06-03)
- [x] Drag-and-drop: `drag_drop::bind_drag_events(canvas_id, callback)` wires `dragenter/over/leave/drop`; `DragPayload` carries text/url/file names from `DataTransfer`; `DragEvent` type with kind/position/payload (done 2026-06-03)
- [x] Fullscreen API: `fullscreen::request_fullscreen`, `exit_fullscreen`, `is_fullscreen`, `toggle_fullscreen`, `on_fullscreen_change` listener (done 2026-06-03)
- [x] Cursor management: `cursor_css(CursorShape) -> &'static str` maps all cursor variants to CSS strings; `apply_cursor(canvas_id, shape)` sets `canvas.style.cursor` on wasm32 (no-op on native); `js_cursor_css()` export (done 2026-06-03)
- [x] Web font loading: `font_loading::load_font(request, callback)` and `load_fonts_parallel` using `FontFace` API and `document.fonts.add()`; `FontLoadRequest` builder (done 2026-06-03)
- [x] CSS integration: `css::inject_canvas_styles()` injects `CANVAS_BASE_CSS` into `<head>` (body overflow:hidden, canvas fill/no-select); `inject_css(text, marker)` generic with idempotency guard; `mark_canvas` applies `data-oxiui` attribute (done 2026-06-03)
- [x] Responsive design: `responsive::Breakpoint` enum (xs/sm/md/lg/xl/xxl), `detect_breakpoint()`, `detect_color_scheme()`, `detect_reduced_motion()`; `on_breakpoint_change` and `on_orientation_change` listeners (done 2026-06-03)
- [x] Performance monitoring: `performance::now_ms()` (performance.now / std::time fallback), `FrameTimer` FPS accumulator, `request_animation_frame`, `start_animation_loop` (done 2026-06-03)
- [x] Error handling: `error_handling::install_panic_hook()` (console_error_panic_hook), `install_onerror_handler()`, `install_unhandled_rejection_handler()`, `install_all_error_handlers()` (done 2026-06-03)
- [x] WebGPU detection: `detect_gpu_capability() -> GpuCapability` feature-detects `navigator.gpu` → WebGL2 → WebGL1 → SoftwareFallback on wasm32; returns `NotApplicable` on native; `js_detect_gpu()` JS export (done 2026-06-03)
- [x] Service worker support: `service_worker::register_service_worker(url, callback)` and `unregister_all_service_workers(callback)` using `navigator.serviceWorker` API (done 2026-06-03)
- [ ] Web accessibility: ARIA attributes on canvas fallback content (screen readers cannot read canvas pixels), generate off-screen DOM nodes mirroring the a11y tree for AT consumption (~100 SLOC) — DEFERRED pending oxiui-accessibility completion
- [x] Module export expansion: `#[wasm_bindgen]` exports for `js_set_theme(handle, name)`, `js_send_event(handle, json)`, `js_get_state(handle)`, `js_detect_gpu()`, `js_cursor_css(name)` — all in lib.rs (done 2026-06-03)
- [ ] Hot module reload (dev): detect HMR signal from bundler (webpack/vite), tear down and re-mount without full page reload (~40 SLOC) — DEFERRED (requires bundler-specific integration)

## API Improvements
- [x] `mount()` should return a `Handle` that allows stopping the app, resizing, and injecting events from JS
  - **Goal:** `#[wasm_bindgen] pub struct WebHandle` with `stop()`, `resize(w,h)`, `inject_event(json)->Result<(),JsValue>`, `is_running()->bool`; `mount()` returns `Result<WebHandle, JsValue>` (planned 2026-05-29)
  - **Design:** WebHandle wraps an internal state token; methods stub/no-op on non-wasm targets; inject_event parses JSON-encoded UiEvent and feeds it to the event loop
  - **Files:** `crates/oxiui-web/src/lib.rs`
  - **Tests:** WebHandle methods compile on native; is_running() returns false on stub
  - **Risk:** wasm_bindgen types require careful cfg-gating on non-wasm targets
- [x] Configuration object: `MountOptions { theme: &str, width: u32, height: u32, hidpi: bool }` passed from JS
  - **Goal:** `#[wasm_bindgen] pub struct MountOptions{theme_name,width,height,hidpi}` with builder methods; passed to `mount(canvas_id, opts)` (planned 2026-05-29)
  - **Design:** fields `pub theme_name: Option<String>`, `pub width: Option<f32>`, `pub height: Option<f32>`, `pub hidpi: Option<bool>`; `#[wasm_bindgen] impl MountOptions { pub fn new()->Self; pub fn with_theme(mut self, t:&str)->Self; pub fn with_width(mut self, w:f32)->Self; ... }`
  - **Files:** `crates/oxiui-web/src/lib.rs`
  - **Tests:** builder chain sets all fields; `MountOptions::new()` defaults all to None
  - **Risk:** wasm_bindgen does not support `Option<f32>` directly in some versions — may need to use f64 or a sentinel value
- [x] `#[wasm_bindgen]` typed error return instead of `JsValue` string errors
  - **Goal:** `#[wasm_bindgen] pub enum MountError{CanvasNotFound=0, InitFailed=1, FeatureNotSupported=2}` + `impl From<MountError> for JsValue` (planned 2026-05-29)
  - **Design:** replace string-based `JsValue` errors with typed enum; `From` impl converts to a JS number/string; mount() signature uses `Result<WebHandle, MountError>` internally, converted to `Result<WebHandle, JsValue>` at the wasm_bindgen boundary
  - **Files:** `crates/oxiui-web/src/lib.rs`
  - **Tests:** MountError::FeatureNotSupported as u8 == 2; From<MountError> for JsValue works
  - **Risk:** wasm_bindgen enum variants must be repr(u32) or similar
- [x] Async `mount()` signature: `pub async fn mount(canvas_id: &str) -> Result<Handle, MountError>` for proper error propagation
  - **Goal:** `pub async fn mount(canvas_id: &str, opts: MountOptions) -> Result<WebHandle, JsValue>` (wasm); non-wasm cfg-gated stub returns `Err(MountError::FeatureNotSupported.into())`; plus `mount_sync` for non-async callers (planned 2026-05-29)
  - **Design:** `#[cfg(target_arch="wasm32")]` async version; `#[cfg(not(target_arch="wasm32"))]` sync stub: `pub async fn mount(_:&str, _:MountOptions)->Result<WebHandle,JsValue>{Err(MountError::FeatureNotSupported.into())}`; `pub fn mount_sync(canvas_id:&str, opts:MountOptions)->Result<WebHandle,JsValue>` wraps the sync path
  - **Files:** `crates/oxiui-web/src/lib.rs`
  - **Tests:** native stub returns Err; mount_sync compiles on native
  - **Risk:** async fn on wasm requires wasm-bindgen-futures — check it's already a dep
- [ ] TypeScript definition generation: `wasm-bindgen --typescript` output with proper type annotations for all exports — DEFERRED (requires wasm-pack build pipeline)

## Testing
- [x] `cargo check --target wasm32-unknown-unknown` green for all features (~0 SLOC, CI gate)
  - **Goal:** verify `cargo check --target wasm32-unknown-unknown` compiles cleanly for the updated API (planned 2026-05-29)
  - **Design:** run as part of per-crate gate; no new code needed beyond the above changes
  - **Files:** none (CI-gate check only)
  - **Tests:** compile-only check
  - **Risk:** wasm32 may not be installed — `rustup target add wasm32-unknown-unknown` first
- [x] Native stub test: `mount("anything")` returns `Err` on non-wasm targets (~10 SLOC)
  - **Goal:** `#[test] async fn mount_returns_err_on_non_wasm()` — calls the stub, asserts `Err(MountError::FeatureNotSupported)` (planned 2026-05-29)
  - **Design:** native target (not wasm32), so the cfg-gated stub runs; use `tokio::test` or `futures::executor::block_on` to drive the async fn
  - **Files:** new `crates/oxiui-web/tests/api_tests.rs`
  - **Tests:** 1 test verifying stub returns Err
  - **Risk:** need an async test runner (tokio or futures) — check workspace deps; alternatively use `pollster::block_on` (Pure Rust, no tokio needed)
- [ ] Canvas resolution test (wasm-pack headless): mount on a test canvas, verify no JS exception (~20 SLOC) — **DEFERRED (requires wasm-pack headless browser test runner)**
- [x] Event translation tests (unit, non-DOM): construct synthetic web events, verify `UiEvent` output matches expected variants (~60 SLOC)
  - **Goal:** native unit tests verifying that web event → UiEvent translation functions produce correct output without any DOM or browser dependency (planned 2026-05-29)
  - **Design:** for any event-translation functions in lib.rs (e.g. mapping key names to `oxiui_core::Key` variants), test them directly; use synthetic inputs; no wasm-pack or headless browser needed
  - **Files:** `crates/oxiui-web/tests/api_tests.rs`
  - **Tests:** ~5 event translation tests (key_a→Key::A, Enter→Key::Enter, mouse position, etc.)
  - **Risk:** if translation functions are currently absent, add them to lib.rs as pure functions (no DOM calls) then test
- [x] Clipboard API mock test: `clipboard::tests` verifies write/read callbacks on native stubs (done 2026-06-03)
- [ ] ResizeObserver test: simulate resize callback, verify `UiEvent::Resize` emitted with correct dimensions (~20 SLOC) — DEFERRED (requires wasm-pack headless browser)
- [x] IME composition test: `ime::tests` verifies `ime_preedit_event`, `ime_commit_event`, and `ime_full_cycle` produce correct events natively (done 2026-06-03)
- [ ] WebGPU fallback test: mock `navigator.gpu = undefined`, verify GL backend selected (~20 SLOC) — DEFERRED (requires wasm-pack headless browser)
- [ ] wasm binary size check: verify release build wasm < 2MB (gzip < 500KB) for reasonable load times (~0 SLOC, CI gate) — **DEFERRED (requires wasm-pack build pipeline and CI wasm32 target)**
- [ ] Lighthouse accessibility audit: canvas fallback content has ARIA attributes, keyboard navigable (~manual, CI if possible) — **BLOCKED: requires deployed browser environment; cannot automate without headless Chrome + Lighthouse CI**

## Performance
- [ ] wasm-opt: run `wasm-opt -O3` post-build to minimize binary size and improve runtime performance — **DEFERRED (post-build tooling step; requires binaryen installed in CI)**
- [ ] Code splitting: lazy-load non-critical features (drag-and-drop, service worker) as separate wasm modules if supported — **DEFERRED (requires bundler-specific wasm chunk splitting; not supported in stable wasm-bindgen today)**
- [x] `requestAnimationFrame` scheduling with visibilitychange: only render when dirty, skip frames when tab is backgrounded — `DirtyFlag`, `bind_visibility_change`, and `start_dirty_animation_loop` complete in `performance.rs`. **Updated 2026-06-03:** `WasmApp` in `wasm.rs` now wires a `visibilitychange` DOM listener on `document` and a `DirtyFlag`; when the tab is hidden (`document.visibilityState == "hidden"`) or no dirty input occurred, `egui::Context::request_repaint_after(1s)` defers the next rAF to reduce CPU/battery usage. The first frame always renders immediately (dirty flag pre-set to `true`).
- [ ] SharedArrayBuffer: if cross-origin isolation headers are set, use shared memory for parallel rendering (rayon-web) — **DEFERRED (requires COOP/COEP server headers; rayon-web not yet stable on wasm32)**
- [ ] Texture streaming: progressive texture upload to avoid long stalls on first frame — **DEFERRED pending oxiui-render-wgpu wasm32 support**
- [x] Tree-shaking: `#[cfg(feature = "...")]` gates on all optional DOM APIs — `drag-drop`, `service-worker`, `font-loading`, `fullscreen` feature flags; modules excluded when feature disabled. **Audited 2026-06-03:** `drag_drop` and `service_worker` modules already gated in lib.rs; `ServiceWorkerContainer`/`DragEvent`/`FileList` web-sys features are part of the workspace web-sys feature list but their Rust module paths are only reachable when the corresponding feature flag is enabled. All four optional modules have `#[cfg(feature)]` guards.

## Integration
- [x] `oxiui-core` integration: `events` module produces `UiEvent` instances (MouseDown/Up/Move, KeyDown/Up, Wheel, Touch→Mouse, ImePreedit/ImeCommit) using `oxiui_core` types directly (done 2026-06-03)
- [x] `oxiui-egui` integration: eframe WebRunner wired in `wasm.rs`; `WasmApp` captures egui context; `WebHandle::inject_event` calls `oxiui_egui::forward_event_to_egui` (done 2026-06-03)
- [ ] `oxiui-render-wgpu` integration: WebGPU backend shares wgpu shaders (WGSL works natively in browsers) — DEFERRED pending render-wgpu wasm support
- [ ] `oxiui-render-soft` integration: canvas 2D context `putImageData` path for CPU-rendered framebuffer upload as WebGPU fallback — DEFERRED
- [x] `oxiui-theme` integration: `responsive::detect_color_scheme()` detects `prefers-color-scheme: dark`; `detect_reduced_motion()` detects `prefers-reduced-motion` (done 2026-06-03)
- [ ] `oxiui-accessibility` integration: generate off-screen DOM elements mirroring the AccessKit tree, ARIA role/label attributes, focus management via `tabindex` — DEFERRED pending oxiui-accessibility
- [ ] `oxiui-table` integration: table widget rendering in browser, virtual scroll with smooth scrolling via `requestAnimationFrame` — DEFERRED
- [x] COOLJAPAN ecosystem: wasm-bindgen and web-sys are Pure Rust; no emscripten or C/C++ toolchain required; console_error_panic_hook for Rust panic surfacing; font loading via CSS Font API; compression via oxiarc-* available (done 2026-06-03)
