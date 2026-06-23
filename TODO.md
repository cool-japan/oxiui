# OxiUI TODO

**v0.2.1 — Unreleased** | **v0.2.0 released 2026-06-23** | **v0.1.3 released 2026-06-20** — Pure Rust Policy v2: GTK/inotify quarantined, 16 crates, Pure Rust facade.

Milestones derived from `../phase3/oxiui_blueprint.md` §Phased milestones.

## Milestones

- [x] **M0** — workspace skeleton, `oxiui-core` traits, error enum, CI scripts,
  `deny.toml`, `Dockerfile.ffi-audit` (software path).
  - Gate: `cargo tree` shows zero `*-sys`; `software`-feature build green in
    `rust:slim`.
- [x] **M1** — egui adapter + wgpu render + COOLJAPAN theme (dark/light) +
  OxiText/OxiFont integration for all text.
  - Gate: a "Hello world" facade app runs on Linux/macOS/Windows with default
    features.
    - **Goal:** Default-features build of the `oxiui` facade exposes
      `oxiui::App::new("title").theme(oxiui::theme::cooljapan_default()).content(|ui| { ui.heading("Hello"); }).run()?`
      which boots an egui app on the host (Linux/macOS/Windows) with COOLJAPAN theming, all text shaped by OxiText and rasterized via OxiFont's glyph data. `cargo tree --workspace --edges normal` on the default closure shows zero `freetype-sys`, `harfbuzz-sys`, `pango-sys`, `fontconfig-sys`, `gtk-*`, `qt-*`, `sdl2*`. A `--no-default-features --features software` build remains green in `rust:slim` (no GPU stack required at build time).
    - **Design:** 7 crates total (1 expanded + 6 new). `oxiui-core` (Widget/UiCtx/Theme/Layout/EventSink traits + UiError, zero deps); `oxiui-text` (OxiText+OxiFont bridge, TextPipeline); `oxiui-theme` (COOLJAPAN dark/light palettes, Tokyo Night colors); `oxiui-render-wgpu` (wgpu surface helper, feature `gpu`); `oxiui-render-soft` (softbuffer CPU backend, feature `software`); `oxiui-egui` (egui+eframe adapter, Theme→Visuals bridge, OxiFont byte injection, `eframe = default-features=false, features=["wgpu"]`); `oxiui` facade (`default = ["gpu","egui"]`). Path deps: oxifont (Wave 1), oxitext (Wave 2). Sub-crates: strict `default = []`. M0 artifacts created first: `deny.toml`, `Dockerfile.ffi-audit`, `scripts/ffi-audit.sh`.
    - **Files:** `oxiui/Cargo.toml` (members += 6); `oxiui/crates/oxiui-core/` (expanded); `oxiui/{deny.toml,Dockerfile.ffi-audit,scripts/ffi-audit.sh}`; `oxiui/crates/oxiui-{text,theme,render-wgpu,render-soft,egui,oxiui}/`; facade `examples/hello.rs`.
    - **Tests:** per-crate units (core traits compile, theme palette non-zero, text pipeline shapes "Hello" → 5 glyphs, egui visuals from palette, render-soft constructs, facade run_headless_once Ok); ffi-audit exits 0.
    - **Risk:** wgpu/winit API churn (pin in workspace); eframe glow feature (use `default-features=false, features=["wgpu"]`); GPU absent in CI (gpu smoke is `#[ignore]`; ffi-audit uses `--features software`).
- [x] **M2** — iced adapter + theme bridge; choose-your-architecture story
  documented.
  - Gate: iced "Hello world" via facade; both adapters share theme.
- [x] **M3 — `oxiui-table` (virtualized rows) + iced facade `run()` wiring (charts deferred)** (completed 2026-05-25)
  - **Goal:** `oxiui-table` renders virtualized (windowed) rows over both egui and iced
    backends through the shared `UiCtx`/theme. The iced facade `App::run()` Backend::Iced path
    is fully wired (closure→message round-trip) — completing the M2 stub. `oxiui-charts`
    deferred (the plotting OxiPhoton does not exist).
  - **Design:**
    - **`oxiui-table`** (`default = []`; feature `table`): a `Table` widget with a
      `RowSource` trait (`row_count() -> usize`, `row(i) -> Vec<Cell>`) and a viewport that
      only materializes visible rows + a small overscan (virtualization). Render adapters for
      egui (`egui::ScrollArea` + `show_rows`) and iced (`iced::widget::scrollable` +
      windowed `column`). Column headers + fixed row height for M3; variable height noted future.
    - **iced facade `run()` wiring:** replace the M2 `UiError::Unsupported` stub. Build an
      `iced::application(boot, update, view)` where `view` drives the user's
      `content(|ui| ...)` closure through `oxiui-iced`'s `IcedUiCtx`, and `update` maps
      `IcedUiCtx` button messages back to `ButtonResponse.clicked` for the next frame
      (closure→message round-trip). Theme via `palette_to_iced_theme`. OxiFont bytes loaded
      via `iced::Settings.fonts` (M2 pattern).
    - Façade `oxiui`: gated re-export `table`; `run()` dispatches Backend::Iced → real iced app.
  - **Files:** `oxiui/Cargo.toml` (member += `oxiui-table`); `oxiui/crates/oxiui-table/
    {Cargo.toml, src/lib.rs, src/egui_table.rs, src/iced_table.rs, tests/table.rs}` (NEW);
    `oxiui/crates/oxiui/src/lib.rs` (Backend::Iced run() wiring; remove the Unsupported stub);
    `oxiui/crates/oxiui-iced/src/lib.rs` (message round-trip);
    `oxiui/crates/oxiui/examples/hello_table.rs` (NEW).
  - **Prerequisites:** none (iced 0.14 already wired in Wave 4).
  - **Tests:** table — a 10k-row `RowSource`, assert only a viewport-sized window is
    materialized per frame (count `row(i)` calls); headers present. iced run() —
    `run_headless_once` path exercises the closure via NullCtx for both backends; example
    `hello_table` + `hello_iced` build with `--features iced,table`.
  - **Risk:** iced 0.14 retained-mode ↔ immediate `UiCtx` round-trip is the tricky part
    (button click state must survive one frame of latency) — accept best-effort M3 mapping,
    documented. `oxiui-charts` deferred with an explicit blocker note (needs a plotting
    backend that doesn't exist; `oxiphoton` is unrelated photonics sim).
- [ ] **M3 (deferred) — `oxiui-charts` (line/scatter/bar over OxiPhoton)**
  - **BLOCKED:** The plotting "OxiPhoton" does not exist. The existing `oxiphoton` crate
    (`oxiphoton`) is a photonics-simulation (FDTD/optics) crate with no
    plotting API. Re-evaluate when a plotting backend exists under the OxiUI umbrella.
- [x] **M4** — `oxiui-accessibility` (accesskit) + `oxiui-web` (wasm) + IME CJK
  matrix. (completed 2026-05-25)
  - Gate: NVDA / VoiceOver / Orca smoke; Japanese IME works on all 3 desktops
    + wasm.
  - [x] **M4 — `oxiui-accessibility` (accesskit) + `oxiui-web` (wasm) + IME CJK** (completed 2026-05-25)
    - **Goal:** `oxiui-accessibility` builds an accesskit a11y tree from the widget tree (feature `a11y`); `oxiui-web` targets wasm32 (feature `web`, `cargo check --target wasm32-unknown-unknown`); CJK IME plumbing wired across desktop backends. Gate: a11y tree non-empty; web checks on wasm32; IME preedit surfaces to egui/iced input path.
    - **Design (8a — a11y):** `accesskit 0.24.0` + `accesskit_winit 0.33.0`. Walk `oxiui-core` widget tree → `accesskit::TreeUpdate` (roles: window/group/button/label/table-row). Headless-testable: build tree from sample widget graph + assert node roles without a live AT.
    - **Design (8b — web):** `wasm-bindgen 0.2.122` + `web-sys 0.3.99`; wgpu via eframe's wasm backend. `mount(canvas_id)` entry boots oxiui on `<canvas>`. Proven by `cargo check --target wasm32-unknown-unknown`. Runtime verification = browser-CI only.
    - **Design (IME CJK):** thread winit `Ime::{Preedit, Commit}` events into `oxiui-core` event sink → egui (`egui::Event::Ime`) / iced text input. Document manual cross-desktop + browser IME matrix.
    - **Files:** `oxiui/Cargo.toml` (members += `oxiui-accessibility`, `oxiui-web`; ws deps += accesskit, accesskit_winit, wasm-bindgen, web-sys; features `a11y`, `web`); `oxiui/crates/oxiui-accessibility/{Cargo.toml, src/lib.rs, src/tree.rs, tests/tree.rs}` (NEW); `oxiui/crates/oxiui-web/{Cargo.toml, src/lib.rs, src/wasm.rs}` (NEW); IME events added to `oxiui-core/src/lib.rs` as `UiEvent::ImePreedit`/`ImeCommit` variants; `oxiui-egui` `forward_event_to_egui()` function; `oxiui-iced` `forward_ime_event()` stub.
    - **Tests:** a11y — sample widget graph → TreeUpdate with expected node count + roles (headless). web — `cargo check --target wasm32-unknown-unknown` green. IME — 3 unit tests for `UiEvent::ImePreedit`/`ImeCommit` roundtrip in `oxiui-core`. ffi-audit default closure clean.
    - **Risk:** accesskit_winit ↔ winit version must match eframe 0.34's winit. wasm wgpu backend — rely on eframe/wgpu wasm features, no manual wgpu pin.
    - **Note:** `oxiui-web` wasm32 path compile-checked (`cargo check --target wasm32-unknown-unknown`); runtime testing requires browser CI. IME CJK events plumbed through `UiEvent::ImePreedit`/`ImeCommit` (marked `#[non_exhaustive]`); egui forwarded via `oxiui_egui::forward_event_to_egui()`; iced 0.14 IME is a best-effort no-op stub (`forward_ime_event()`) since iced 0.14 has no public per-widget IME injection API; manual cross-platform IME matrix test pending.
- [x] **M5 — softbuffer headless stable + high-contrast theme + `oxiui-slint` + `oxiui-dioxus` (optional alternates; OQ#3: ship both at M5, deprecate one at M6)** (completed 2026-05-25)
  - **Goal:** `oxiui-render-soft` headless path stable enough for `Dockerfile.ffi-audit` smoke; `high-contrast` COOLJAPAN palette (WCAG AAA) added; `oxiui-slint` and `oxiui-dioxus` add experimental optional GUI adapters. MSRV bumps to 1.89 (cascades from oxitext M5 via `wide 1.4.0`). Gate: ffi-audit Dockerfile smoke runs OxiUI app entirely headless; slint + dioxus examples build.
  - **Design:** 8a — softbuffer headless + high-contrast: harden `render_headless_once(w, h) -> RgbaBuffer`; `render_to_png(path)` via `png` crate (Pure); `cooljapan_high_contrast()` palette (luma contrast > 7.0, WCAG AAA) in `oxiui-theme`; `Dockerfile.ffi-audit` smoke layer running `hello_headless` example asserting non-zero pixel count. 8b — `oxiui-slint` (`default=[]`, feature `slint 1.16.1`, MSRV 1.88 < 1.89 floor): `SlintCtx` impl of `UiCtx`; `Palette→slint` style mapping; `hello_slint.rs` example. `oxiui-dioxus` (`default=[]`, feature `dioxus 0.7.9`): `DioxusCtx` reactive adapter; `hello_dioxus.rs` example. Façade: `Backend::Slint`/`Backend::Dioxus` variants; `rust-version="1.89"`.
  - **Files:** `oxiui/Cargo.toml` (members += `oxiui-slint`, `oxiui-dioxus`; ws deps += slint 1.16.1, dioxus 0.7.9; `rust-version="1.89"`; features `slint`, `dioxus`, `high-contrast`); `crates/oxiui-render-soft/src/{headless.rs,png.rs}` (NEW); `crates/oxiui-theme/src/high_contrast.rs` (NEW); `crates/oxiui-slint/{Cargo.toml, src/{lib,ctx}.rs, tests/build.rs}` (NEW); `crates/oxiui-dioxus/{Cargo.toml, src/{lib,ctx}.rs, tests/build.rs}` (NEW); `crates/oxiui/examples/{hello_headless,hello_slint,hello_dioxus}.rs` (NEW); `Dockerfile.ffi-audit` (headless smoke layer added).
  - **Prerequisites:** Slice 5 MSRV bump (oxitext → 1.89) cascades here transitively; confirmed fine.
  - **Tests:** headless — `render_headless_once(800, 600)` RGBA buffer has > 0 non-background pixels; PNG round-trip parses back via `png` crate. high-contrast — luma contrast ratio > 7.0 on foreground/background pair (WCAG AAA). slint — `cargo build --example hello_slint --features slint` green. dioxus — same. ffi-audit Docker smoke passes.
  - **Risk:** slint Theme/Style plug-in seam — verify in 1.16.1; if absent, palette mapping is docs-only note. dioxus 0.7 API churn — confirm public API surface at impl. softbuffer headless on macOS/Linux requires no display server (pure pixel buffer).

- [x] **M6 — `oxiui-compute-wgpu` + `oxiui-render-wgpu` crates.io publication** (completed 2026-06-04)
  - **Done:** `oxiui-compute-wgpu` v0.1.1 and `oxiui-render-wgpu` v0.1.1 published to crates.io 2026-06-04.
    Downstream consumers can now depend on them via version. `oxiphysics` migration is a separate future task.
  - **`oxiui-compute-wgpu` additions completed (2026-06-02):**
    - `SHADER_SPH_DENSITY` — cubic-spline SPH density kernel (WGSL)
    - `SHADER_BITONIC_SORT` — in-workgroup bitonic sort ≤1024 f32 values (WGSL)
    - `SHADER_MAP_F32_TEMPLATE` / `SHADER_ZIP_MAP_F32_TEMPLATE` — element-wise map/zip templates
    - `Dispatcher` struct: `map_f32`, `zip_map_f32`, `reduce_sum_f32`, `sph_density`, `sort_f32`
    - `ComputeContext::dispatcher()` convenience method
  - **`oxiui-render-wgpu` additions completed (2026-06-02):**
    - `SurfaceContext` — windowed surface from raw window/display handles
    - `SurfaceConfig` — swap-chain config (dimensions, present mode, alpha mode)
    - `SurfaceContext::acquire_frame` / `present_frame` / `resize`
  - **oxiphysics migration steps** (once published):
    1. Add to `oxiphysics` workspace deps:
       ```toml
       oxiui-compute-wgpu = "0.1"
       oxiui-render-wgpu  = "0.1"
       ```
    2. In `oxiphysics-gpu/Cargo.toml`: replace `wgpu.workspace = true` with
       `oxiui-compute-wgpu.workspace = true`; use `oxiui_compute_wgpu::wgpu::*`
       for raw wgpu types and `oxiui_compute_wgpu::ComputeContext` instead of
       manual Instance→Adapter→Device→Queue init.
    3. In `oxiphysics-viz/Cargo.toml`: replace `wgpu.workspace = true` with
       `oxiui-render-wgpu.workspace = true`; use `SurfaceContext::from_raw_handles`
       for windowed physics visualization.
    4. Remove direct `wgpu` dep from both crates (use the re-export from
       `oxiui-compute-wgpu::wgpu` / `oxiui-render-wgpu::wgpu`).
  - **Gate:** `cargo publish --dry-run -p oxiui-compute-wgpu` and
    `cargo publish --dry-run -p oxiui-render-wgpu` pass; oxiphysics workspace
    builds with `oxiui-compute-wgpu` in place of `wgpu`; all oxiphysics-gpu
    tests pass.
  - **Readiness caveats (inherit from Tier B notice):**
    - `oxiphysics-gpu`'s CPU-simulation dispatch layer (closures over f64 data)
      stays on CPU regardless; only the wgpu-backend hardware path migrates.
    - `SHADER_BITONIC_SORT` handles ≤1024 elements per workgroup; for larger
      particle counts, oxiphysics-gpu must either call sort in tiled segments or
      switch to a multi-pass radix sort (future `SHADER_RADIX_SORT_MULTIPASS`
      kernel).

## Dependency inversion (2026-06-05)

- [x] Decision: `oxiui-web` stays `publish = false` and out of the `oxiui` facade — documented as a copy-template wasm32 cdylib entry point (not a library dep). Facade README corrected to drop the non-existent `web` feature. (done 2026-06-05)

## Per-Crate Detail

Detailed TODO lists with estimated SLOC, testing plans, performance targets, and integration
requirements are maintained in each subcrate's directory:

| Crate | Status | Key Gaps |
|-------|--------|----------|
| [`oxiui-core`](crates/oxiui-core/TODO.md) | ~195 SLOC, seed traits only | Widget tree, layout engine (flex/grid), event dispatch, focus, hit-test, reactive state, animations |
| [`oxiui-text`](crates/oxiui-text/TODO.md) | ~36 SLOC, pipeline wrapper | Text layout/line-breaking, text input widget, selection, IME rendering, rich text, font fallback |
| [`oxiui-theme`](crates/oxiui-theme/TODO.md) | ~69 SLOC, dark/light palettes | High-contrast, design tokens, typography scale, CSS-like style sheets, responsive breakpoints, animation tokens |
| `oxiui-compute-wgpu` | ~2925 SLOC, functional | multi-pass radix sort (>1024 elements), timestamp queries, WGSL hot-reload |
| [`oxiui-render-wgpu`](crates/oxiui-render-wgpu/TODO.md) | ~4139 SLOC, headless+surface | 3D mesh pipeline, depth buffer management, shadow maps, MSAA, post-processing |
| [`oxiui-render-soft`](crates/oxiui-render-soft/TODO.md) | ~46 SLOC, zeroed-buffer stub | Scanline rasterizer, AA, bezier/path rendering, blending, gradient, glyph blitting, headless/PNG |
| [`oxiui-egui`](crates/oxiui-egui/TODO.md) | ~116 SLOC, functional adapter | Expanded widget forwarding, layout, tooltips, popups, rich text, clipboard, style token mapping |
| [`oxiui-iced`](crates/oxiui-iced/TODO.md) | ~235 SLOC, functional adapter | Text input, checkbox, slider, dropdown, layout control, state persistence, IME (blocked on iced API) |
| [`oxiui-table`](crates/oxiui-table/TODO.md) | ~255 SLOC, virtualized rows | Sorting, resizing, reordering, selection, editing, filtering, pagination, variable height, CSV export |
| [`oxiui-accessibility`](crates/oxiui-accessibility/TODO.md) | ~127 SLOC, tree builder | Platform adapter wiring, dynamic updates, focus tracking, live regions, action handling, contrast detection |
| [`oxiui-web`](crates/oxiui-web/TODO.md) | ~74 SLOC, canvas stub | WebRunner wiring, events, IME, clipboard, resize, WebGPU detection, a11y DOM fallback |
| [`oxiui`](crates/oxiui/TODO.md) | ~403 SLOC, facade | Window config, multi-window, lifecycle hooks, state management, plugins, dialogs, menu bar, hotkeys |

## Open Questions

1. **GOVERNANCE §7 substitution-table inclusion.** Should §7 add `gtk-rs` /
   `gtk4` / `qmetaobject` / `qt-*` / `sdl2` / `sdl2-sys` / `cocoa-rs` (for
   windowing) / raw `windows-rs` (for windowing) / `freetype-sys` /
   `harfbuzz-sys` / `pango-sys` / `fontconfig-sys` with **OxiUI** as the
   required replacement? Exact parallel to the OxiCrypto Open Question on
   `ring` / `aws-lc-rs`. Currently `deny.toml` enforces it per-workspace;
   promoting to §7 makes it ecosystem-wide.
2. **egui vs iced as the recommended default.** egui is simpler,
   immediate-mode, and ships today. iced is more structured and scales to
   larger apps. Default is `["egui"]` in this draft; should the recommendation
   flip for new apps above a certain complexity threshold, and if so where is
   that threshold documented?
3. **slint and dioxus — keep both as official optional adapters, or pick
   one?** Both are credible Pure Rust; carrying both doubles the maintenance
   surface. M5 ships both as experimental; should one be deprecated by M6?
4. **Native menu bars and system tray.** Currently out of scope. Demand exists
   in oxieda and oximedia previewer. Is `oxiui-native-shell` (Bounded FFI,
   opt-in only, parallel to oxitls-adapter-aws-lc) a future M6 addition, or
   does each consumer roll its own?
5. **Mobile path.** iOS / Android share enough with winit + wgpu (via
   android-activity / UIKit interop) that a future `oxiui-mobile` is
   conceivable. Defer to a Phase 4 conversation, or pre-commit a tracking
   issue now?
6. **`wgpu` direct consumers — RESOLVED (option b, 2026-06-02).** `oxiphysics`
   (`oxiphysics-gpu`, `oxiphysics-viz`) used `wgpu` directly. Decision: **option
   (b)** — `oxiui-compute-wgpu` is the canonical oxi* GPU-compute sub-crate.
   `oxiui-render-wgpu` now provides `SurfaceContext` for windowed rendering.
   Migration is blocked only on **crates.io publication** of both sub-crates.
   See **M6** below for the publication + migration milestone.
