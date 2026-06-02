# oxiui-theme — COOLJAPAN dark/light themes for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-theme.svg)](https://crates.io/crates/oxiui-theme)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-theme` supplies the visual identity of the COOLJAPAN OxiUI toolkit: a dark/light **Tokyo Night** palette plus the full design-system scaffolding around it — design tokens (spacing / radius / elevation), a typographic scale, a CSS-subset stylesheet engine with selector matching and specificity, animation tokens, responsive breakpoints, semantic-palette derivation, a runtime theme manager with change listeners, high-contrast accessibility variants, and a gallery of popular ready-made palettes (Nord, Dracula, Solarized, Catppuccin, Material).

Everything is expressed in terms of `oxiui-core`'s `Theme`, `Palette`, `Color`, and `FontSpec` types, so any adapter that consumes a `&dyn Theme` works with these themes unchanged. The crate is `#![forbid(unsafe_code)]` and 100% Pure Rust — its only dependency is `oxiui-core`.

## Installation

```toml
[dependencies]
oxiui-theme = "0.1.0"
```

## Quick Start

```rust
use oxiui_core::Theme;
use oxiui_theme::{cooljapan_default, ThemeExt};

// The default COOLJAPAN theme is Tokyo Night (dark).
let theme = cooljapan_default();
assert_eq!(theme.palette().background.0, 26); // #1A1B26

// ThemeExt augments any Theme with tokens, typography, and the
// extended semantic palette (blanket-implemented for every Theme).
let tokens = theme.tokens();
let scale = theme.typography();
let extended = theme.extended_palette();
let _ = (tokens, scale, extended);
```

### Building a validated custom theme

```rust
use oxiui_core::Color;
use oxiui_theme::PaletteBuilder;

let result = PaletteBuilder::new()
    .background(Color(26, 27, 38, 255))
    .surface(Color(36, 40, 59, 255))
    .primary(Color(122, 162, 247, 255))
    .on_primary(Color(26, 27, 38, 255))
    .text_primary(Color(192, 202, 245, 255))
    .text_secondary(Color(86, 95, 137, 255))
    .build(); // -> Result<CooljapanTheme, Vec<ContrastWarning>>
let _ = result;
```

## API Overview

### Theme construction (crate root)

| Item | Description |
|------|-------------|
| `cooljapan_default()` | The default theme: dark / Tokyo Night |
| `dark()` | Tokyo Night dark theme |
| `light()` | COOLJAPAN light theme |
| `CooljapanTheme` | Concrete `Theme` impl: a `Palette` + a `FontSpec`; `new(palette, font)` |

All three constructors return `Box<dyn Theme>`.

### `ThemeExt` trait

Blanket-implemented for every `oxiui_core::Theme`, returning COOLJAPAN defaults; concrete themes may override.

| Method | Description |
|--------|-------------|
| `tokens()` / `design_tokens()` | Design-token scale (by value / by `'static` reference) |
| `typography()` / `typography_ref()` | Typographic scale (by value / by reference) |
| `is_high_contrast()` | Whether this theme targets high-contrast display |
| `effective_palette()` | Palette with contrast boosted when the OS requests high contrast |
| `extended_palette()` | Derived semantic `ExtendedPalette` (dark inferred from background luminance) |

### OS accessibility helpers

`os_prefers_high_contrast()` and `os_prefers_reduced_motion()` read the `OXIUI_HIGH_CONTRAST` / `OXIUI_REDUCED_MOTION` environment variables (`"1"`/`"true"` = active).

### `tokens` module — design tokens

`DesignTokens` — `spacing(SpacingStep)`, `radius(RadiusStep)`, `elevation(level)`. `SpacingStep` and `RadiusStep` are stepped scales.

### `typography` module

`TypographyScale` — `roles_descending()` yields six `TextStyleToken`s (heading → caption). `TextStyleToken` carries a per-role text style.

### `spec` module — borders & shadows

| Item | Description |
|------|-------------|
| `BorderSpec` / `BorderSpecs` | Single / per-side border specs; `is_invisible`, `is_uniform` |
| `BorderStyle` | Line style enum |
| `ShadowSpec` | Box-shadow spec; `new`, `drop_shadow`, `with_spread`, `with_inset`, `to_pixel_color`, `is_invisible` |
| `elevation_to_shadow(elevation)` | Map a continuous elevation to a `ShadowSpec` |
| `elevation_shadow(level)` | Map a discrete level to `Option<ShadowSpec>` |
| `elevation_shadows(elevation)` | Layered shadow stack for an elevation |

### `stylesheet` module — CSS-subset engine

| Item | Description |
|------|-------------|
| `StyleSheet` | Parse + match CSS-subset rules: `parse`, `matching_rules`, `compute_style` |
| `Rule`, `Selector`, `SelectorPart` | Parsed rule + selector model |
| `Specificity(u32, u32, u32)` | ID/class/type specificity; `add_id`, `add_class`, `add_type` |
| `ComputedStyle` | The resolved style for an element |
| `CssValue` | A parsed CSS value |
| `ParseResult` / `ParseDiagnostic` | Parse output + diagnostics |

### `compile` module

`CompiledStyleSheet` — pre-compiled stylesheet for fast repeated styling; `compile(sheet, generation)`, `compute_style`.

### `inheritance` module

`resolve(parent, child)` (re-exported as `resolve_inheritance`) — propagate inherited CSS properties from a parent `ComputedStyle` into a child.

### `style_cache` module

`StyleCache` — memoises computed styles; `get_or_compute`.

### `anim_tokens` module — animation tokens

| Item | Description |
|------|-------------|
| `TransitionSpec` | A reusable transition; presets `fade_in()`, `slide_in()`, `scale_up()` |
| `AnimationSpec` / `AnimationKeyframe` | Keyframe animation model |
| `EasingKind` | Named easing curve |
| `FillMode` / `IterationCount` | Animation fill mode and iteration count |

### `breakpoint` module — responsive design

`Breakpoint` — responsive size tiers; `for_width(width)`, `min_width`, `matches_width`.

### `manager` module — runtime switching

`ThemeManager` — holds the active `CooljapanTheme` and notifies subscribers on change: `new`, `theme`, `set_theme`, `subscribe`, `unsubscribe`, `listener_count`. `ThemeListener` is the boxed callback type.

### `overlay` module

`PartialTheme` + `overlay(base, overrides)` — layer partial overrides on top of a base `Theme` to produce a derived `CooljapanTheme`.

### `palette_ext` module

`ExtendedPalette` — semantic palette derived from a base `Palette`; `derive(base, dark)` adds status/hover/etc. colours.

### `lazy_palette` module

`LazyPaletteVariants` — lazily-computed interaction-state variants from a base colour: `hover`, `pressed`, `disabled`.

### `icons` module

| Item | Description |
|------|-------------|
| `IconSet` (trait) | Supplies SVG path-data for named icons |
| `BuiltinIcons` | The built-in icon set |
| `IconName` | `Close`, `Menu`, `ArrowRight/Left/Up/Down`, `Check`, `Search` |
| `IconVariant` | `Outline`, `Filled`, `Rounded` |

### `high_contrast` module

`cooljapan_high_contrast()` / `cooljapan_high_contrast_light()` — accessible high-contrast palettes. Plus WCAG helpers `wcag_luminance(r, g, b)` and `wcag_contrast(fg, bg)`.

### `builder` module — validated palette builder

`PaletteBuilder` — fluent builder with WCAG contrast validation: `background`, `surface`, `text_primary`, `text_secondary`, `primary`, `on_primary`, `validate() -> ValidationResult`, `build() -> Result<CooljapanTheme, Vec<ContrastWarning>>`. Includes `WcagLevel` and `ContrastWarning`.

### `color` module — colour utilities

Free functions over `oxiui_core::Color`: `lerp`, `mix`, `lighten`, `darken`, `with_alpha`, `scale_alpha`, `to_hsl`/`from_hsl`, `saturate`/`desaturate`, `to_oklch`/`from_oklch`, `oklch_lerp`, `best_contrast`.

### `gallery` module — ready-made palettes

`make_nord_dark`, `make_nord_light`, `make_dracula`, `make_solarized_dark`, `make_solarized_light`, `make_catppuccin_mocha`, `make_catppuccin_latte`, `make_material_dark`, `make_material_light` — each returns a `CooljapanTheme`.

## Tokyo Night palette (default dark)

| Role | Hex | RGB |
|------|-----|-----|
| `background` | `#1A1B26` | (26, 27, 38) |
| `surface` | `#24283B` | (36, 40, 59) |
| `primary` | `#7AA2F7` | (122, 162, 247) |
| `on_primary` | `#1A1B26` | (26, 27, 38) |
| `text` | `#C0CAF5` | (192, 202, 245) |
| `muted` | `#565F89` | (86, 95, 137) |

Font: Inter, 14 pt, weight 400.

## Related crates

- [`oxiui-core`](https://crates.io/crates/oxiui-core) — defines `Theme`, `Palette`, `Color`, `FontSpec` (the only dependency)
- [`oxiui`](https://crates.io/crates/oxiui) — the OxiUI facade
- [`oxiui-text`](https://crates.io/crates/oxiui-text) — text layer whose `FontSpec` comes from a theme
- [`oxiui-accessibility`](https://crates.io/crates/oxiui-accessibility) — pairs with the high-contrast palettes here

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
