# oxiui-web — wasm32 / browser entry-point template for OxiUI

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-web` is a **`publish = false` wasm32 entry-point crate**. On the `wasm32`
target it is a `cdylib` that mounts an OxiUI app onto an HTML `<canvas>` element
via [`wasm-bindgen`], driving the egui backend (`oxiui-egui`) through
[`eframe`]'s `WebRunner`. It exposes a small `wasm-bindgen` surface (`mount`,
`JsWebHandle`, `JsMountOptions`) so the app can be controlled from JavaScript or
TypeScript.

The crate is dual-target by design. The browser-specific dependencies
(`wasm-bindgen`, `web-sys`, `js-sys`, `eframe`, `egui`, `serde_json`) live in a
`[target.'cfg(target_arch = "wasm32")'.dependencies]` table and never appear in
native `[dependencies]`. On non-wasm targets the crate compiles to a thin stub
whose `mount` / `mount_sync` functions return `Err(MountError::FeatureNotSupported)`,
so native builds (and the OxiUI workspace's ffi-audit) succeed without pulling in
any browser stack. Everything here is Pure Rust — no C/C++ system widgets, no
native WebView.

## Not a library dependency — copy this as a template

This crate is intentionally **unpublished** (`publish = false` in its
`Cargo.toml`) and is **not** part of the [`oxiui`](../oxiui) facade. It is **not**
meant to be consumed as a dependency from crates.io.

Instead, **copy it as a template** for your own wasm app:

1. Copy the `crates/oxiui-web/` directory into your project (or vendor `src/lib.rs`
   and the `Cargo.toml` `[target.'cfg(...)']` dependency table).
2. Adapt `src/lib.rs` — replace the placeholder `WasmApp` frame with your own
   `content` closure / `UiCtx` calls.
3. Adjust the feature flags to your needs (see below); drop the DOM modules you
   do not use to shrink the wasm binary.
4. Depend on the published OxiUI crates you actually need from crates.io
   (`oxiui-core`, `oxiui-egui`, …) rather than on this template crate.

## Building

This crate is a wasm32 entry point: its `[lib]` declares
`crate-type = ["cdylib", "rlib"]`. Build it for the
`wasm32-unknown-unknown` target with your preferred wasm toolchain
(e.g. [`wasm-pack`] or [`trunk`]):

```sh
# Produce the wasm binary + JS glue into ./pkg
wasm-pack build --target web

# …or build the wasm artifact directly:
cargo build --target wasm32-unknown-unknown --release
```

On any non-wasm target the crate still compiles (to the inert stub), which keeps
`cargo build` / `cargo test` and the workspace ffi-audit green.

## Feature flags

The default set enables every optional DOM module so the template works out of
the box. Disable defaults and opt in individually to trim the wasm binary:

| Feature | Default | Effect |
|---------|---------|--------|
| `drag-drop` | on | Wire `dragenter` / `dragover` / `dragleave` / `drop` DOM listeners (`drag_drop` module). |
| `service-worker` | on | Service-worker registration / unregistration helpers (`service_worker` module). |
| `font-loading` | on | Load fonts via the CSS Font Loading API (`font_loading` module). |
| `fullscreen` | on | Request / exit / query the Fullscreen API (`fullscreen` module). |
| `web` | off | Generic marker feature for bundler-level tree-shaking experiments; no effect on the dependency set (that is selected by the `cfg(target_arch = "wasm32")` table). |

```toml
# Minimal wasm binary — opt into only what you need:
oxiui-web = { path = "vendored/oxiui-web", default-features = false, features = ["fullscreen"] }
```

## Quick start (after copying the template)

### JavaScript / TypeScript (wasm32)

After `wasm-pack build --target web`, import the generated module and mount onto
a `<canvas>`:

```js
import init, { mount } from './pkg/oxiui_web.js';

await init();
const handle = await mount('my-canvas'); // id of a <canvas> element

// Control the running app from JS:
handle.resize(800, 600);
handle.inject_event(JSON.stringify({ /* a UiEvent */ }));
if (handle.is_running()) {
    handle.stop();
}
```

### Native stub (any non-wasm target)

On non-wasm targets `mount` is a stub that always reports the unsupported target,
so the same source compiles and tests everywhere:

```rust
use oxiui_web::{mount, MountOptions, MountError};

match mount("my-canvas", MountOptions::new()) {
    Err(MountError::FeatureNotSupported) => { /* expected off-wasm */ }
    _ => unreachable!("native mount is always a stub"),
}
```

## API overview

### Mount entry points

| Item | Target | Signature / Description |
|------|--------|-------------------------|
| `mount` | wasm32 | `async fn mount(canvas_id: &str) -> Result<JsWebHandle, JsValue>` — resolves the `<canvas>`, starts `eframe::WebRunner`, returns a JS-facing handle. Exported via `#[wasm_bindgen]`. |
| `mount` | non-wasm | `fn mount(canvas_id: &str, opts: MountOptions) -> Result<WebHandle, MountError>` — stub, always `Err(MountError::FeatureNotSupported)`. |
| `mount_sync` | non-wasm | `fn mount_sync(canvas_id: &str, opts: MountOptions) -> Result<WebHandle, MountError>` — synchronous stub, always `Err(MountError::FeatureNotSupported)`. |

### `WebHandle`

A handle to a mounted OxiUI web app. On wasm32 it is backed by a shared
`egui::Context` slot populated on the first paint; on native it is inert.

| Method | Description |
|--------|-------------|
| `WebHandle::new()` | Construct a handle in the running state. |
| `stop(&self)` | Mark the app as stopped. |
| `resize(&self, width, height)` | On wasm32, request a repaint so the next frame picks up new canvas dimensions; no-op on native. |
| `inject_event(&self, ev_json: &str) -> Result<(), String>` | On wasm32, deserialise a JSON `oxiui_core::UiEvent` and forward it to egui via `oxiui_egui::forward_event_to_egui`; no-op on native. Returns `Err` if the JSON is invalid. |
| `is_running(&self) -> bool` | `true` while the app is running. |

Also implements `Debug` and `Default` (`Default` == `new()`).

### `JsWebHandle` (wasm32 only)

`#[wasm_bindgen]` wrapper around `WebHandle`, returned by `mount`. Exposes
`stop()`, `resize(width, height)`, `inject_event(ev_json) -> Result<(), JsValue>`,
and `is_running() -> bool` to JavaScript.

### `MountOptions`

Builder-style configuration for the mount call. Derives `Default`, `Clone`, `Debug`.

| Field / Method | Description |
|----------------|-------------|
| `theme_name: Option<String>` / `with_theme(&str)` | Optional theme name (e.g. `"dark"`, `"light"`). |
| `width: Option<f32>` / `with_width(f32)` | Optional canvas width in logical pixels. |
| `height: Option<f32>` / `with_height(f32)` | Optional canvas height in logical pixels. |
| `hidpi: Option<bool>` / `with_hidpi(bool)` | Enable HiDPI / Retina rendering. |
| `MountOptions::new()` | All fields `None`. |

A `#[wasm_bindgen]` builder `JsMountOptions` mirrors these setters for JS callers
(wasm32 only).

### `map_web_key`

| Function | Description |
|----------|-------------|
| `map_web_key(key: &str) -> oxiui_core::Key` | Maps a Web `KeyboardEvent.key` string to an `oxiui_core::Key`. Named keys (`"Enter"`, `"Tab"`, `" "` → `Space`, arrows, `Home`/`End`, `PageUp`/`PageDown`, `F1`–`F24`) map to their variants; single printable code points become `Key::Character`; any other multi-character name becomes `Key::Named` (forward-compatible escape hatch). |

## Errors — `MountError`

A `Clone + Copy + Debug + PartialEq + Eq` enum (also `std::error::Error`, and
converts to `JsValue` on wasm32 via its `Display` string).

| Variant | Discriminant | Meaning |
|---------|--------------|---------|
| `CanvasNotFound` | `0` | The requested `<canvas>` element was not found in the DOM. |
| `InitFailed` | `1` | The canvas was found but the runner could not be initialised. |
| `FeatureNotSupported` | `2` | The operation is not supported on the current target (returned by every native stub). |

The async wasm32 `mount` returns `Result<JsWebHandle, JsValue>`; DOM/eframe
failures surface as descriptive `JsValue` strings (missing `window`/`document`,
element is not a `<canvas>`, eframe/wgpu init failure), with `CanvasNotFound`
converted into a `JsValue`.

## Architecture

On wasm32, `mount(canvas_id)`:

1. Resolves the `<canvas>` element by `id` through `web_sys`.
2. Starts an `eframe::WebRunner` on the canvas running a minimal `WasmApp` (an
   `eframe::App` bridge — replace this with your own app when templating).
3. Captures the live `egui::Context` into an `Arc<Mutex<Option<egui::Context>>>`
   shared with the returned `WebHandle`, so `resize()` and `inject_event()` can
   reach into the running event loop from outside.
4. Returns a `JsWebHandle` for JavaScript control.

## Related crates

| Crate | Role |
|-------|------|
| [`oxiui`](../oxiui) | Facade crate. It does **not** depend on (or feature-gate) this template; on wasm32 `App::run` returns `Err(Unsupported)` pointing you at `oxiui_web::mount`. |
| [`oxiui-core`](../oxiui-core) | Defines `Key`, `UiEvent` (deserialised by `inject_event`), and `serde` support. |
| [`oxiui-egui`](../oxiui-egui) | egui adapter; provides `forward_event_to_egui` used to inject events into the live context. |
| [`oxiui-render-wgpu`](../oxiui-render-wgpu) | wgpu render backend powering the browser canvas via WebGL/WebGPU. |

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)

[`wasm-bindgen`]: https://rustwasm.github.io/wasm-bindgen/
[`wasm-pack`]: https://rustwasm.github.io/wasm-pack/
[`trunk`]: https://trunkrs.dev/
[`eframe`]: https://crates.io/crates/eframe
