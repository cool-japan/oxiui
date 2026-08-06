# oxiui-dioxus — Dioxus adapter for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-dioxus.svg)](https://crates.io/crates/oxiui-dioxus)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-dioxus` is the [Dioxus](https://dioxuslabs.com) backend adapter for OxiUI. Dioxus is a reactive, component-based framework: components are functions that return `Element` via the `rsx!` macro. This adapter bridges that retained, reactive model onto OxiUI's immediate-mode `UiCtx` closure API via two pieces: [`DioxusCtx`], a `UiCtx` implementation that collects each widget call into an ordered list, and [`run_dioxus`], a driver that executes a content closure against that context with a supplied theme.

Dioxus is dual-licensed **MIT OR Apache-2.0**, so this adapter carries no copyleft obligations. The adapter is configured to use Dioxus's `minimal` feature set (`macro`, `html`, `signals`, `hooks`, `launch`) — all Pure Rust. The `desktop` feature (wry/tao/WebKit/Chromium) is intentionally **excluded** because it pulls in C/C++ system dependencies that violate the COOLJAPAN Pure-Rust policy. The crate is fully usable with `default = []` (collection mode) and pulls in the `dioxus` crate only when you enable `--features dioxus`.

> **Milestone status:** Native Dioxus window rendering is not yet wired — the Pure-Rust `dioxus-native` (Blitz/Vello) renderer is not yet stable, and the `desktop` feature pulls in wry/tao/WebKit (C/C++), which violates the Pure Rust policy. [`run_dioxus`] therefore returns a typed `UiError::Unsupported` rather than a fake success, so callers can distinguish "no window opened" from `Ok`. For headless widget collection, construct a [`DioxusCtx`] directly (fully supported, no display or heavy deps).

## Installation

```toml
[dependencies]
# Collection mode only — no Dioxus dependency
oxiui-dioxus = "0.2.3"

# Enable Dioxus rendering (minimal Pure-Rust feature set; no wry/tao/WebKit)
oxiui-dioxus = { version = "0.2.3", features = ["dioxus"] }
```

## Quick Start

### Collection mode (no `dioxus` feature, fully testable)

```rust
use oxiui_dioxus::DioxusCtx;
use oxiui_core::UiCtx;

let mut ctx = DioxusCtx::default();
ctx.heading("App Title");
ctx.label("Hello, Dioxus!");
let _resp = ctx.button("Click me");

assert_eq!(ctx.items.len(), 3);
assert_eq!(ctx.items[0], "heading:App Title");
```

### Headless widget collection (supported today)

```rust
use oxiui_core::UiCtx;
use oxiui_dioxus::DioxusCtx;

let mut ctx = DioxusCtx::default();
ctx.heading("Hello from Dioxus");
ctx.label("OxiUI + dioxus backend");
assert_eq!(ctx.items.len(), 2);
```

`run_dioxus` currently returns `UiError::Unsupported` (the native window path is
not yet wired); use `DioxusCtx` directly for headless collection.

## API Overview

### Types

| Item | Kind | Description |
|------|------|-------------|
| [`DioxusCtx`] | struct | `UiCtx` adapter that records each widget call as a `"<kind>:<text>"` string in its public `items: Vec<String>` field. Derives `Debug` and `Default`. |

### Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| [`run_dioxus`] | `run_dioxus<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>` where `F: FnOnce(&mut dyn UiCtx)` | Contracted to launch a Dioxus window. That path is not yet wired (dioxus-native is not yet stable), so it currently returns `UiError::Unsupported`. Use `DioxusCtx` directly for headless collection. |

### `DioxusCtx` as a `UiCtx`

`DioxusCtx` implements the three **required** `UiCtx` methods. All other `UiCtx` widget methods (`slider`, `checkbox`, `dropdown`, `text_input`, …) fall back to their default trait implementations, which report `supported == false` so callers can detect non-support and degrade gracefully.

| Method | Behaviour in `DioxusCtx` |
|--------|--------------------------|
| `heading(&mut self, text)` | Pushes `"heading:<text>"` onto `items`. |
| `label(&mut self, text)` | Pushes `"label:<text>"` onto `items`. |
| `button(&mut self, label)` | Pushes `"button:<label>"` onto `items`; returns `ButtonResponse { clicked: false, hovered: false }` (collection mode never registers clicks). |

The `items` entries use the format `"<kind>:<text>"` (e.g. `"label:Hello"`, `"button:Quit"`), letting headless tests assert on the exact widget sequence without a display server.

## Feature Flags

| Feature | Default | Effect |
|---------|---------|--------|
| `dioxus` | off | Enables the optional `dioxus` dependency with the `minimal` feature set (`macro`, `html`, `signals`, `hooks`, `launch`) — all Pure Rust. The `desktop` feature (wry/tao/WebKit, C/C++ deps) is **excluded**. With this feature off, the crate has only `oxiui-core` as a dependency. |

## Errors

[`run_dioxus`] returns `Result<(), oxiui_core::UiError>`. It currently always returns [`UiError::Unsupported`] because the native Dioxus window path is not yet wired. Once the launch path lands it will return `Ok(())` on a clean exit, or [`UiError::Backend`] if the Dioxus runtime reports an error. `UiError` is `#[non_exhaustive]`; see [`oxiui-core`](../oxiui-core) for the full variant list.

## Palette mapping note

Dioxus renders via CSS-in-Rust (inline `style=""` attributes). The `palette` argument to [`run_dioxus`] is available for downstream consumers who format `style` strings from the palette colours; it is not automatically injected in M5. A helper `palette_to_css_vars()` is planned for M6 to emit `:root { --background: #rrggbb; … }` global CSS.

## Architecture note

Because Dioxus is reactive (not immediate-mode), the adapter operates in two phases:

1. **Collection pass** — the content closure is executed against a [`DioxusCtx`], accumulating widget descriptions in `items`.
2. **Render pass (M6)** — those items are translated into a Dioxus `rsx!`/`Element` tree and handed to `dioxus::launch()` running on the Pure-Rust `dioxus-native` renderer.

In M5 only the collection pass is active, which is what makes the crate headless-testable and example-buildable without a display or any C/C++ dependencies.

## Related Crates

| Crate | Role |
|-------|------|
| [`oxiui`](../oxiui) | Facade crate; select this adapter with `Backend::Dioxus` under `--features dioxus`. |
| [`oxiui-core`](../oxiui-core) | Defines `UiCtx`, `Theme`, `Palette`, `ButtonResponse`, and `UiError`. |
| [`oxiui-theme`](../oxiui-theme) | COOLJAPAN theme constructors (`cooljapan_dark`, `dark`, `light`) used as the `palette` argument. |
| [`oxiui-egui`](../oxiui-egui) | Default immediate-mode adapter (egui + wgpu). |
| [`oxiui-iced`](../oxiui-iced) | Retained-mode iced adapter. |
| [`oxiui-slint`](../oxiui-slint) | Slint adapter (same collection-mode pattern as this crate). |

[`DioxusCtx`]: https://docs.rs/oxiui-dioxus/latest/oxiui_dioxus/ctx/struct.DioxusCtx.html
[`run_dioxus`]: https://docs.rs/oxiui-dioxus/latest/oxiui_dioxus/fn.run_dioxus.html
[`UiError::Backend`]: https://docs.rs/oxiui-core/latest/oxiui_core/enum.UiError.html

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
