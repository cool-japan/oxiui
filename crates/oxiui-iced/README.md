# oxiui-iced — iced GUI framework adapter for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-iced.svg)](https://crates.io/crates/oxiui-iced)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-iced` is the **iced adapter** for OxiUI. It bridges OxiUI's backend-agnostic widget model to [`iced`] — the Elm-architecture, retained-mode Rust GUI framework. The adapter implements [`oxiui_core::UiCtx`] by *collecting* each widget call into a list of [`WidgetSpec`]s, then materialising an `iced::Element` (a vertical `Column`) for iced's `view` phase. It also converts OxiUI [`Palette`] / [`oxiui_theme::DesignTokens`] values into `iced::Theme` and per-widget styles.

iced is a **Pure Rust** GUI framework, so this adapter keeps the OxiUI stack C/C++-free. The central design challenge is the paradigm mismatch: iced is *retained-mode* (Elm-style `update`/`view` loop) whereas OxiUI's `UiCtx` is *immediate-mode* (per-frame closure). `oxiui-iced` resolves this with a widget-collection architecture — an [`IcedUiCtx`] gathers widget specs each frame, an [`IcedConfig`] carries frame-to-frame state, and [`apply_message`] advances that state in your `update` function. `#![forbid(unsafe_code)]` is enforced crate-wide.

## Installation

```toml
[dependencies]
oxiui-iced = "0.1.3"
```

## Quick Start

Collect OxiUI widget calls and build an `iced::Element`:

```rust
use oxiui_core::UiCtx;
use oxiui_iced::{IcedConfig, IcedUiCtx};

let config = IcedConfig::default().with_spacing(8.0);
let mut ctx = IcedUiCtx::new(config);

ctx.heading("OxiUI on iced");
ctx.label("Immediate-mode calls, collected into a retained-mode tree.");
let _ = ctx.button("Click me");

assert_eq!(ctx.spec_count(), 3);
let _element = ctx.into_iced_element(); // iced::Element<'static, Message>
```

### Advancing state in your `update` function

```rust
use std::collections::{HashMap, HashSet};
use oxiui_iced::{apply_message, Message, WidgetState};

let mut state: HashMap<usize, WidgetState> = HashMap::new();
let mut clicks: HashSet<usize> = HashSet::new();

// On receiving an iced message:
apply_message(&mut state, &mut clicks, &Message::ButtonPressed(0));
assert!(clicks.contains(&0));

// Feed `state`/`clicks` back into the next frame's IcedConfig.
```

### Theming from an OxiUI palette

```rust
use oxiui_core::{Color, Palette};
use oxiui_iced::palette_to_iced_theme;

let palette = Palette::new(
    Color(255, 255, 255, 255), // background
    Color(240, 240, 240, 255), // surface
    Color(0, 100, 200, 255),   // primary
    Color(255, 255, 255, 255), // on_primary
    Color(0, 0, 0, 255),       // text
    Color(128, 128, 128, 255), // muted
);
let _theme: iced::Theme = palette_to_iced_theme(&palette);
```

## API Overview

### `adapter` module

#### `IcedUiCtx` — the `UiCtx` implementation

Collects widget operations into [`WidgetSpec`]s, then builds an iced widget tree on demand.

| Method | Description |
|--------|-------------|
| `IcedUiCtx::new(config)` | Construct from an [`IcedConfig`] (pre-allocates the spec vector) |
| `spec_count()` | Number of widget specs collected so far |
| `into_specs() -> Vec<WidgetSpec>` | Consume, returning the raw spec list |
| `into_iced_element() -> Element<'static, Message>` | Build the iced `Column` widget tree |

Implements [`oxiui_core::UiCtx`] widgets including `heading`, `label`, `button`, `text_input`, `checkbox`, `slider`, `dropdown`, `image`, `separator`, `spacer`, scroll/layout containers, `tooltip`, `popup`, `modal`, `grid`, and `rich_text` — each pushing a [`WidgetSpec`].

#### State and configuration

| Item | Description |
|------|-------------|
| `IcedConfig` | Frame-to-frame state: `pending_clicks`, `state`, `spacing`, `padding`, `title`, `spec_capacity_hint`. Builders: `with_spacing`, `with_padding`, `with_title`, `with_spec_capacity` |
| `Message` | Bridge messages: `ButtonPressed`, `TextChanged`, `CheckboxToggled`, `SliderChanged`, `DropdownSelected` |
| `WidgetState` | Per-widget retained state: `Text`, `Checked`, `Slider`, `Selected` |
| `apply_message(&mut state, &mut clicks, &msg)` | Advance retained state from a received [`Message`] |

#### Widget specs and caching

| Item | Description |
|------|-------------|
| `WidgetSpec` | The collected widget descriptor enum (heading, label, button, text-input, checkbox, slider, dropdown, image, separator, spacer, scroll, tooltip, popup, modal, horizontal, vertical, grid, rich-text) |
| `IcedSpan` | A styled rich-text span (`text`, `color`, `bold`, `size`) |
| `spec_fingerprint(&WidgetSpec) -> u64` | Stable hash of a spec for change detection |
| `SpecCache` | Tracks the previous frame's spec fingerprints; `sync(&specs) -> bool` (true if rebuild needed), `rebuild_count()` |
| `ThemeCache` | Lazy `palette → iced::Theme` cache; `get_or_compute(&palette)` |

#### Standalone widget wrapper and helpers

| Item | Description |
|------|-------------|
| `OxiIcedWidget` | Wraps a single [`WidgetSpec`] as a standalone widget; `new(spec)`, `spec()`, `width(len)`, `height(len)` |
| `oxi_widget(spec) -> OxiIcedWidget` | Convenience constructor |
| `IcedNullCtx` | A `UiCtx` that records specs without rendering (for testing); `recording()` |
| `map_iced_key(&iced::keyboard::Key) -> oxiui_core::events::Key` | Map an iced key to an OxiUI key |
| `map_iced_modifiers(iced::keyboard::Modifiers) -> oxiui_core::events::Modifiers` | Map iced modifiers to OxiUI modifiers |
| `map_iced_keyboard_event(&iced::keyboard::Event) -> Option<oxiui_core::UiEvent>` | Map an iced keyboard event into an OxiUI event |

### `theme` module

| Function / Item | Description |
|-----------------|-------------|
| `palette_to_iced_theme(&Palette) -> iced::Theme` | Convert an OxiUI palette to an iced theme |
| `palette_to_iced_theme_ext(&dyn Theme) -> iced::Theme` | Convert from any OxiUI `Theme` trait object |
| `palette_and_tokens_to_iced_theme(...)` | Combine palette and design tokens into an iced theme |
| `text_input_style_from_palette(&Palette) -> text_input::Style` | Per-widget text-input style from a palette |
| `scrollable_style_from_palette(&Palette) -> scrollable::Style` | Per-widget scrollable style from a palette |
| `text_input_style_from_theme(&dyn Theme)` / `scrollable_style_from_theme(&dyn Theme)` | Same, from a `Theme` trait object |
| `DesignTokensAdapter` | Token-derived sizing (`border_radius`, `body_font_size`, `headline_font_size`, `base_spacing`); `from_tokens`, `body_text_size`, `headline_text_size`, `standard_padding` |

### Event forwarding (crate root)

| Function | Description |
|----------|-------------|
| `forward_ime_event(&UiEvent)` | Best-effort IME forwarding stub (see note below) |

## Feature Flags

This crate exposes no Cargo features; `default` is empty. The `iced` dependency is built with its `image` and `advanced` features.

## Errors

The adapter surfaces results through OxiUI response types and the shared [`oxiui_core::UiError`]; see the [`oxiui-core`](../oxiui-core) error table for the full variant list.

## Adapter Notes and Deviations

The adapter targets iced 0.14; a few mappings are best-effort and documented inline:

- **Immediate → retained bridge.** `IcedUiCtx` collects `WidgetSpec`s per frame and builds a `Column` in `into_iced_element`; the message round-trip is wired through [`Message`] and [`apply_message`].
- **IME (CJK).** iced 0.14 exposes no public per-widget IME injection API, so [`forward_ime_event`] no-ops on `ImePreedit` / `ImeCommit`. Full IME support awaits an upstream iced API.
- **Slider precision.** OxiUI's `UiCtx::slider` uses `f64`; values are cast to `f32` at the iced widget boundary and re-widened on message receipt (`Message::SliderChanged(usize, f64)`).
- **Window title.** `IcedConfig::title` is a configuration seam a host `iced::Application::title` callback can read; `oxiui-iced` itself does not host an `iced::Application`.

## Related Crates

- [`oxiui`](../../) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `UiCtx`, `Palette`, `UiEvent`, `UiError`, event and response types.
- [`oxiui-theme`](../oxiui-theme) — `DesignTokens`, `TypographyScale`, the `Theme` trait.
- [`oxiui-egui`](../oxiui-egui) — the alternative egui/eframe framework adapter.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
