# oxiui — The COOLJAPAN Pure-Rust GUI facade

[![Crates.io](https://img.shields.io/crates/v/oxiui.svg)](https://crates.io/crates/oxiui)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui` is the top-level façade crate for OxiUI — the COOLJAPAN Pure-Rust UI layer with **no GTK (C), no Qt (C++), no SDL (C), no system widgets, and no raw AppKit / Win32 / Cocoa bindings**. It wraps the immediate-mode widget API from `oxiui-core`, selects a render + framework backend through Cargo features, and re-exports everything an application needs behind one builder: [`App`]. By default it boots an egui application rendered via wgpu; alternative backends (iced, Slint, Dioxus, software, web/WASM) are opt-in.

The facade is the only crate most applications depend on. It owns the application life-cycle: window configuration ([`AppConfig`]), the event loop ([`App::run`]), headless rendering for CI ([`App::run_headless_once`], [`App::screenshot`]), and cross-cutting features — plugins ([`Plugin`]), global hotkeys ([`HotkeyRegistry`]), a fuzzy-searchable command palette ([`CommandPalette`]), and a toast [`NotificationQueue`]. GPU drivers (Vulkan/Metal/DX12/WebGPU) are OS-provided at runtime and do not appear in `cargo tree --edges normal`, keeping the build Pure Rust.

> **Default-features note (GOVERNANCE §6):** `default = ["gpu", "egui"]`. A zero-feature facade build must select at least one Pure adapter (ADAPTER_PATTERN §3 rule 4), mirroring precedents such as `oxicrypto`'s `default = ["pure"]` and `oxitls`'s `default = ["pure", "webpki-roots"]`. For a strictly minimal / ffi-audit build use `--no-default-features --features software`.

## Installation

```toml
[dependencies]
# Default: egui + wgpu native app
oxiui = "0.1.1"

# Headless / ffi-audit: CPU softbuffer framebuffer, no GPU stack at build time
oxiui = { version = "0.1.1", default-features = false, features = ["software"] }

# iced retained-mode backend
oxiui = { version = "0.1.1", features = ["iced"] }

# Everything: tables, accessibility, web, plus the iced backend
oxiui = { version = "0.1.1", features = ["iced", "table", "a11y", "web"] }
```

## Quick Start

### egui backend (default)

```rust,no_run
use oxiui::{App, AppConfig};

App::new(AppConfig::new().title("Hello OxiUI"))
    .theme(oxiui::theme::cooljapan_default())
    .content(|ui| {
        ui.heading("Hello, world!");
        if ui.button("Quit").clicked { /* exit logic */ }
    })
    .run()
    .expect("UI error");
```

### iced backend

```rust,ignore
use oxiui::{App, AppConfig, Backend};

App::new(AppConfig::new().title("Hello OxiUI (iced)"))
    .theme(oxiui::theme::cooljapan_default())
    .backend(Backend::Iced)
    .content(|ui| {
        ui.heading("Hello from iced!");
        if ui.button("Quit").clicked { std::process::exit(0); }
    })
    .run()
    .expect("UI error");
```

### Headless (CI-friendly, no display)

```rust
use oxiui::{App, AppConfig};

let exit = App::new(AppConfig::new().title("smoke"))
    .content(|ui| { ui.heading("h"); })
    .run_headless_once()?;
assert_eq!(exit, oxiui::AppExit::Ok);
# Ok::<(), oxiui::UiError>(())
```

## Backend Selection

The active backend is chosen with [`App::backend`] and the [`Backend`] enum. The default is [`Backend::Egui`]. Each non-default variant is gated behind its own Cargo feature; the variant is only present when its feature is enabled.

| `Backend` variant | Feature | Framework / mode | Status |
|-------------------|---------|------------------|--------|
| `Egui` (default) | `egui` | egui + eframe, immediate-mode, rendered via wgpu | Live native window. |
| `Iced` | `iced` | iced, retained-mode (Elm-style update/view) | Live native window. Button clicks carry one-frame latency (inherent to the retained↔immediate bridge). |
| `Slint` | `slint` | Slint toolkit (Pure-Rust CPU renderer) | M5: headless collection mode. Native window (`run_event_loop`) deferred to M6. **GPL-3.0 OR royalty-free OR commercial** — see `oxiui-slint`. |
| `Dioxus` | `dioxus` | Dioxus reactive framework (`minimal` Pure-Rust feature set) | M5: headless collection mode. Native rendering via `dioxus-native` deferred to M6. |

The egui-on-wasm32 path is not driven through `App::run` (which returns `UiError::Unsupported` on wasm32); instead use [`oxiui::web::mount`](#web-wasm32-entry-point) from a browser binary.

## API Overview

### Application builder

| Item | Kind | Description |
|------|------|-------------|
| [`App`] | struct | The application builder and runner. Lazy: no GPU device, window, or event loop is created until `run()`. |
| [`AppConfig`] | struct | Window configuration (title, size, min/max size, decorations, transparency, always-on-top, icon, position, extra fonts). Builder methods + `Default`. |
| [`AppExit`] | enum | Exit status: `Ok`, `Error(String)`, `RequestedByUser`, `Programmatic(String)`. |
| [`Backend`] | enum | Backend selector (see table above). |

#### `App` builder methods

| Method | Description |
|--------|-------------|
| `App::new(config)` | Create an app from an `AppConfig` (theme defaults to `cooljapan_default`). |
| `.theme(Box<dyn Theme>)` | Set the UI theme. |
| `.content(F)` | Set the per-frame content closure (`FnMut(&mut dyn UiCtx) + Send`). |
| `.with_state(state, F)` | Set a stateful content closure; `state` is owned and passed by `&mut` each frame. |
| `.backend(Backend)` | Select the GUI backend. |
| `.min_size` / `.max_size` / `.decorations` / `.transparent` / `.always_on_top` / `.icon` / `.position` | Window-config passthroughs. |
| `.with_font(family, bytes)` | Register an extra font family (egui path; iced stores bytes, registration deferred). |
| `.with_frame_skip(bool)` | egui: defer repaint by 1 s when no input events occurred (CPU-saving dirty flag). |
| `.with_egui_ctx(F)` | egui escape hatch — per-frame callback receiving the raw `egui::Context` (requires `egui`). |
| `.table(source)` | Render an `oxiui_table::RowSource` as the app content (requires `table`). |
| `.on_init` / `.on_frame` / `.on_close` / `.on_resize` / `.on_focus` | Register lifecycle hooks (called in registration order). |
| `.plugin(P)` | Register a `Plugin`; plugins are sorted by ascending `priority()`. |
| `.notify(title, body, urgency)` | Enqueue a toast (urgency 0=low/3 s, 1=normal/5 s, 2=critical/10 s). |
| `.try_hotkey(mods, key, action)` | Register a global hotkey; `Err(HotkeyConflict)` on duplicate `(mods, key)`. |
| `.register_command(name, shortcut)` | Add a command to the palette. |
| `.command_matches(query) -> Vec<String>` | Fuzzy-search command labels (subsequence match). |
| `.notifications()` / `.hotkeys()` / `.extra_fonts()` | Read-only accessors for testing. |
| `.run() -> Result<AppExit, UiError>` | Launch the native window + event loop (dispatches by backend). |
| `.run_headless_once() -> Result<AppExit, UiError>` | Run one synthetic frame against a no-op `UiCtx` (no window; fires init/frame hooks + plugins). |
| `.run_with_return(F) -> Result<T, UiError>` | Run `content` once headlessly and forward its return value. |
| `.screenshot() -> Result<Vec<u8>, UiError>` | Render a headless frame to PNG bytes (requires `software`; else `UiError::Unsupported`). |
| `.build_a11y_snapshot(window_id) -> A11yTree` | Record content through `RecordingUiCtx` into an accessibility tree (requires `a11y`). |

### Cross-cutting feature types

| Item | Kind | Description |
|------|------|-------------|
| [`Plugin`] | trait | Life-cycle plugin: `init`, `update`, and `priority` (lower = earlier). |
| [`HotkeyRegistry`] | struct | Registry enforcing unique `(Modifiers, Key)` bindings; `register`, `conflict_check`, `len`, `is_empty`. |
| [`HotkeyBinding`] | struct | A single registered hotkey (`id`, `modifiers`, `key`, `action`). |
| [`HotkeyConflict`] | struct | Error when two hotkeys share a `(Modifiers, Key)` pair (`Display` + `Error`). |
| [`Command`] | struct | A named, searchable command (`id`, `label`, `shortcut`, `action`). |
| [`CommandPalette`] | struct | Fuzzy (subsequence, case-insensitive) command registry; `register`, `register_with_shortcut`, `search`, `len`, `is_empty`. |
| [`Notification`] | struct | A pending toast (`title`, `body`, `duration_ms`, `urgency`, `created_at`). |
| [`NotificationQueue`] | struct | FIFO toast queue; `push`, `enqueue`, `pop_due`, `len`, `is_empty`. |

### Pluggable runner infrastructure (`oxiui::runner`)

| Item | Kind | Description |
|------|------|-------------|
| [`BackendRunner`] | trait | Object-safe trait decoupling backend selection from `App::run`; `run(self, config, content, lifecycle)`. |
| [`LifecycleConfig`] | struct | Optional `on_close` / `on_resize` / `on_focus` callbacks passed to a runner. |
| `ContentFn` | type alias | `Box<dyn FnMut(&mut dyn UiCtx) + Send>`. |
| [`EguiRunner`] | struct | egui runner (wiring stub for M6; requires `egui`). |
| `IcedRunner` | struct | iced runner (wiring stub for M6; requires `iced`). |

> Note: `EguiRunner` / `IcedRunner` are stable public types but currently wiring stubs — the live render paths remain inside `App::run`. Full delegation lands in M6.

### Re-exported modules

| Module | Gate | Contents |
|--------|------|----------|
| `oxiui::prelude` | — | The common set: `App`, `AppConfig`, `AppExit`, `Backend`, `HotkeyConflict`, `Notification`, `Plugin`; from `oxiui-core`: `ButtonResponse`, `Color`, `UiCtx`, `UiError`, `Point`/`Rect`/`Size`, `AlignContent`/`FlexWrap`/`RichTextSpan`, reactive (`Signal`, `Computed`, `ReactiveRuntime`, `ReactiveError`); from `oxiui-theme`: `CooljapanTheme`. |
| `oxiui::core` | — | Glob re-export of all of `oxiui-core`. |
| `oxiui::theme` | — | `cooljapan_default`, `dark`, `light`. |
| `oxiui::theme_picker` | — | `theme_picker`, `by_name` (re-exported as `theme_by_name`), `BUILTIN_THEMES`. |
| `oxiui::text` | — | `FontSpec`, `FontStyle`, `FontFeature`. |
| `oxiui::solver` | — | Constraint solver: `Constraint`, `Expression`, `RelOp`, `Solver`, `SolverError`, `Strength`, `Term`, `Variable`. |
| `oxiui::reactive` | — | `Signal`, `Computed`, `ReactiveRuntime`, `ReactiveError`. |
| `oxiui::render` | `software` | `Framebuffer`, `RgbaBuffer`, `render_headless_once`, `render_headless_scene`. |
| `oxiui::table` | `table` | Glob re-export of `oxiui-table`. |
| `oxiui::accessibility` | `a11y` | `A11yTree`, `A11yNode`, `WidgetRole`. |
| `oxiui::recording` | `a11y` | `RecordingUiCtx`, `RecordingEntry`. |
| `oxiui::web` | `web` | `mount` (browser canvas entry point). |

### Crate-root re-exports

From `oxiui-core` at the crate root: `ButtonResponse`, `Color`, `FontSpec`, `Palette`, `Theme`, `UiCtx`, `UiError`. From `runner`: `BackendRunner`, `LifecycleConfig` (always), plus `EguiRunner` (`egui`) and `IcedRunner` (`iced`). From `theme_picker`: `theme_by_name`, `theme_picker`, `BUILTIN_THEMES`.

### Web (wasm32) entry point

With `--features web`, `oxiui::web::mount(canvas_id)` boots an OxiUI app on a `<canvas>` (wasm32 only; returns `Err` on native). See [`oxiui-web`](../oxiui-web) for the full browser API.

## Feature Flags

Features fall into two groups: **render backends** (how pixels reach the screen) and **framework backends** (which widget framework drives the frame), plus optional capability modules.

### Render backends

| Feature | Pulls in | Description |
|---------|----------|-------------|
| `gpu` | `oxiui-render-wgpu` | wgpu GPU rendering (Metal/Vulkan/DX12/WebGPU drivers OS-provided at runtime). Part of `default`. |
| `software` | `oxiui-render-soft` | Pure-CPU softbuffer framebuffer; enables `oxiui::render::*` and `App::screenshot`. No GPU stack at build time. |

### Framework backends

| Feature | Pulls in | Description |
|---------|----------|-------------|
| `egui` | `oxiui-egui`, `oxiui-render-wgpu`, `egui`, `eframe`, `png` | egui + eframe immediate-mode backend (default). Enables the `with_egui_ctx` escape hatch and `Backend::Egui`. |
| `iced` | `oxiui-iced`, `iced` | iced retained-mode backend; enables `Backend::Iced` and `IcedRunner`. |
| `slint` | `oxiui-slint` (+ its `slint`) | Slint adapter; enables `Backend::Slint`. **GPL-3.0 OR royalty-free OR commercial** — verify license compatibility. |
| `dioxus` | `oxiui-dioxus` (+ its `dioxus`) | Dioxus reactive adapter (`minimal` Pure-Rust set); enables `Backend::Dioxus`. |

### Capability modules

| Feature | Pulls in | Description |
|---------|----------|-------------|
| `table` | `oxiui-table` | Table widget; enables `oxiui::table::*` and `App::table`. |
| `a11y` | `oxiui-accessibility`, `accesskit` | Accessibility tree builder; enables `oxiui::accessibility`, `oxiui::recording`, and `App::build_a11y_snapshot`. |
| `web` | `oxiui-web` | wasm32 browser canvas mount; enables `oxiui::web::mount`. |
| `default` | `gpu` + `egui` | Boots an egui app rendered via wgpu. |

## Errors

[`App`] methods return `Result<_, oxiui_core::UiError>`. `UiError` is `#[non_exhaustive]`.

| Variant | Meaning (facade context) |
|---------|--------------------------|
| `Backend(String)` | Windowing / GPU / backend-runtime initialisation failure (e.g. eframe or iced `run` error). |
| `Render(String)` | Render-pipeline error. |
| `Window(String)` | Window-management error. |
| `Unsupported(String)` | Requested feature/backend not available (e.g. `screenshot` without `software`; `App::run` on wasm32). |
| `Layout(String)` | Layout-engine error (unsatisfiable constraints). |
| `Focus(String)` | Focus-management error. |
| `Clipboard(String)` | Clipboard access error. |
| `DragDrop(String)` | Drag-and-drop protocol error. |
| `Other(String)` | Any other error. |

A separate [`HotkeyConflict`] error (its own `std::error::Error` type) is returned by `App::try_hotkey` / `HotkeyRegistry::register` on a duplicate `(Modifiers, Key)` binding.

## Examples

The crate ships runnable examples (see `examples/`):

```sh
cargo run --example hello                              # egui (default)
cargo run --example hello_iced   --features iced       # iced backend
cargo run --example hello_table  --features table      # table widget
cargo run --example hello_slint  --features slint      # Slint adapter
cargo run --example hello_dioxus --features dioxus     # Dioxus adapter
```

## Related Crates

| Crate | Role |
|-------|------|
| [`oxiui-core`](../oxiui-core) | Core traits/types: `UiCtx`, `Theme`, `Palette`, `Color`, `UiError`, events, reactive primitives, constraint solver. |
| [`oxiui-text`](../oxiui-text) | Text shaping and font handling. |
| [`oxiui-theme`](../oxiui-theme) | COOLJAPAN themes (`cooljapan_default`, `dark`, `light`, `CooljapanTheme`). |
| [`oxiui-table`](../oxiui-table) | Table widget (`table` feature). |
| [`oxiui-accessibility`](../oxiui-accessibility) | accesskit-based a11y tree (`a11y` feature). |
| [`oxiui-render-soft`](../oxiui-render-soft) | Pure-CPU software renderer (`software` feature). |
| [`oxiui-render-wgpu`](../oxiui-render-wgpu) | wgpu GPU renderer (`gpu` feature). |
| [`oxiui-egui`](../oxiui-egui) | egui + eframe backend (`egui` feature). |
| [`oxiui-iced`](../oxiui-iced) | iced retained-mode backend (`iced` feature). |
| [`oxiui-slint`](../oxiui-slint) | Slint adapter (`slint` feature). |
| [`oxiui-dioxus`](../oxiui-dioxus) | Dioxus adapter (`dioxus` feature). |
| [`oxiui-web`](../oxiui-web) | wasm32 browser entry point (`web` feature). |

[`App`]: https://docs.rs/oxiui/latest/oxiui/struct.App.html
[`AppConfig`]: https://docs.rs/oxiui/latest/oxiui/struct.AppConfig.html
[`AppExit`]: https://docs.rs/oxiui/latest/oxiui/enum.AppExit.html
[`App::run`]: https://docs.rs/oxiui/latest/oxiui/struct.App.html#method.run
[`App::run_headless_once`]: https://docs.rs/oxiui/latest/oxiui/struct.App.html#method.run_headless_once
[`App::screenshot`]: https://docs.rs/oxiui/latest/oxiui/struct.App.html#method.screenshot
[`App::backend`]: https://docs.rs/oxiui/latest/oxiui/struct.App.html#method.backend
[`Backend`]: https://docs.rs/oxiui/latest/oxiui/enum.Backend.html
[`Backend::Egui`]: https://docs.rs/oxiui/latest/oxiui/enum.Backend.html
[`Plugin`]: https://docs.rs/oxiui/latest/oxiui/trait.Plugin.html
[`HotkeyRegistry`]: https://docs.rs/oxiui/latest/oxiui/struct.HotkeyRegistry.html
[`HotkeyBinding`]: https://docs.rs/oxiui/latest/oxiui/struct.HotkeyBinding.html
[`HotkeyConflict`]: https://docs.rs/oxiui/latest/oxiui/struct.HotkeyConflict.html
[`Command`]: https://docs.rs/oxiui/latest/oxiui/struct.Command.html
[`CommandPalette`]: https://docs.rs/oxiui/latest/oxiui/struct.CommandPalette.html
[`Notification`]: https://docs.rs/oxiui/latest/oxiui/struct.Notification.html
[`NotificationQueue`]: https://docs.rs/oxiui/latest/oxiui/struct.NotificationQueue.html
[`BackendRunner`]: https://docs.rs/oxiui/latest/oxiui/runner/trait.BackendRunner.html
[`LifecycleConfig`]: https://docs.rs/oxiui/latest/oxiui/runner/struct.LifecycleConfig.html
[`EguiRunner`]: https://docs.rs/oxiui/latest/oxiui/runner/struct.EguiRunner.html

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
