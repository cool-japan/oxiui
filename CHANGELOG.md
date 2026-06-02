# Changelog

All notable changes to OxiUI are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
OxiUI adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] — 2026-06-01

Initial release of the OxiUI workspace — 13 crates, ~37 000 Rust SLOC,
zero FFI under default features.  No GTK, no Qt, no SDL, no AppKit, no Win32.

### New Crates

| Crate | Description |
|---|---|
| `oxiui-core` | Core trait surface: `Widget`, `UiCtx`, `Theme`, `Layout`, `EventSink`, `RenderBackend`; reactive primitives (`Signal`, `Computed`, `ReactiveRuntime`); constraint solver; event system; paint/draw-list; geometry |
| `oxiui-text` | OxiText + OxiFont bridge — `TextPipeline` shapes and rasterises text via the COOLJAPAN text stack; truncation, word-wrap, rich-text, IME preedit |
| `oxiui-theme` | COOLJAPAN dark/light palettes (Tokyo Night), high-contrast WCAG-AAA palette; `DesignTokens`, `TypographyScale`; `theme_picker` helper |
| `oxiui-render-wgpu` | wgpu GPU render-surface — texture atlas, draw-call batcher, clip-stack, quality presets, headless device init |
| `oxiui-render-soft` | Software CPU framebuffer backend — scanline rasteriser with AA, Bézier/path rendering, blend modes, PNG export, headless smoke path |
| `oxiui-egui` | egui + eframe adapter — palette→`egui::Visuals`, `StatefulEguiAdapter`, `tokens_to_egui_style`, OxiFont byte injection |
| `oxiui-iced` | iced 0.14 Elm-architecture adapter — `IcedUiCtx`, palette→`iced::Theme`, button/label/input widgets, IME stub |
| `oxiui-table` | Virtualized table widget — `RowSource` trait, viewport-windowed rendering, egui + iced backends, sorting/filtering |
| `oxiui-accessibility` | accesskit a11y tree builder — `A11yNode`, `A11yTree`, `A11yNodeBuilder`; widget-graph → `accesskit::TreeUpdate`; headless unit-testable |
| `oxiui-web` | wasm32 entry point — `mount()` on `<canvas>`, `WebHandle`, key mapping, non-wasm stubs; IME via web events |
| `oxiui-slint` | slint 1.16.1 optional adapter — `SlintCtx`, palette mapping (docs-only for M5), headless collection mode |
| `oxiui-dioxus` | dioxus 0.7 optional adapter — `DioxusCtx` reactive bridge, headless collection mode |
| `oxiui` | Facade crate — `App` builder (`title`, `theme`, `content`, `backend`, `min_size`); `Backend::{Egui,Iced,Slint,Dioxus}`; `reactive` module re-exports |

### Milestones delivered

- **M0** — Workspace skeleton, `oxiui-core` trait surface, `deny.toml`, `Dockerfile.ffi-audit`, scripts.
- **M1** — egui adapter + wgpu render + COOLJAPAN theme + OxiText/OxiFont integration.  Hello-world facade works on Linux/macOS/Windows.
- **M2** — iced adapter + theme bridge; both adapters share palette.
- **M3** — `oxiui-table` virtualized rows + iced facade `App::run()` fully wired.  Charts deferred (no plotting backend).
- **M4** — `oxiui-accessibility` (accesskit) + `oxiui-web` (wasm32) + IME CJK events (`UiEvent::ImePreedit`/`ImeCommit`).
- **M5** — softbuffer headless stable, high-contrast WCAG-AAA theme, `oxiui-slint` + `oxiui-dioxus` optional adapters, `Dockerfile.ffi-audit` smoke layer.

### Test coverage

1 204 unit tests across 13 crates — all pass.

### Policy compliance

- Pure Rust default features: zero `*-sys` crates under `cargo tree --edges normal`.
- MSRV: 1.89 (cascaded from oxitext M5 via `wide 1.4.0`).
- License: Apache-2.0.
- No `unwrap()` in production code.
