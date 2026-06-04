# OxiUI

**Version 0.1.1 — 2026-06-04**

OxiUI is the COOLJAPAN-blessed Pure Rust UI layer: no GTK (C), no Qt (C++), no
SDL (C), no system widgets, no raw AppKit / Win32 / Cocoa bindings. It is a
thin, opinionated facade over **egui** (immediate-mode) and **iced**
(Elm-architecture), rendered through **wgpu** (or **softbuffer** for headless),
windowed through **winit**, with all text shaped through **OxiText** +
**OxiFont**. OxiUI exists so that any GUI app in the COOLJAPAN ecosystem can
build with a single `cargo build` in a fresh `rust:slim` container, with no
`libgtk-dev`, `libqt-dev`, or `libsdl2-dev` choreography.

## Status: v0.1.1 — Milestones M0–M5 complete

All six planned milestones for the initial release are done:

| Milestone | Description | Status |
|-----------|-------------|--------|
| M0 | Workspace skeleton, `oxiui-core` traits, `deny.toml`, ffi-audit | ✓ |
| M1 | egui + wgpu + COOLJAPAN theme + OxiText/OxiFont integration | ✓ |
| M2 | iced adapter + theme bridge | ✓ |
| M3 | `oxiui-table` virtualized rows + iced facade `run()` fully wired | ✓ |
| M4 | accesskit a11y + wasm32 entry point + IME CJK events | ✓ |
| M5 | softbuffer headless stable + high-contrast WCAG-AAA + slint/dioxus adapters | ✓ |

## Quick start

```toml
[dependencies]
# Default: egui + wgpu (GPU path)
oxiui = "0.1.1"

# Headless / CI / ffi-audit path (no GPU stack):
oxiui = { version = "0.1.1", default-features = false, features = ["software"] }

# iced backend:
oxiui = { version = "0.1.1", features = ["iced"] }
```

```rust
use oxiui::{App, theme};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new("My App")
        .theme(theme::cooljapan_default())
        .content(|ui| {
            ui.heading("Hello from OxiUI");
        })
        .run()?;
    Ok(())
}
```

## Crates in this workspace

| Crate | Default feature | Description |
|-------|-----------------|-------------|
| `oxiui-core` | (no deps) | Traits, types, reactive state, constraint solver, paint/draw-list, geometry |
| `oxiui-text` | `text` | OxiText + OxiFont bridge — shaping, raster, IME, truncation, word-wrap |
| `oxiui-theme` | optional | Dark/light/high-contrast palettes, `DesignTokens`, `TypographyScale`, `theme_picker` |
| `oxiui-render-wgpu` | `gpu` | wgpu GPU render surface — atlas, batcher, clip-stack, quality presets |
| `oxiui-render-soft` | `software` | CPU scanline rasteriser — AA, Bézier, blend modes, PNG export, headless |
| `oxiui-egui` | `egui` | egui + eframe adapter — palette→Visuals, `StatefulEguiAdapter`, font injection |
| `oxiui-iced` | `iced` | iced 0.14 adapter — `IcedUiCtx`, palette→`iced::Theme`, button/label/input |
| `oxiui-table` | `table` | Virtualized table — `RowSource` trait, egui + iced backends, sorting/filtering |
| `oxiui-accessibility` | `a11y` | accesskit a11y tree — `A11yNode`, `A11yTree`, headless unit-testable |
| `oxiui-web` | `web` | wasm32 entry point — `mount()` on `<canvas>`, key mapping, non-wasm stubs |
| `oxiui-slint` | `slint` | slint 1.16.1 optional adapter — `SlintCtx`, headless collection mode |
| `oxiui-dioxus` | `dioxus` | dioxus 0.7 optional adapter — `DioxusCtx` reactive bridge |
| `oxiui` | facade | `App` builder, `Backend::{Egui,Iced,Slint,Dioxus}`, reactive re-exports |

## Tests

1 948 unit tests across 14 crates — all pass (`cargo nextest run`).

## Replaces (FFI being eliminated)

- `gtk-rs` / `gtk4` — links GTK C
- `qt-*` / `qmetaobject` — links Qt C++
- `sdl2` / `sdl2-sys` — links SDL C
- raw `cocoa-rs` / `objc-foundation` (for windowing — use `winit`)
- raw `windows-rs` (for windowing — use `winit`)

## Anchor crates (Pure Rust, OS-drivers at runtime)

- `egui` + `eframe` — immediate-mode widgets; the **default Pure adapter**.
- `iced` — Elm-architecture retained-mode; **opt-in alternative** adapter.
- `wgpu` — cross-platform GPU API (Vulkan / Metal / DX12 / WebGPU); OS-driver
  line — NOT linked at build time, per GOVERNANCE §8.
- `winit` — windowing + event loop + IME + clipboard.
- `softbuffer` — CPU framebuffer fallback / headless path.
- `slint`, `dioxus` — experimental optional adapters (M5).

## Inter-Oxi

- **Depends on:** `oxitext` (text shaping), `oxifont` (font loading + raster).
- **Depended on by (all OPTIONAL):** oxieda (data UI), oximedia previewer,
  oxiphoton viewer, oxirag chat UI, oxify settings panel.

## Note on GPU drivers

Vulkan / Metal / DX12 drivers (under `wgpu`) and OS windowing (Wayland / X11 /
Cocoa / Win32 under `winit`) are the unavoidable OS boundary — Pure Rust at the
Rust crate layer, OS-side at the syscall. Per GOVERNANCE §8 non-goals, this is
acceptable.

## Blueprint

`../phase3/oxiui_blueprint.md`

## License

Apache-2.0. See [LICENSE](LICENSE).
