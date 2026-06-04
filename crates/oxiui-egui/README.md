# oxiui-egui — egui/eframe adapter for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-egui.svg)](https://crates.io/crates/oxiui-egui)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-egui` is the **egui/eframe adapter** for OxiUI. It bridges OxiUI's backend-agnostic widget model to [`egui`] — the popular immediate-mode GUI library — and its [`eframe`] application shell. The adapter implements [`oxiui_core::UiCtx`] in terms of an `egui::Ui`, converts an OxiUI [`Palette`] / [`oxiui_theme::DesignTokens`] into `egui::Visuals` / `egui::Style`, loads OxiFont bytes into egui's font system, and forwards OxiUI [`UiEvent`]s into egui's input queue.

Both egui and eframe are **Pure Rust** GUI frameworks, so this adapter keeps the OxiUI stack C/C++-free. (eframe's default rendering goes through `wgpu`, the Rust graphics boundary, which dispatches to OS-provided GPU drivers at runtime.) Because egui is immediate-mode while OxiUI's `UiCtx` is also immediate-mode, the mapping is direct: each `UiCtx` call forwards straight to its egui equivalent. `#![forbid(unsafe_code)]` is enforced crate-wide.

## Installation

```toml
[dependencies]
oxiui-egui = "0.1.1"
```

## Quick Start

Wrap an `egui::Ui` as an OxiUI [`UiCtx`] and render OxiUI widgets directly:

```rust,no_run
use oxiui_core::UiCtx;
use oxiui_egui::EguiUiCtx;

fn draw(ui: &mut egui::Ui) {
    let mut ctx = EguiUiCtx::new(ui);
    ctx.heading("OxiUI on egui");
    ctx.label("Backend-agnostic widgets, rendered by egui.");
    let response = ctx.button("Click me");
    if response.clicked {
        // handle click
    }
}
```

### Applying an OxiUI palette to an egui context

```rust,no_run
use oxiui_core::{Color, Palette};
use oxiui_egui::palette_to_egui_visuals;

fn theme(ctx: &egui::Context, palette: &Palette) {
    ctx.set_visuals(palette_to_egui_visuals(palette));
}
```

### Caching adapter for an eframe app

```rust,ignore
use oxiui_egui::StatefulEguiAdapter;

let mut adapter = StatefulEguiAdapter::new()
    .with_palette(my_palette)
    .with_design_tokens(my_tokens, my_typography)
    .with_font_bytes(font_data);

// In your eframe::App::update():
adapter.apply(ctx); // cheap after the first frame if the theme is unchanged
```

## API Overview

### `EguiUiCtx<'a>` — the `UiCtx` implementation

Wraps an `&mut egui::Ui` and implements [`oxiui_core::UiCtx`], forwarding each widget call to its egui equivalent.

| Method | Description |
|--------|-------------|
| `EguiUiCtx::new(ui)` | Wrap an egui `Ui` reference |
| `response()` | The egui `Response` of the most recently rendered widget |
| `clipboard_get()` | Read the most-recently-copied text from egui's output queue (intra-frame) |
| `clipboard_set(text)` | Queue a copy-to-clipboard command via egui |

Implemented `UiCtx` widgets (all forwarding to egui): `heading`, `label`, `button`, `text_input`, `checkbox`, `slider`, `dropdown`, `image`, `separator`, `spacer`, `scroll_area`, `tooltip`, `popup`, `modal`, `horizontal`, `vertical`, `grid`, `menu_bar`, `rich_text`, `drag_source`, `drop_target`.

### Theme conversion functions

| Function | Description |
|----------|-------------|
| `palette_to_egui_visuals(&Palette) -> egui::Visuals` | Map an OxiUI palette to an egui colour scheme |
| `palette_to_egui_visuals_with_tokens(&Palette, &DesignTokens) -> egui::Style` | Palette visuals plus token-driven spacing and corner radius |
| `tokens_to_egui_style(&DesignTokens, &TypographyScale) -> egui::Style` | Full token + typography mapping to all five `egui::TextStyle` variants |

### Font loading

| Function | Description |
|----------|-------------|
| `load_font_into_egui(&egui::Context, Vec<u8>) -> Result<(), UiError>` | Validate and install a single font as the `"OxiFont"` family |
| `load_fonts_into_egui(&[(&str, Vec<u8>)], &egui::Context) -> Result<(), UiError>` | Install multiple named `"OxiFont-<name>"` families |

Both validate font bytes via [`oxiui_text::TextPipeline::from_bytes`] and return [`UiError::Render`] on invalid input.

### Event forwarding

| Function | Description |
|----------|-------------|
| `forward_event_to_egui(&egui::Context, &UiEvent)` | Forward an OxiUI [`UiEvent`] (IME, keyboard, pointer, resize) into egui's input queue |

Handled families: IME (`ImePreedit`, `ImeCommit`), keyboard (`KeyDown`, `KeyUp`, `KeyPress`), pointer (`MouseMove`, `Mouse`, `MouseDown`, `MouseUp`), and `Resize` (noted only — egui resize is driven by `RawInput.screen_rect`). Other variants are silently ignored.

### Widget bridge

| Item | Description |
|------|-------------|
| `OxiWidget<'a>` | Wraps a `&mut dyn oxiui_core::Widget` so it can be placed in an egui layout via `ui.add(...)`; implements `egui::Widget`. `OxiWidget::new(widget)` |

### Adapters

| Item | Description |
|------|-------------|
| `EguiAdapter` | Stateless builder: `new()`, `with_palette(p)`, `build() -> impl Fn(&egui::Context)` |
| `StatefulEguiAdapter` | Per-frame caching adapter (see below) |

#### `StatefulEguiAdapter`

Caches expensive operations across frames so repeated `apply` calls are cheap when the theme is stable.

| Method | Description |
|--------|-------------|
| `new()` | Empty adapter |
| `with_palette(p)` | Set the palette (visuals recomputed only on change) |
| `with_design_tokens(tokens, typography)` | Apply a token-driven style once on the first frame |
| `with_font_bytes(bytes)` | Load fonts once on the first frame |
| `set_palette(p)` | Update the live palette (marks visuals stale) |
| `apply(&egui::Context)` | Apply state for one frame (fonts/tokens once, visuals on change) |
| `visuals_recompute_count` (field) | Number of times visuals were recomputed (instrumentation) |
| `fonts_load_count` (field) | Number of `set_fonts` calls (≤ 1 in normal use) |

## Feature Flags

This crate exposes no Cargo features; `default` is empty.

## Errors

Functions that can fail return [`oxiui_core::UiError`]. Font-loading helpers use the `UiError::Render` variant for invalid font bytes; see the [`oxiui-core`](../oxiui-core) error table for all variants.

## Adapter Notes and Deviations

The adapter is faithful to egui 0.34's API; a few mappings are approximate and documented inline:

- `Key::Character` / `Key::Named` are forwarded via `egui::Key::from_name`; unrecognised names fall back to `egui::Key::F12`.
- `rich_text` honours span colour, size, and italics; `egui` 0.34's `TextFormat` has no per-span bold field, so bold spans render at the default weight.
- `Palette` carries no error/warning/success colours (those live on `oxiui_theme::ExtendedPalette`), so egui's `warn_fg_color` / `error_fg_color` keep their defaults.

## Related Crates

- [`oxiui`](../../) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `UiCtx`, `Widget`, `Palette`, `UiEvent`, `UiError`, response types.
- [`oxiui-theme`](../oxiui-theme) — `DesignTokens`, `TypographyScale`, theming primitives.
- [`oxiui-text`](../oxiui-text) — font validation used by the font loaders.
- [`oxiui-render-wgpu`](../oxiui-render-wgpu) — the wgpu render path eframe uses by default.
- [`oxiui-iced`](../oxiui-iced) — the alternative iced framework adapter.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
