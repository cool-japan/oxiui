# oxiui-theme TODO

## Active /ultra plan (2026-05-29 — Slice F complete)
Slice F items implemented and verified. Every `[x]` item has passing tests.

## Status
Theming crate (~3.7 kLOC, 16 source files). Provides `CooljapanTheme`
implementing the `Theme` trait, dark (Tokyo Night) / light / high-contrast
(dark + light AAA) palettes, an `ExtendedPalette` with status + on-surface
roles, `DesignTokens` (spacing/radius/elevation/opacity), `TypographyScale`,
`BorderSpec`/`BorderSpecs` (per-side, with `BorderStyle::Double`),
`ShadowSpec` + `elevation_shadow` (single) + `elevation_shadows` (ambient+key pair),
perceptual colour utilities (Oklch lerp via `oklch_lerp`, HSL
`saturate`/`desaturate`, `to_hsl`/`from_hsl`, `to_oklch`/`from_oklch`),
a fluent `PaletteBuilder` with WCAG `validate()` and `ContrastWarning`,
theme composition via `overlay()` + `PartialTheme`,
a `ThemeManager` observer for runtime switching,
animation tokens (`TransitionSpec` / `AnimationSpec` / `EasingKind` /
`fade_in` / `slide_in` / `scale_up` presets), a predefined gallery
(Nord dark/light, Dracula, Solarized dark/light, Catppuccin Mocha/Latte,
Material dark/light), a hand-written CSS-subset `StyleSheet` parser with
specificity cascade, CSS `inheritance::resolve` for inheritable/non-inheritable
properties, `Breakpoint` (xs/sm/md/lg/xl/xxl) responsive thresholds, and
`IconSet` + `BuiltinIcons` with hand-authored SVG path-data for 8 icons ×
4 sizes × 3 variants. 115 tests, 0 warnings.

## Core Implementation
- [x] High-contrast COOLJAPAN palette: WCAG AAA compliant (contrast ratio > 7.0 on all text/background pairs), `cooljapan_high_contrast()` constructor, both dark-HC and light-HC variants (~80 SLOC)
- [x] Extended `Palette`: add `error`, `warning`, `success`, `info` semantic colors, `surface_variant`, `outline`, `shadow` colors, `on_surface`, `on_background` text-on-surface colors (~40 SLOC)
- [x] Design token system: `DesignTokens` struct with spacing scale (4/8/12/16/24/32/48/64px), border-radius scale (none/sm/md/lg/xl/full), elevation/shadow levels (0-5), opacity levels (~120 SLOC)
- [x] Typography scale: `TypographyScale` with named sizes (display/headline/title/body/caption/overline), each with font-size, line-height, letter-spacing, weight (~100 SLOC)
- [x] Border specification: `BorderSpec` with width, style (solid/dashed/dotted/none), color, per-side overrides via `BorderSpecs` (~60 SLOC)
- [x] Shadow specification: `ShadowSpec` with offset-x, offset-y, blur-radius, spread-radius, color, inset flag; elevation presets mapping to shadow stacks (ambient + key) (~80 SLOC)
- [x] CSS-like style sheets: `StyleSheet` parser that reads a simplified CSS subset (selectors by widget type/class/id, properties for color/spacing/border/font), cascading specificity resolution (~500 SLOC)
    - **Goal:** add a declarative styling layer over existing palette/token/typography. All pure Rust, no parser/SVG deps. Crate-local.
    - **Design:** `StyleSheet` hand-written recursive-descent parser for CSS subset (type/.class/#id selectors + compound, property block, grouped selectors). `Specificity{id,class,type}` lexicographic cascade with last-wins-at-equal-specificity → `ComputedStyle`. `inherit`/`initial`/`unset` keywords; inheritable props (font,color) flow parent→child, non-inheritable (spacing,border) reset; `resolve(parent,own)->ComputedStyle`. `Breakpoint{Xs,Sm,Md,Lg,Xl,Xxl}` with pixel thresholds + `breakpoint_for(width)->Breakpoint`. `IconSet` trait + `BuiltinIcons` with hand-authored SVG path-data string constants (close/menu/arrow/check/search, sizes 16/20/24/32, variants outline/filled/rounded) — NO SVG-parsing dep. Malformed input → Result/skip-with-diagnostic, never panic.
    - **Files:** new `src/{stylesheet.rs,inheritance.rs,breakpoint.rs,icons.rs}`; `lib.rs` re-exports.
    - **Tests:** parse simple/compound/grouped selectors; property parsing; cascade+specificity; inherit/initial/unset; inheritable vs non-inheritable; breakpoint_for thresholds; IconSet::path_data non-empty for all builtins×variants×sizes.
    - **Defer:** theme serialization via oxicode (needs dep); stylesheet-compilation lookup table (follow-up to parser).
- [x] Style inheritance: parent-to-child style propagation (font, color inheritable by default; spacing, border not inheritable), `inherit`/`initial`/`unset` keywords (~100 SLOC)
- [x] Responsive breakpoints: `Breakpoint` enum (xs/sm/md/lg/xl/xxl) with pixel thresholds, `@media`-like conditional styling per breakpoint (~80 SLOC)
- [x] Animation tokens: `TransitionSpec` with property name, duration, easing, delay; `AnimationSpec` with keyframes; token library of standard UI transitions (fade-in/slide-in/scale-up) (~120 SLOC)
- [x] Runtime theme switching: `ThemeManager` that holds the active theme, notifies listeners on change (data-only; smooth-animated transitions live in render layer) (~100 SLOC)
- [x] Theme serialization: save/load themes to/from a Pure Rust format via oxicode; user-customizable theme files (~80 SLOC) — implemented in `src/serial.rs` using `oxicode::Encode + oxicode::Decode` derives on `ThemeSnapshot` (wraps `DesignTokens` + `TypographyScale`); public API: `serialize_theme` / `deserialize_theme`
- [x] Color utilities: contrast ratio calculator (WCAG formula), color interpolation (Oklch lerp), lighten/darken/saturate/desaturate operations, alpha compositing (~150 SLOC)
- [x] Predefined theme gallery: Material-style, Nord, Dracula, Solarized, Catppuccin palettes as opt-in constructors (~200 SLOC, ~40 each)
- [x] Icon theme: `IconSet` trait for themed icon sets (outline/filled/rounded), SVG path data for common icons (close/menu/arrow/check/search), size variants (16/20/24/32) (~200 SLOC)

## API Improvements
- [x] `Theme` trait: add `tokens() -> DesignTokens`, `typography() -> TypographyScale`, `shadows() -> Vec<ShadowSpec>` methods (via `ThemeExt`)
- [x] `Palette` builder: `PaletteBuilder::new().background(Color(..)).primary(Color(..)).build()` + `validate()` with WCAG contrast warnings (re-exported from `oxiui_core::color_space`)
- [x] Make `CooljapanTheme` public; allow users to construct custom themes with the same API
- [x] Theme composition: `OverlayTheme::new(base).palette(p).font(f)` for partial palette/font customization without full redefinition
- [ ] `#[derive(Clone)]` for themes; currently returns `Box<dyn Theme>` which is not cloneable **BLOCKED: `Theme` trait lives in `oxiui-core`; would need an `Arc<dyn Theme>` migration across the entire workspace — API-breaking change requiring cross-crate coordination**

## Testing
- [x] WCAG contrast ratio tests: verify all palette combinations meet minimum AA (4.5:1) or AAA (7.0:1) thresholds (~60 SLOC)
- [x] High-contrast palette tests: every text/background pair exceeds 7.0 contrast ratio — for BOTH dark-HC and light-HC variants (~40 SLOC)
- [x] Typography scale tests: verify monotonic size ordering (caption < body < title < headline < display) (~30 SLOC)
- [x] Design token tests: spacing scale values are all multiples of 4, border-radius values are non-negative (~30 SLOC)
- [x] Style sheet parser tests: simple selectors, compound selectors, property parsing, cascading order, specificity tiebreaker (~150 SLOC)
- [x] Runtime theme switching tests: switch dark→light, verify all palette values changed, listener notification fired (~40 SLOC)
- [x] Theme serialization round-trip tests: serialize to oxicode, deserialize, compare equality (~40 SLOC) — 5 round-trip tests in `src/serial.rs` covering default snapshot, custom tokens, named-step lookup, typography field access, and invalid-bytes error handling
- [x] Color utility tests: known contrast ratios (black/white = 21:1), interpolation endpoints, lighten/darken bounds, Oklch lerp, HSL saturate (~60 SLOC)

## Performance
- [x] **Theme: stylesheet compilation (O(1) lookup), property-lookup caching, lazy palette computation** (implemented 2026-05-29)
  - **Goal:** add the perf layer over round-2's CSS-subset StyleSheet parser — no new deps.
  - **Design:**
    - Stylesheet compilation: pre-compile parsed rules into CompiledStyleSheet lookup keyed by (type,class,id) buckets so widget→style resolution is ~O(1) instead of re-running the cascade; preserve specificity ordering inside each bucket.
    - Property-lookup caching: StyleCache memoizing resolved ComputedStyle per (widget-type-key, stylesheet-generation), invalidated on stylesheet change.
    - Lazy palette computation: derive hover/pressed/disabled variants lazily via OnceCell-style LazyPaletteVariants cache instead of eagerly.
  - **Files:** `src/{stylesheet.rs,lib.rs}`; new `src/compile.rs` + `src/style_cache.rs`. Keep stylesheet.rs (653) under 2000.
  - **Prerequisites:** none (parser already landed).
  - **Tests (~9):** compiled lookup returns same ComputedStyle as uncompiled cascade for simple/compound/grouped selectors; specificity tiebreak preserved post-compile; cache hit identical; cache invalidates on stylesheet change; lazy variant computed once (spy) + cached.
  - **Risk:** compiled path must be observably equivalent to cascade — test equivalence directly. Defer: theme serialization via oxicode (dep decision), derive(Clone) themes (Arc<dyn Theme> migration — API decision).
- [x] Style sheet compilation: pre-compile selector matching into a lookup table for O(1) widget-to-style resolution
- [x] Lazy palette computation: derive secondary colors (hover/pressed/disabled variants) lazily and cache

## Integration
- [x] `oxiui-core` integration: `Theme` trait expanded to include `DesignTokens` and `TypographyScale`; `Layout` trait should consume spacing tokens
- [x] `oxiui-egui` integration: map `DesignTokens` (spacing, border-radius, shadows) to egui `Style`/`Spacing`/`Rounding` structs, not just `Visuals` — `palette_to_egui_visuals_with_tokens` + `tokens_to_egui_style` in `oxiui-egui/src/lib.rs`
- [x] `oxiui-iced` integration: map `DesignTokens` to iced `Container::Style`, `Button::Style`, etc.; extend `palette_to_iced_theme` to cover the expanded palette — `DesignTokensAdapter` + `palette_and_tokens_to_iced_theme` + `text_input_style_from_palette` + `scrollable_style_from_palette` in `oxiui-iced/src/theme.rs`
- [x] `oxiui-render-wgpu` integration: shadow rendering uses `ShadowSpec` token values; gradient stops from theme color scale — `theme_bridge` module gated behind `theme` feature in `oxiui-render-wgpu` (2026-06-03)
- [x] `oxiui-render-soft` integration: CPU shadow rendering from `ShadowSpec`
- [x] `oxiui-accessibility` integration: high-contrast theme auto-detected and applied when OS accessibility preference is set
- [x] COOLJAPAN ecosystem: theme serialization via oxicode (not bincode); color math via pure Rust (no OxiBLAS needed for color ops)
  - **Goal:** `ThemeSnapshot` round-trips the full theme — design tokens, typography, colour palette, and fonts — through oxicode. Blocker (missing core derives) removed by building the prerequisite in `oxiui-core`. Completes this item.
  - **Design:** (1) `oxiui-core`: add `oxicode.workspace = true` to Cargo.toml; derive `oxicode::Encode, oxicode::Decode` on `Color`, `Palette`, `FontSpec`, `FontStyle` (enum with Oblique{degrees:f32} struct-variant — supported), `FontFeature`; add `Default + PartialEq` to `Palette` (hand-written sensible default, required by ThemeSnapshot). (2) `oxiui-theme/src/serial.rs`: extend `ThemeSnapshot` with `palette: Palette` and font fields; update Default + serialize/deserialize. Verify color math is already pure Rust in `oxiui_core::color_space` — flip item to [x] once confirmed.
  - **Files:** `oxiui-core/Cargo.toml`, `oxiui-core/src/lib.rs`, `oxiui-theme/src/serial.rs`.
  - **Prerequisites:** five oxiui-core derives + `Palette: Default + PartialEq`.
  - **Tests:** palette round-trip; `FontStyle::Oblique{degrees}` round-trip; full-snapshot round-trip incl. palette+fonts; existing 5 round-trip tests green.
  - **Risk:** purely additive (no API removed/renamed, cycle-free). Palette Default: hand-write neutral colours (not all-zero transparent).

## Proposed follow-ups
- CSS-like style sheets, style inheritance, responsive breakpoints, theme
  serialization — these are large/vague and need design work before they can be
  scoped into a single pass.
- `theme-property-lookup-caching` and `style-sheet-compilation` — blocked on
  the CSS-sheet design above.
- `oxiui-egui`/`oxiui-iced`/`oxiui-render-wgpu`/`oxiui-render-soft`/
  `oxiui-accessibility` integration items — cross-crate; should be tracked on
  the consuming crates rather than driven from `oxiui-theme`.
- **Theme serialization via oxicode:** needs serde derive + oxicode dep decision; deferred.
- **Stylesheet compilation:** pre-compile selector matching into O(1) lookup table — follow-up to the parser.
- **Theme property lookup caching:** cache resolved styles per widget type.
- **Lazy palette computation:** hover/pressed/disabled variants lazily derived.
