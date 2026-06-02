# oxiui-web TODO

## Status
Minimal wasm32 entry point (~74 SLOC across lib.rs and wasm.rs). Provides `mount(canvas_id)` that resolves a `<canvas>` DOM element by ID and validates it is an `HTMLCanvasElement`. The canvas handle is immediately dropped -- no rendering or event loop is started. Native stub returns `Err`. The eframe `WebRunner` integration is not wired (documented as TODO(M5)).

## Core Implementation
- [ ] eframe WebRunner integration: wire `eframe::WebRunner::new().start(canvas_id, options, app_creator).await` to boot the egui event loop inside the browser, async entry point with `wasm_bindgen_futures::spawn_local` (~100 SLOC)
- [ ] Canvas sizing: `ResizeObserver` on the canvas element, translate resize events to `UiEvent::Resize`, handle `window.devicePixelRatio` for HiDPI displays, update surface size on DPR change (~80 SLOC)
- [ ] Web event handling: `addEventListener` bindings for mouse events (click, move, down, up, wheel), keyboard events (keydown, keyup), touch events (touchstart, touchmove, touchend), translate to `UiEvent` variants (~200 SLOC)
- [ ] IME composition events: `compositionstart`, `compositionupdate`, `compositionend` DOM events → `UiEvent::ImePreedit` / `UiEvent::ImeCommit`, position composition window near text input caret (~80 SLOC)
- [ ] Clipboard API: `navigator.clipboard.readText()` / `writeText()` via web-sys bindings, fallback to `document.execCommand('copy'/'paste')` for older browsers, bridge to `oxiui-core` clipboard abstraction (~60 SLOC)
- [ ] Drag-and-drop: `dragenter`, `dragover`, `dragleave`, `drop` events on canvas, `DataTransfer` access for files and text, `FileReader` for dropped file content (~80 SLOC)
- [ ] Fullscreen API: `canvas.requestFullscreen()`, exit fullscreen, fullscreen change event, toggle control (~30 SLOC)
- [ ] Cursor management: `canvas.style.cursor` CSS property updates based on `oxiui-core` cursor state (pointer, text, grab, etc.) (~20 SLOC)
- [ ] Web font loading: `FontFace` API for loading OxiFont files from URLs, `document.fonts.add()`, ready state polling (~60 SLOC)
- [ ] CSS integration: inject minimal CSS for canvas container (no scrollbar on body, canvas fill container, prevent text selection), scoped styles to avoid global pollution (~40 SLOC)
- [ ] Responsive design: media query listeners for viewport breakpoints, orientation change events, translate to `oxiui-theme` responsive breakpoint state (~40 SLOC)
- [ ] Performance monitoring: `performance.now()` for frame timing, `requestAnimationFrame` callback scheduling, frame rate display overlay (dev mode) (~40 SLOC)
- [ ] Error handling: `window.onerror` / `unhandledrejection` handlers that surface errors to OxiUI error reporting, `console_error_panic_hook` integration for Rust panic messages in browser console (~30 SLOC)
- [ ] WebGPU detection: feature-detect `navigator.gpu`, fall back to WebGL2 via wgpu's GL backend, report capability level to rendering pipeline (~40 SLOC)
- [ ] Service worker support: cache wasm binary and assets for offline use, provide `register_service_worker()` utility (~40 SLOC)
- [ ] Web accessibility: ARIA attributes on canvas fallback content (screen readers cannot read canvas pixels), generate off-screen DOM nodes mirroring the a11y tree for AT consumption (~100 SLOC)
- [ ] Module export expansion: `#[wasm_bindgen]` exports for `set_theme(name)`, `send_event(json)`, `get_state() -> JsValue` to allow JS interop beyond `mount()` (~60 SLOC)
- [ ] Hot module reload (dev): detect HMR signal from bundler (webpack/vite), tear down and re-mount without full page reload (~40 SLOC)

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
- [ ] TypeScript definition generation: `wasm-bindgen --typescript` output with proper type annotations for all exports

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
- [ ] Canvas resolution test (wasm-pack headless): mount on a test canvas, verify no JS exception (~20 SLOC)
- [x] Event translation tests (unit, non-DOM): construct synthetic web events, verify `UiEvent` output matches expected variants (~60 SLOC)
  - **Goal:** native unit tests verifying that web event → UiEvent translation functions produce correct output without any DOM or browser dependency (planned 2026-05-29)
  - **Design:** for any event-translation functions in lib.rs (e.g. mapping key names to `oxiui_core::Key` variants), test them directly; use synthetic inputs; no wasm-pack or headless browser needed
  - **Files:** `crates/oxiui-web/tests/api_tests.rs`
  - **Tests:** ~5 event translation tests (key_a→Key::A, Enter→Key::Enter, mouse position, etc.)
  - **Risk:** if translation functions are currently absent, add them to lib.rs as pure functions (no DOM calls) then test
- [ ] Clipboard API mock test: mock `navigator.clipboard`, verify read/write round-trip (~30 SLOC)
- [ ] ResizeObserver test: simulate resize callback, verify `UiEvent::Resize` emitted with correct dimensions (~20 SLOC)
- [ ] IME composition test: simulate `compositionupdate`/`compositionend`, verify `ImePreedit`/`ImeCommit` events (~30 SLOC)
- [ ] WebGPU fallback test: mock `navigator.gpu = undefined`, verify GL backend selected (~20 SLOC)
- [ ] wasm binary size check: verify release build wasm < 2MB (gzip < 500KB) for reasonable load times (~0 SLOC, CI gate)
- [ ] Lighthouse accessibility audit: canvas fallback content has ARIA attributes, keyboard navigable (~manual, CI if possible)

## Performance
- [ ] wasm-opt: run `wasm-opt -O3` post-build to minimize binary size and improve runtime performance
- [ ] Code splitting: lazy-load non-critical features (drag-and-drop, service worker) as separate wasm modules if supported
- [ ] `requestAnimationFrame` scheduling: only render when dirty (event-driven), skip frames when tab is backgrounded (`visibilitychange`)
- [ ] SharedArrayBuffer: if cross-origin isolation headers are set, use shared memory for parallel rendering (rayon-web)
- [ ] Texture streaming: progressive texture upload to avoid long stalls on first frame
- [ ] Tree-shaking: `#[cfg(feature = "...")]` gates on all optional DOM APIs to minimize baseline wasm size

## Integration
- [ ] `oxiui-core` integration: web event translation produces `UiEvent` instances consumed by the core event dispatch system
- [ ] `oxiui-egui` integration: eframe's wasm backend already uses egui; wire `EguiUiCtx` through `WebRunner`'s update closure
- [ ] `oxiui-render-wgpu` integration: WebGPU backend shares wgpu shaders (WGSL works natively in browsers)
- [ ] `oxiui-render-soft` integration: canvas 2D context `putImageData` path for CPU-rendered framebuffer upload as WebGPU fallback
- [ ] `oxiui-theme` integration: detect `prefers-color-scheme: dark` media query, auto-select dark/light theme; detect `prefers-reduced-motion`, `prefers-contrast: more`
- [ ] `oxiui-accessibility` integration: generate off-screen DOM elements mirroring the AccessKit tree, ARIA role/label attributes, focus management via `tabindex`
- [ ] `oxiui-table` integration: table widget rendering in browser, virtual scroll with smooth scrolling via `requestAnimationFrame`
- [ ] COOLJAPAN ecosystem: wasm-bindgen and web-sys are Pure Rust; no emscripten or C/C++ toolchain required; asset loading from URLs (no file system); wasm binary compression via oxiarc-* for self-hosted deployments if applicable
