# OxiUI TODO

**v0.2.2 released 2026-08-06** | **v0.2.1 released 2026-07-30** | **v0.2.0 released 2026-06-23** — Pure Rust Policy v2: GTK/inotify quarantined, 16 crates, Pure Rust facade.

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
  - **Plan-vs-reality correction (added 2026-08-03):** two of the M5 bullets above described in the original Design/Files plan were not actually delivered at M5's completion, despite the milestone checkbox above being marked `[x]`. (1) `hello_headless.rs` and the `Dockerfile.ffi-audit` smoke layer running it did not exist until the 2026-08-03 hygiene wave built them (see the Production-Readiness Backlog "Wave 3 hygiene pass" entry below) — `render_headless_once`/`render_to_png` themselves were real and unit-tested, but the example + container smoke-test gate the milestone claimed to pass was never actually running. (2) The facade never grew a `Backend::Slint` variant — `crates/oxiui/src/lib.rs`'s `Backend` enum has only `Egui`, `Iced` (feature `iced`), and `Dioxus` (feature `dioxus`); slint is driven through the separate `oxiui-slint` crate's own `run_slint`/`SlintCtx` entry points, not through `App::backend(Backend::Slint)`. The facade's own doc comment (`crates/oxiui/src/lib.rs`) is and was accurate about this; only this file's M5 plan text overclaimed it.
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

SLOC below is `tokei crates/<crate>/src` code-line count (2026-08-03), not the
original seed-stage estimate. Four rows (`oxiui-core`, `oxiui-render-soft`,
`oxiui-web`, `oxiui`) previously described post-M6 crates using their pre-M1
seed sizes and status text (e.g. "zeroed-buffer stub" for an 8-module, fully
implemented CPU rasterizer) — corrected below; the "Key Gaps" text for those
four now reflects specific gaps actually verified during the 2026-08-03
hygiene pass rather than the original scaffolding checklist, which is long
since done.

| Crate | Status | Key Gaps |
|-------|--------|----------|
| [`oxiui-core`](crates/oxiui-core/TODO.md) | 9091 SLOC, implemented (widget tree, flex/grid + Cassowary constraint-solver layout, event dispatch, focus/hit-test, `Signal`/`Computed` reactive state, animation primitives) | See crate TODO.md for the current backlog; the original "seed traits only" scaffolding checklist is done |
| [`oxiui-text`](crates/oxiui-text/TODO.md) | 3766 SLOC, pipeline wrapper | Text layout/line-breaking, text input widget, selection, IME rendering, rich text, font fallback |
| [`oxiui-theme`](crates/oxiui-theme/TODO.md) | 3169 SLOC, dark/light palettes | High-contrast, design tokens, typography scale, CSS-like style sheets, responsive breakpoints, animation tokens |
| `oxiui-compute-wgpu` | 3463 SLOC, functional | multi-pass radix sort (>1024 elements), timestamp queries, WGSL hot-reload |
| [`oxiui-render-wgpu`](crates/oxiui-render-wgpu/TODO.md) | 10013 SLOC, headless+surface | 3D mesh pipeline, depth buffer management, shadow maps, MSAA, post-processing |
| [`oxiui-render-soft`](crates/oxiui-render-soft/TODO.md) | 5659 SLOC, implemented (scanline rasterizer, AA, bezier/path fill+stroke, blending, gradients, glyph blitting, headless/PNG) | Two crash bugs found by the new `fuzz/` harness (2026-08-03), not yet fixed: `scanline.rs:259` `y_start - edge_y_start` can subtract-overflow for an extreme-but-finite vertex Y (fast-forward loop added by the `oxiui-bug-1` fix); `path.rs`'s `mid()` can overflow a coordinate sum to `Infinity` from finite `flatten_quad`/`flatten_cubic` inputs, causing unbounded recursion. Repros checked into `fuzz/regressions/` (not gitignored, unlike `fuzz/corpus/`/`fuzz/artifacts/`) with decoded parameters in `fuzz/regressions/README.md`. |
| [`oxiui-egui`](crates/oxiui-egui/TODO.md) | 809 SLOC, functional adapter | Expanded widget forwarding, layout, tooltips, popups, rich text, clipboard, style token mapping |
| [`oxiui-iced`](crates/oxiui-iced/TODO.md) | 1381 SLOC, functional adapter | Text input, checkbox, slider, dropdown, layout control, state persistence, IME (blocked on iced API) |
| [`oxiui-table`](crates/oxiui-table/TODO.md) | 4295 SLOC, virtualized rows | Sorting, resizing, reordering, selection, editing, filtering, pagination, variable height, CSV export |
| [`oxiui-accessibility`](crates/oxiui-accessibility/TODO.md) | 2494 SLOC, tree builder | Platform adapter wiring, dynamic updates, focus tracking, live regions, action handling, contrast detection |
| [`oxiui-web`](crates/oxiui-web/TODO.md) | 2976 SLOC, implemented (WebRunner wiring, events, IME, clipboard, resize, WebGPU detection; `cargo check --target wasm32-unknown-unknown` green including `set_theme`'s live egui-context theme swap, added 2026-08-03) | `tests/api_tests.rs` doesn't compile for `--target wasm32-unknown-unknown` (written against the native no-op `mount(id, opts)` stub; real `wasm::mount(id)` is `async` and single-arg) — no wasm32-specific test coverage exists yet. A handful of pre-existing `clippy --target wasm32-unknown-unknown` warnings in `performance.rs`/`font_loading.rs` (type-complexity / non-Send-Sync Arc) are also unaddressed. |
| [`oxiui`](crates/oxiui/TODO.md) | 3218 SLOC, facade (window config, lifecycle hooks, state management, plugins, dialogs, hotkeys, multi-window + menu-bar shell) | Multi-window / menu-bar backend wiring landed 2026-08-04 (`shell.rs` + `menu::render_menu_bar` + egui deferred viewports); iced secondary windows remain `UiError::Unsupported` pending an `iced::daemon` port |

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


---

<!-- production-readiness-backlog 2026-07-16 -->
## Production-Readiness Backlog — 2026-07-16

_Consolidated from static audit + Opus adversarial bug-hunt (48 verified defects across noffi) + baseline nextest/clippy + design investigation. See `../NOFFI_PRODUCTION_BACKLOG.md` for the full cross-project list and severity/model legend. Not implemented; no commits._

**Confirmed bugs — Opus-verified (software rasterizer, unbounded/overflow on unclamped coords):**
- [x] **S · high** `oxiui-render-soft/src/scanline.rs:238` — fill loop iterates full vertical extent from vertex coords, never clamped to framebuffer height → billions of iterations from one large Y. R2/N0. Fixed via `fill_polygon`'s Y-range clamp to framebuffer height (`fill_polygon_with_scratch` in `crates/oxiui-render-soft/src/scanline.rs`) plus an edge fast-forward so partially-offscreen shapes still render correctly. Covered by new tests `fill_polygon_far_out_of_bounds_is_clamped_and_fast` and `fill_polygon_partial_offscreen_top_still_paints_visible_rows`.
- [x] **S · med** `oxiui-render-soft/src/scanline.rs:358` — `paint_span` `for px in x0..x1` from raw float X, no width clamp → billions of no-op iterations. R2/N0. Fixed via a width clamp in `paint_span` before the `x0..x1` loop. Covered by new test `paint_span_far_out_of_bounds_x_is_clamped`.
- [x] **S · med** `oxiui-render-soft/src/blend.rs:314` — `composite_into` computes `w*h*4` guard and `(j*w+i)*4` index in u32 → overflow defeats bounds check. R2/N0. Fixed via checked `usize` arithmetic for the required-length guard (treats overflow as "unsatisfiable", returns 0). Covered by new tests `composite_into_near_u32_boundary_does_not_overflow_or_panic` and `composite_into_exact_u32_boundary_dims_no_panic`.
- Fix: clamp raster spans to framebuffer rect; do size math in usize/checked.
**Flagship realization pass (U1–U5) — completed 2026-07-17:**
- [x] **A/med · U1** EguiRunner/IcedRunner realization — the live `eframe::run_native` / `iced::application` paths were moved out of `lib.rs` into `runner.rs`. The runners now carry the app's theme/hooks/plugins state; `App::run()` moves its fields into the matching runner (`std::mem::take`) and delegates via `BackendRunner::run`. The runners no longer return `Ok` immediately. wasm/native cfg gating preserved (wasm egui → `Unsupported("use oxiui_web::mount")`; no-backend → `Unsupported`).
- [x] **A/med-hard/Opus · U2** lifecycle hooks wired. `runner::LifecycleTracker` dedups raw size/focus/close snapshots into `LifecycleEvent`s. egui fires `on_resize`/`on_focus` from per-frame `viewport_rect()`/`focused` polling and `on_close` from `eframe::App::on_exit`. iced fires them from an `event::listen_with` subscription (`Resized`/`Focused`/`Unfocused`/`CloseRequested`), booted with `exit_on_close_request(false)` + `window_size(...)` (the previously-dead width/height are now honoured). Persist-on-close: `with_persistent_state` shares state via `Arc<Mutex<_>>` and encodes to disk in the `on_close` hook (replacing the old `let _ = path` no-op). `run_headless_once` now fires `on_close` after its single frame so persistence is exercised headlessly. Hooks receive a shared `NullUiCtx` (hoisted to `null_ctx.rs`), since lifecycle events fire outside a live frame.
- [x] **B/easy · U3** all `#[allow(unused_variables)]` (facade `native_dialog`, `oxiui-render-soft` `canvas_upload`/`fft_blur`, `oxiui-tray`, and 21 in `oxiui-web`) replaced with explicit `let _ = (...)` discards inside the inactive-cfg branch. Zero `#[allow(unused_variables)]` remain in the workspace.
- [x] **B/easy · U4** `fft_blur` feature-off path now forwards to the direct `shadow::gaussian_blur_alpha` blur (numerically equivalent) instead of silently no-oping; the `fft-blur` feature only changes *speed*, never *whether* the blur happens. Covered by `fft_blur_fallback_blurs_when_feature_off`.
- [x] **B/easy · U5** docs truth pass (this file, runner/lifecycle doc-comments, CHANGELOG). Genuinely out of scope / upstream-blocked, documented honestly rather than as done: `oxiui-charts` (needs a plotting backend that does not exist yet), iced IME injection (no public per-widget IME API in iced 0.14), softbuffer window path, and the `oxiui-web` wasm32 build (pre-existing `web-sys` API drift: `Document::head`/`exec_command`, `FontFace` constructor — unrelated to U1–U5).

**Wave 3 hygiene pass — completed 2026-08-03 (`rustfmt.toml`/`clippy.toml`, GPU error handling, fuzz harness):**
- [x] **B/easy** `rustfmt.toml` + `clippy.toml` added at the workspace root (stable-only keys: `edition`/`max_width`/`tab_spaces`; `msrv = "1.89"` matching `Cargo.toml`'s `rust-version`). The chosen values are *identical* to stable rustfmt's own defaults (`rustfmt --print-config default` prints `max_width = 100`, `tab_spaces = 4`; `edition` is read from `Cargo.toml` by `cargo fmt` regardless), so this file adds zero incremental reformat pressure beyond what an unconfigured `cargo fmt` would already demand. Verified 2026-08-03/04: `cargo fmt --check` at that point had exactly one violation, `crates/oxiui/src/multiwindow.rs`'s `take_shell` signature at 103 columns — freshly-written code from the concurrent facade multi-window/menu-bar wave (see the still-open item below), not a pre-existing style deviation and not caused by this `rustfmt.toml`'s settings. Re-run `cargo fmt --check` once that item lands.
- [x] **B/med** `oxiui-compute-wgpu`: `buffer::read_back`/`read_back_range`/`TypedBuffer::download` now return `Result<_, ComputeError>` instead of `.expect()`-panicking on device-poll/mapping failure (device lost, OOM); all 5 `Dispatcher` methods (`map_f32`, `zip_map_f32`, `reduce_sum_f32`, `sph_density`, `sort_f32`) propagate it. `integration::render_soft`'s `gpu_gaussian_blur_rgba`/`gpu_ordered_dither_rgba`/`gpu_linear_gradient_fill` and `integration::text::rasterize_glyphs` keep their existing void/no-`Result` public signatures but now fall back to their sibling CPU implementation on a GPU-runtime failure (not just when `ctx` is `None`), matching the crate's existing GPU-unavailable-fallback architecture — safe because none of the four write their output buffer until the GPU readback has already succeeded.
- [x] **B/easy** `crates/oxiui/examples/hello_headless.rs` (NEW) + `[[example]]` `required-features = ["software"]` entry + two `crates/oxiui/tests/example_compilation.rs` regression tests (`example_hello_headless_compiles`, `example_hello_headless_runs_and_exits_ok`) + `Dockerfile.ffi-audit` smoke-test layer (`cargo run --example hello_headless`). Closes the M5 gap where the milestone claimed this smoke layer without it existing.
- [x] **B/easy** `oxiui-web::set_theme` now applies the theme to the live `egui::Context` (dark/light via `egui::Context::set_theme`; high-contrast via `oxiui_theme::cooljapan_high_contrast()` mapped through `oxiui_egui::palette_to_egui_visuals`) instead of encoding an inert `__theme:` sentinel through `inject_event` that nothing decoded.
- [x] **B/easy** `notify` dependency moved from an inline `crates/oxiui-hot-reload-notify/Cargo.toml` pin to `[workspace.dependencies]`.
- [~] **B/easy, deferred (blocked)** COOLJAPAN sibling dep-pin sweep (`oxifont`/`oxitext`/`oxitext-sdf` `"0.2.1"` → `"0.2.2"`): the live `crates.io` HTTP API refused a direct query (`403`, "unable to process request... API data access policy"), so this was instead confirmed against the local `~/.cargo/registry/index/.../.cache/ox/{if/oxifont,it/oxitext,it/oxitext-sdf}` sparse-index cache and `Cargo.lock` — both show the newest published version of all three as `0.2.1`; `0.2.2` does not resolve. The sibling repos' own `Cargo.toml` already say `version = "0.2.2"`, but a version bump in a crate's own manifest is not evidence that version has been `cargo publish`ed. Cannot bump the pin without that publish happening (by another wave/session — this wave is forbidden from running `cargo publish`), since resolving a nonexistent `0.2.2` would break `cargo build`. Re-attempt once those three crates actually ship 0.2.2.
- [x] **B/med** `fuzz/` cargo-fuzz harness added (`fuzz_fill_polygon`, `fuzz_composite_into`, `fuzz_bezier_flatten` targets over `oxiui-render-soft`; `cargo +nightly fuzz build` green). **Found two live crash bugs within seconds of fuzzing, neither fixed by this wave (out of B-severity/this wave's scope — the wave that fixed the sibling S-bugs below should take these too):**
  - **`scanline.rs:259`** (inside the fast-forward loop the `oxiui-bug-1` fix above added): `e.x += e.dx * (y_start - edge_y_start) as f32;` — `y_start` is clamped to `[0, framebuffer_height]` but `edge_y_start` comes from an *unclamped* `as i32` cast on a raw vertex Y and can be `i32::MIN` for an extreme-but-finite coordinate, so the subtraction panics `attempt to subtract with overflow`. Repro: `fuzz/regressions/fuzz_fill_polygon/crash-75bd14507aaf06979c7bdbc7479e667dda9c14ef` (checked in, not gitignored — decoded to a 3-point triangle with one `y` at `-1.70e38` in `fuzz/regressions/README.md`; the original `fuzz/artifacts/` copy IS gitignored and would not have survived a commit). Fix shape: `saturating_sub`.
  - **`path.rs` `flatten_quad`/`flatten_cubic`**: the private `mid(a, b)` helper computes `(a.0 + b.0) * 0.5`; when both coordinates are large and same-signed, `a.0 + b.0` overflows `f32` to `Infinity` *before* the halving — a **finite** input pair can manufacture a non-finite midpoint. That `Infinity` then makes the chord-deviation base-case comparison always `false` (same failure shape as an actual NaN input), so recursion never terminates: `AddressSanitizer: stack-overflow` in `flatten_quad`. Repro: `fuzz/regressions/fuzz_bezier_flatten/crash-bc9e57a5522d24fc0508b9f3935bd9ab75686094` (checked in, not gitignored — decoded to `p0 = (0.0, -2.35e38)` plus `p1`/`p2`/`tolerance` in `fuzz/regressions/README.md`). Reachable from the public `Path::fill`/`stroke`. Fix shape: saturate/clamp inside `mid()`, or cap recursion depth.
- [x] **A/hard — done 2026-08-04 (Wave 4)** Facade multi-window registry and menu bar are now consumed by the backend layer.
  - **New `crates/oxiui/src/shell.rs`** — `ShellConfig { windows, contents, menu_bar, handle }`, built by `WindowRegistry::take_shell` and handed to the backend by `App::run()` through a new **`BackendRunner::set_shell`** trait method. Its *default* implementation accepts an empty shell and returns `UiError::Unsupported` for a non-empty one, so no runner (including third-party ones) can silently swallow a menu bar or a window.
  - **Menu bar rendering.** `menu::render_menu_bar(&MenuBar, &mut MenuBarState, &mut dyn UiCtx) -> usize` draws the bar with plain `UiCtx` primitives (`menu_bar`/`button`/`popup`/`separator`), tracks the open menu/submenu chain in `MenuBarState`, invokes the registered `MenuItem::Action` callback on click and returns how many fired. It degrades instead of vanishing when an adapter reports a container unsupported (`menu_bar` → `horizontal` → inline; `popup` → `vertical` → inline) — which is exactly what happens on iced 0.14, whose `IcedUiCtx::menu_bar` is unsupported. Wired into `OxiEguiApp::ui` (above the content), the iced `view` function, `App::run_headless_once` and `App::build_a11y_snapshot`.
  - **Secondary windows (native egui).** `OxiEguiApp::drive_secondary_windows` opens one `egui::Context::show_viewport_deferred` viewport — a real OS window — per open descriptor, keyed by a stable `ViewportId::from_hash_of(("oxiui_secondary_window", id))`, with `WindowConfig` → `ViewportBuilder` translation (title/size/resizable/decorations/transparent/always-on-top). Per-window content closures come from the new `App::open_window_with(config, F)` (`SharedContent = Arc<Mutex<ContentFn>>`, satisfying egui's `Fn + Send + Sync` deferred-callback bound); a window registered without one draws its title. Closing is driven both ways: the viewport callback records `input().viewport().close_requested()` and the next frame stops showing that viewport (which is what destroys the OS window).
  - **Runtime control.** `App::window_handle() -> &WindowHandle` returns a cloneable, `Send + Sync` command queue (`open` / `close` / `focus`, each `Result<(), UiError>`); the backend drains it every frame into a `WindowSession` state machine (unknown id → `UiError::Window`, focus on a closed window → `UiError::Window`) and issues `ViewportCommand::Focus` for focus requests.
  - **Honest rejection elsewhere.** `IcedRunner::set_shell` takes the menu bar but returns `UiError::Unsupported` for secondary windows (iced 0.14's `application` runtime is single-window; multi-window needs `iced::daemon`), and `App::run()` with `Backend::Dioxus` rejects any non-empty shell up front. Both are proven by tests that call the real `App::run()` and assert it errors *before* an event loop boots.
  - **Widget-id isolation.** New `EguiUiCtx::with_id_base` / `IcedUiCtx::with_id_base` (+ `IcedUiCtx::next_widget_id`) reserve a high id range (`usize::MAX / 2`) for OxiUI chrome. Required, not cosmetic: the menu bar's widget count changes as its drop-down opens/closes, and a shared range would shift every content widget id with it — rerouting iced button clicks and invalidating egui's persistent widget state. On iced the bar is additionally rendered into its own `IcedUiCtx` and stacked above the content element.
  - **Testing.** 26 new unit tests (`menu.rs` renderer/state incl. submenu expand-collapse and action dispatch, `multiwindow.rs` handle+session, `egui_backend.rs` viewport-id/builder + close-flag folding, `runner.rs` `set_shell` accept/reject, `shell.rs`) plus `crates/oxiui/tests/shell_tests.rs` (15 facade-level tests, incl. chrome/content id-range isolation and multi-frame init-once parity driving frames through a probe `UiCtx` via the new `App::run_headless_frame(&mut dyn UiCtx) -> usize`). Docs corrected in `lib.rs`, `menu.rs`, `multiwindow.rs` and `crates/oxiui/README.md`: they no longer claim backend consumption that did not exist, and now state the per-backend support matrix.
- Verification note: `cargo deny check bans` fails on `flate2`/`miniz_oxide` reached transitively via `png`. `png` is a non-optional dependency of `oxiui-render-soft` (used by `RgbaBuffer::save_png`) and is ALSO reached inside the `oxiui` facade's own **default** feature closure (`default = ["gpu", "egui"]`) — confirmed via `cargo tree -p oxiui -i flate2`: `eframe`'s clipboard path (`egui-winit` → `arboard` → `image` → `png`) plus `oxiui`'s own `egui`/`software` features' `dep:png`. This means the `deny.toml` header's "pure facade `oxiui`... verified FFI-free" claim was already inaccurate with respect to the compression-crate ban rows specifically (corrected in `deny.toml` itself, 2026-08-03) — it was and remains accurate for the GTK/Qt/SDL/font-C/crypto-C rows. Confirmed pre-existing — `png` was already a workspace dependency before this wave and before the `deny.toml` ban list existed; not touched or introduced by any 2026-07/08 wave. Fixing it means replacing the `png` crate, out of this wave's scope; no `deny.toml` exception was added.
