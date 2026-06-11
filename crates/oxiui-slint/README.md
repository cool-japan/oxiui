# oxiui-slint — Slint adapter for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-slint.svg)](https://crates.io/crates/oxiui-slint)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-slint` is the [Slint](https://slint.dev) backend adapter for OxiUI. It bridges OxiUI's immediate-mode `UiCtx` API onto the Slint GUI toolkit. The adapter is built around two pieces: [`SlintCtx`], a `UiCtx` implementation that collects each widget call into an ordered list, and [`run_slint`], a driver that executes a content closure against that context with a supplied theme.

Slint ships a Pure-Rust CPU renderer (`renderer-software`), so the adapter introduces **no GTK/Qt/C++ system widgets**. The crate itself is Apache-2.0 and is fully usable with `default = []` (collection mode), but enabling the `slint` feature transitively pulls in the `slint` crate, which is licensed under **GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0**. Downstream consumers must ensure their project's license is compatible with one of those three Slint options. Slint is only compiled when you explicitly request `--features slint`.

> **Milestone status:** As of M5, [`run_slint`] executes the content closure in headless collection mode (no display required) and returns `Ok(())`, even when the `slint` feature is enabled. Opening a native window via `slint::run_event_loop()` is deferred to M6 — see the notes below.

## Installation

```toml
[dependencies]
# Collection mode only — no Slint dependency, no GPL implications
oxiui-slint = "0.1.2"

# Enable Slint rendering (pulls in the GPL/royalty-free/commercial slint crate)
oxiui-slint = { version = "0.1.2", features = ["slint"] }
```

## Quick Start

### Collection mode (no `slint` feature, fully testable)

```rust
use oxiui_slint::SlintCtx;
use oxiui_core::UiCtx;

let mut ctx = SlintCtx::default();
ctx.heading("My Window");
ctx.label("Status: ok");
let resp = ctx.button("Continue");

assert_eq!(ctx.items.len(), 3);
assert_eq!(ctx.items[0], "heading:My Window");
assert!(!resp.clicked);
```

### Driving a content closure with a theme

```rust,ignore
use oxiui_slint::run_slint;
use oxiui_theme::cooljapan_dark;

run_slint(&*cooljapan_dark(), |ui| {
    ui.heading("Hello from Slint");
    ui.label("OxiUI + slint backend");
})
.expect("slint run failed");
```

## API Overview

### Types

| Item | Kind | Description |
|------|------|-------------|
| [`SlintCtx`] | struct | `UiCtx` adapter that records each widget call as a `"<kind>:<text>"` string in its public `items: Vec<String>` field. Derives `Debug` and `Default`. |

### Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| [`run_slint`] | `run_slint<F>(palette: &dyn oxiui_core::Theme, content: F) -> Result<(), UiError>` where `F: FnOnce(&mut dyn UiCtx)` | Runs one Slint-backed UI frame. Executes `content` against a `SlintCtx` in collection mode. Returns `Ok(())` in M5. |

### `SlintCtx` as a `UiCtx`

`SlintCtx` implements the three **required** `UiCtx` methods. All other `UiCtx` widget methods (`slider`, `checkbox`, `dropdown`, `text_input`, …) fall back to their default trait implementations, which report `supported == false` so callers can detect non-support and degrade gracefully.

| Method | Behaviour in `SlintCtx` |
|--------|-------------------------|
| `heading(&mut self, text)` | Pushes `"heading:<text>"` onto `items`. |
| `label(&mut self, text)` | Pushes `"label:<text>"` onto `items`. |
| `button(&mut self, label)` | Pushes `"button:<label>"` onto `items`; returns `ButtonResponse { clicked: false, hovered: false }` (collection mode never registers clicks). |

The `items` entries use the format `"<kind>:<text>"` (e.g. `"label:Hello"`, `"button:Quit"`), letting headless tests assert on the exact widget sequence without opening a window.

## Feature Flags

| Feature | Default | Effect |
|---------|---------|--------|
| `slint` | off | Enables the optional `slint` dependency (`renderer-software`, `compat-1-2`, `std`; no `backend-winit`). **Pulls in the GPL-3.0 OR royalty-free OR commercial Slint crate** — verify license compatibility. With this feature off, the crate has only `oxiui-core` as a dependency and is 100% Apache-2.0 Pure Rust. |

## Errors

[`run_slint`] returns `Result<(), oxiui_core::UiError>`. In M5 it is always `Ok(())`. From M6 onward it will return [`UiError::Backend`] if Slint's event loop reports an error. `UiError` is `#[non_exhaustive]`; see [`oxiui-core`](../oxiui-core) for the full variant list.

## Palette mapping note

Slint 1.16.1 exposes `slint::Color::from_argb_u8(a, r, g, b)` and per-component accessors (available under `renderer-software`, no `backend-winit` needed). However, Slint's global style/theme API in 1.16.1 does not expose a pluggable external-palette injection seam: the `StyleMetrics` struct is set internally and is not public. The `palette` argument to [`run_slint`] is therefore available for downstream consumers who build `slint::Color` values directly in their own components; it is not automatically applied to Slint's global style in M5. Full palette mapping is planned for M6 once a public API seam is confirmed.

## Related Crates

| Crate | Role |
|-------|------|
| [`oxiui`](../oxiui) | Facade crate; select this adapter with `Backend::Slint` under `--features slint`. |
| [`oxiui-core`](../oxiui-core) | Defines `UiCtx`, `Theme`, `Palette`, `ButtonResponse`, and `UiError`. |
| [`oxiui-theme`](../oxiui-theme) | COOLJAPAN theme constructors (`cooljapan_dark`, `dark`, `light`) used as the `palette` argument. |
| [`oxiui-egui`](../oxiui-egui) | Default immediate-mode adapter (egui + wgpu). |
| [`oxiui-iced`](../oxiui-iced) | Retained-mode iced adapter. |
| [`oxiui-dioxus`](../oxiui-dioxus) | Reactive Dioxus adapter (same collection-mode pattern as this crate). |

[`SlintCtx`]: https://docs.rs/oxiui-slint/latest/oxiui_slint/ctx/struct.SlintCtx.html
[`run_slint`]: https://docs.rs/oxiui-slint/latest/oxiui_slint/fn.run_slint.html
[`UiError::Backend`]: https://docs.rs/oxiui-core/latest/oxiui_core/enum.UiError.html

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
