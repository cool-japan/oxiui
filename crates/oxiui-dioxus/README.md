# oxiui-dioxus — Dioxus adapter for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-dioxus.svg)](https://crates.io/crates/oxiui-dioxus)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-dioxus` is the [Dioxus](https://dioxuslabs.com) backend adapter for OxiUI. Dioxus is a reactive, component-based framework: components are functions that return `Element` via the `rsx!` macro. This adapter bridges that retained, reactive model onto OxiUI's immediate-mode `UiCtx` closure API via two pieces: [`DioxusCtx`], a `UiCtx` implementation that collects each widget call into an ordered list, and [`run_dioxus`], a driver that executes a content closure against that context with a supplied theme.

Dioxus is dual-licensed **MIT OR Apache-2.0**, so this adapter carries no copyleft obligations. The adapter is configured to use Dioxus's `minimal` feature set (`macro`, `html`, `signals`, `hooks`, `launch`) — all Pure Rust. The `desktop` feature (wry/tao/WebKit/Chromium) is intentionally **excluded** because it pulls in C/C++ system dependencies that violate the COOLJAPAN Pure-Rust policy. The crate is fully usable with `default = []` (collection mode) and pulls in the `dioxus` crate only when you enable `--features dioxus`.

> **Milestone status:** As of M5, [`run_dioxus`] executes the content closure in headless collection mode (no display, no Dioxus runtime) and returns `Ok(())`. Full native rendering — translating collected items into an `rsx!` element tree and launching via `dioxus-native` (the Pure-Rust Blitz/Vello renderer) — is deferred to M6.

## Installation

```toml
[dependencies]
# Collection mode only — no Dioxus dependency
oxiui-dioxus = "0.1.1"

# Enable Dioxus rendering (minimal Pure-Rust feature set; no wry/tao/WebKit)
oxiui-dioxus = { version = "0.1.1", features = ["dioxus"] }
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

### Driving a content closure with a theme

```rust,ignore
use oxiui_dioxus::run_dioxus;
use oxiui_theme::cooljapan_dark;

run_dioxus(&*cooljapan_dark(), |ui| {
    ui.heading("Hello from Dioxus");
    ui.label("OxiUI + dioxus backend");
})
.expect("run_dioxus should be Ok");
```

## API Overview

### Types

| Item | Kind | Description |
|------|------|-------------|
| [`DioxusCtx`] | struct | `UiCtx` adapter that records each widget call as a `"<kind>:<text>"` string in its public `items: Vec<String>` field. Derives `Debug` and `Default`. |

### Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| [`run_dioxus`] | `run_dioxus<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>` where `F: FnOnce(&mut dyn UiCtx)` | Runs one Dioxus-backed UI frame. Executes `content` against a `DioxusCtx` in collection mode. Returns `Ok(())` in M5. |

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

[`run_dioxus`] returns `Result<(), oxiui_core::UiError>`. In M5 it is always `Ok(())`. From M6 onward it will return [`UiError::Backend`] if the Dioxus launch reports an error. `UiError` is `#[non_exhaustive]`; see [`oxiui-core`](../oxiui-core) for the full variant list.

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
