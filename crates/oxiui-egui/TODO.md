# oxiui-egui TODO

## Status
Functional egui adapter (~116 SLOC). Implements `MockUiCtx` wrapping `egui::Ui` with heading/label/button forwarding. Provides `palette_to_egui_visuals()` for theme mapping, `forward_event_to_egui()` for IME event injection, and `load_font_into_egui()` for OxiFont byte loading. This is the most complete adapter crate -- the main gaps are expanded widget coverage, richer theme mapping, and accessibility bridging.

## Core Implementation
- [x] Expanded `UiCtx` widget forwarding: `text_input()` → `egui::TextEdit::singleline`, `checkbox()` → `egui::Checkbox`, `slider()` → `egui::Slider`, `dropdown()` → `egui::ComboBox`, `image()` → `egui::Image`, `separator()`, `spacer()` (~200 SLOC)
    - **Goal:** egui adapter implements all 14 `UiCtx` methods against real egui widgets; misnamed `MockUiCtx` becomes `EguiUiCtx`. Crate-local only — does NOT touch `crates/oxiui/`.
    - **Design:** struct `EguiUiCtx<'a>{ ui, last_response, id_seq }`. Per-method: text_input→TextEdit::singleline, checkbox→ui.checkbox, slider→Slider::new (RangeInclusive<f64>), dropdown→ComboBox::from_id_salt.show_index, image→Image::from_uri+fit_to_exact_size, separator→ui.separator(), spacer→ui.add_space(f32), scroll_area/popup/modal→reborrow into child EguiUiCtx inside egui container (use egui::Modal for modal, egui::Window for popup), tooltip→last_response.take().on_hover_text. `response()` accessor, `load_font_into_egui`→Result<(),UiError> via oxiui_text validation, `EguiAdapter` builder. Do NOT edit `crates/oxiui/`.
    - **Files:** `src/lib.rs` only. Grep `tests/` and `examples/` inside this crate and update any `MockUiCtx` referent.
    - **Tests:** new `tests/widgets.rs` via egui headless: text_input, checkbox, slider, dropdown, separator, spacer, image, scroll_area, tooltip, popup, modal, response_accessor, load_font_empty_bytes_is_err, ime forwarding.
    - **Risk:** egui 0.34.2 checked; closure reborrow is the trickiest part. Report deviated if API differs.
    - **Defer:** multi-font-family, rich-text spans, full keyboard/event mapping, layout forwarding.
- [x] Scroll area forwarding: `scroll_area()` → `egui::ScrollArea` with configurable horizontal/vertical/both, scroll-to API (~60 SLOC)
  - **Goal:** `EguiUiCtx::scroll_area` overrides the unsupported default, forwarding to `egui::ScrollArea` with a child `EguiUiCtx`.
  - **Design:** Mirror the Round-4 horizontal/vertical pattern. `scroll_area(closure)` → `egui::ScrollArea::vertical().show(ui, |ui| { let mut child = EguiUiCtx::new(ui, ctx); closure(&mut child); })`. Return `WidgetResponse::supported(false)` (no interaction result from scroll itself). Adapt to egui 0.34 API; report `deviated` if the API differs.
  - **Files:** `crates/oxiui-egui/src/lib.rs`.
  - **Tests (headless):** `scroll_area` with two child labels executes the closure (children rendered, no panic); returns supported response.
  - **Risk:** egui 0.34 ScrollArea API details. Mitigate: adapt, deviated if absent.
- [x] Tooltip forwarding: `tooltip()` → `egui::show_tooltip_at_pointer` / `egui::Response::on_hover_text` (~30 SLOC)
  - **Goal:** `EguiUiCtx::tooltip` overrides default, forwarding to egui's tooltip system.
  - **Design:** `tooltip(closure)` → use `egui::show_tooltip_at_pointer` or `Response::on_hover_ui` pattern. Since tooltip in UiCtx takes a content closure (not a parent response), use a region that renders invisibly to position the tooltip, then call `ui.ctx().show_tooltip_ui(...)`. Adapt to egui 0.34 API.
  - **Files:** `crates/oxiui-egui/src/lib.rs`.
  - **Tests:** tooltip closure executes without panic; returns supported.
  - **Risk:** egui tooltip API may differ from assumption. Mitigate: deviated if needed.
- [x] Popup / modal forwarding: `popup()` → `egui::Area` + `egui::Frame`, modal overlay with background dimming (~80 SLOC)
  - **Goal:** `EguiUiCtx::popup` overrides default, creating a floating popup via `egui::Area`.
  - **Design:** `popup(closure)` → `egui::Area::new(id).show(ctx, |ui| { let mut child = EguiUiCtx::new(ui, ctx); egui::Frame::popup(ui.style()).show(ui, |ui| { closure(&mut child); }); })`. Return supported. Adapt to egui 0.34 Area API.
  - **Files:** `crates/oxiui-egui/src/lib.rs`.
  - **Tests:** popup closure executes without panic; returns supported.
  - **Risk:** egui 0.34 Area/Frame API. Mitigate: adapt, deviated if absent.
- [x] Menu bar forwarding: `egui::menu::bar` → top-level menu, context menu on right-click (~60 SLOC)
    - **Goal:** override `UiCtx::menu_bar()` in `EguiUiCtx` using `egui::menu::bar`; depends on Stage 1/A adding `menu_bar` as a UiCtx default method (planned 2026-05-29)
    - **Design:** `fn menu_bar(&mut self, content:&mut dyn FnMut(&mut dyn UiCtx))->WidgetResponse { egui::menu::bar(self.ui, |ui|{ let mut child=EguiUiCtx::new(ui); content(&mut child); }); WidgetResponse::default() }`; follows existing child-context pattern (scroll_area:129/popup:147/modal:162); context menu via `Response::context_menu` on the bar response if needed
    - **Files:** `crates/oxiui-egui/src/lib.rs`
    - **Tests:** invoke menu_bar with a closure that calls button; verify no panic; WidgetResponse::default() returned
    - **Risk:** egui::menu::bar API — check egui 0.34 docs; may be `egui::menu::bar(ui, |ui|{...})` or `ui.menu_button`; adapt and report deviated if shape differs
- [x] Layout forwarding: `horizontal()`, `vertical()`, `grid()` → `egui::Ui::horizontal`, `egui::Ui::vertical`, `egui::Grid` (~40 SLOC)
    - **Goal:** override `UiCtx::horizontal()`, `vertical()`, `grid(cols,f)` in `EguiUiCtx`; depends on Stage 1/A (planned 2026-05-29)
    - **Design:** `horizontal`: `self.ui.horizontal(|ui|{let mut c=EguiUiCtx::new(ui);content(&mut c);})`; `vertical`: `self.ui.vertical(...)`; `grid(cols,f)`: `egui::Grid::new(egui::Id::new("oxi_grid")).num_columns(cols).show(self.ui,|ui|{let mut c=EguiUiCtx::new(ui);f(&mut c);})` — uses same child-context pattern as scroll_area/popup
    - **Files:** `crates/oxiui-egui/src/lib.rs`
    - **Tests:** horizontal: ≥2 items rendered side-by-side (both visible); vertical: items stacked; grid 3-col/6-items → 2 rows; all return WidgetResponse with supported:true
    - **Risk:** egui::Grid::new takes an id — use a stable id (salt "oxi_grid"); collision if multiple grids — accept for now, user can extend via id_source
- [x] Rich text forwarding: `RichText` spans → `egui::RichText` with bold/italic/color/size, layout job for complex text (~80 SLOC)
    - **Goal:** override `UiCtx::rich_text(spans:&[RichTextSpan])` in `EguiUiCtx` building an `egui::text::LayoutJob`; depends on Stage 1/A (planned 2026-05-29)
    - **Design:** for each `RichTextSpan`: append to `LayoutJob` with `TextFormat{color:Color32::from_rgba_unmultiplied(r,g,b,a), font_id:FontId::proportional(span.font_size), italics:span.italic}`; bold via `FontFamily::Name(arc_str)` if TextFormat lacks a bold field in egui 0.34 — check and adapt; call `self.ui.label(job)` at end
    - **Files:** `crates/oxiui-egui/src/lib.rs`
    - **Tests:** 3 spans with distinct colors → label rendered (no panic); color fields set correctly in LayoutJob; italic span sets italics:true
    - **Risk:** egui 0.34 TextFormat API — `bold` field may not exist; use font family override for bold; report deviated if LayoutJob API differs significantly
- [x] Drag-and-drop: egui's `sense(Sense::drag())` + `dnd_drop_zone` for widget reordering (~60 SLOC)
    - **Goal:** override `UiCtx::drag_source(id,f)` and `drop_target(accept,f)` in `EguiUiCtx`; depends on Stage 1/A (planned 2026-05-29)
    - **Design:** `drag_source(id,f)`: `self.ui.dnd_drag_source(egui::Id::new(id), id, |ui|{let mut c=EguiUiCtx::new(ui);f(&mut c);})`; `drop_target(accept,f)`: `let (_,payload)=self.ui.dnd_drop_zone::<u64,()>(egui::Frame::default(),|ui|{let mut c=EguiUiCtx::new(ui);f(&mut c);})`; `WidgetResponse{drag_dropped: payload.map(|p|accept.contains(&&*p)).unwrap_or(false), supported:true, ..Default::default()}`
    - **Files:** `crates/oxiui-egui/src/lib.rs`
    - **Tests:** drag_source: response has hovered field; drop_target: compiles and returns unsupported:false when no drag active; accept_ids filtering works
    - **Risk:** egui 0.34 DnD API — `dnd_drag_source`/`dnd_drop_zone` may differ from 0.28/0.30; check egui 0.34 changelog; report deviated if API absent
- [x] **egui: extended event forwarding, full keyboard mapping, custom OxiWidget, additive style/palette expansion, multi-font** (done 2026-05-29)
  - **Goal:** broaden the egui adapter additively — without touching the UiCtx trait (layout/menu/rich-text/drag deferred to round 4).
  - **Design:** extend `forward_event_to_egui` (already handles IME) to KeyPress→egui::Event::Key, Mouse→PointerMoved/Button, Resize→viewport; a full keyboard map (Ctrl/Alt/Shift/Meta + function/arrow/enter/esc/tab); OxiWidget wrapper impl egui::Widget over oxiui_core::Widget; additive style/palette expansion — KEEP `palette_to_egui_visuals(&Palette)` signature (populate error/warning/success in body) + ADD `palette_to_egui_visuals_with_tokens(&Palette,&DesignTokens)` mapping spacing/rounding/shadow into egui::Style; multi-font-family loading (sans/serif/mono/CJK priority) extending load_font_into_egui.
  - **Files:** `crates/oxiui-egui/src/lib.rs` (303) only; splitrs if nears 2000. palette_to_egui_visuals signature is preserved so no examples sweep needed (confirm with grep).
  - **Prerequisites:** none.
  - **Tests (~8, headless via egui::Context::run_ui):** inject KeyPress/Mouse/Resize → assert egui input queue; keyboard map covers modifiers+special keys; OxiWidget render() invoked (spy); existing palette_to_egui_visuals unchanged (regression) + new token variant maps fields; multi-font defs contain all families.
  - **Risk:** egui 0.34 API drift on Event/Key variants — adapt and report deviated on mismatch. Defer: layout/menu/rich-text/drag (UiCtx extension); clipboard bridge; web/eframe (browser-blocked).
- [x] Full keyboard event mapping: modifier keys (Ctrl/Alt/Shift/Meta), function keys, arrow keys, enter/escape/tab (~60 SLOC)
- [x] Clipboard bridge: `egui::Context::input().events` clipboard events → `oxiui-core` clipboard abstraction (~40 SLOC)
    - **Goal:** add `clipboard_get()` and `clipboard_set(text)` helpers in `EguiUiCtx` wiring to egui's clipboard output (planned 2026-05-29)
    - **Design:** `clipboard_get`: `self.ui.ctx().output(|o| if o.copied_text.is_empty() { None } else { Some(o.copied_text.clone()) })`; `clipboard_set(text)`: `self.ui.ctx().copy_text(text.to_string())`; try to wire to `oxiui_core::widget_ext::ClipboardProvider` if the adapter already holds one, else expose as standalone pub methods; these are NOT UiCtx trait methods (clipboard abstraction is separate)
    - **Files:** `crates/oxiui-egui/src/lib.rs`
    - **Tests:** copy_text sets copied_text in egui output; clipboard_get returns it; round-trip
    - **Risk:** egui clipboard `output` is write-side only (copy); read-side (paste) requires winit clipboard access — document this limitation; clipboard_get returns only text set in the same frame via copy_text
- [x] Custom egui widget: `OxiWidget` struct implementing `egui::Widget` that wraps any `oxiui_core::Widget` trait object, enabling OxiUI widgets to be placed directly in egui layouts (~100 SLOC)
- [x] egui style mapping expansion: map `DesignTokens` spacing/rounding/shadow to `egui::Style::spacing`, `egui::CornerRadius`, `egui::Shadow` (~60 SLOC)
- [x] Multiple font family support: load multiple OxiFont files (sans/serif/mono/CJK), map to egui font families with priority ordering (~40 SLOC)

## API Improvements
- [x] Rename `MockUiCtx` to `EguiUiCtx` (the name "Mock" is misleading -- it is the real egui adapter, not a test mock)
- [x] Add `EguiUiCtx::response()` method to access the last `egui::Response` for advanced interaction queries (focus, drag, etc.)
- [x] `palette_to_egui_visuals_with_tokens()` added (signature of `palette_to_egui_visuals` preserved); `DesignTokens` maps spacing/radius to `egui::Style`
- [x] Return `Result<(), UiError>` from `load_font_into_egui` if font parsing fails (currently silent)
- [x] Builder pattern for egui adapter configuration: `EguiAdapter::new(ctx).with_palette(p).with_fonts(fonts).build()`

## Testing
- [x] `EguiUiCtx` widget forwarding: create egui context in headless mode, call heading/label/button through `UiCtx`, verify egui output (~80 SLOC)
  - **Goal:** Headless test coverage for scroll_area, tooltip, and popup forwarding.
  - **Design:** Using `egui::Context::default()` + `ctx.run()` pattern (established in Round-4 tests). Tests verify the three new forwarding methods execute their closures and return supported.
  - **Files:** `crates/oxiui-egui/tests/` (new or existing test file).
  - **Tests:** scroll_area child executed; tooltip no panic; popup no panic; all return `supported=true`; test egui::Event handling.
  - **Risk:** egui headless harness; deviated if API changed.
- [x] Palette mapping: every `Palette` field maps to a non-default egui visual, round-trip palette→visuals→check (~40 SLOC)
  - **Goal:** Palette→Visuals mapping is verified round-trip: set primary color, convert via `palette_to_visuals`, assert egui Visuals reflect the color.
  - **Design:** Build a `Palette` with known primary/secondary/background colors; call the existing `palette_to_visuals` (or equivalent) conversion; assert specific color fields match.
  - **Files:** `crates/oxiui-egui/tests/` (same test file as item 4).
  - **Tests:** round-trip primary color; round-trip background; widget_noninteractive color matches.
  - **Risk:** None — tests existing conversion function.
- [x] IME event forwarding: inject `ImePreedit`/`ImeCommit`, verify egui input queue contains corresponding `Ime` events (~40 SLOC) — verified via `test_ime_preedit_event_pushes_to_queue`, `test_ime_preedit_no_panic_in_frame`, `test_ime_commit_event_pushes_to_queue`, `test_ime_commit_no_panic_in_frame`
- [x] Font loading: load test TTF bytes, verify egui font definitions contain "OxiFont" in proportional and monospace families (~30 SLOC) — verified via `test_font_loading_valid_ttf_succeeds`, `test_font_loading_empty_bytes_err`, `test_stateful_adapter_valid_font_loaded_once` (deviation: `Context` has no public `FontDefinitions` getter; success verified via `Result::is_ok()` + panic-free frame)
- [x] Extended event forwarding: inject `KeyPress`, `Mouse`, `Resize`, verify egui events (~40 SLOC) — verified via `test_key_press_string_event_forwarded`, `test_key_up_event_forwarded`, `test_mouse_click_event_forwarded`, `test_resize_event_updates_viewport`, `test_resize_forward_no_panic`
- [x] Custom `OxiWidget` test: wrap a dummy `Widget` impl, render in egui, verify `render()` was called (~30 SLOC) — verified via `test_custom_widget_renders`, `test_custom_widget_painter_no_panic`, `test_egui_adapter_headless_frame_cycle`
- [x] Snapshot test: render a reference UI through `EguiUiCtx`, compare egui output against baseline (~80 SLOC) — implemented in `tests/snapshot_tests.rs`; verifies deterministic shape count across two frames for label/heading/button/separator/horizontal-layout/rich-text; also verifies text/mesh presence in reference UI. Pixel-comparison skipped (platform-dependent); structural shape-count comparison used instead.

## Performance
- [x] Minimize allocations in `MockUiCtx::button()`: avoid `to_string` when egui accepts `&str` natively — `EguiUiCtx::button()` already forwards `&str` to egui with zero allocation; confirmed by `test_button_no_string_alloc_contract`.
- [x] Cache visuals: recompute `palette_to_egui_visuals` only when theme changes, not every frame — `StatefulEguiAdapter` compares palette fields; recomputes only on change. Instrumented via `visuals_recompute_count`.
- [x] Font definition caching: only call `ctx.set_fonts()` once at init, not per-frame — `StatefulEguiAdapter::apply` guards behind `fonts_loaded: bool`; `fonts_load_count` tracks attempts.

## Integration
- [x] `oxiui-core` integration: `EguiUiCtx` implements all 16 current `UiCtx` methods including `label_styled`/`heading_styled` (now overridden to forward `TextStyle` fields to `egui::RichText`); `text_input`, `checkbox`, `slider`, `dropdown`, `image`, `separator`, `spacer`, `scroll_area`, `tooltip`, `popup`, `modal`, `horizontal`, `vertical`, `grid`, `menu_bar`, `rich_text`, `drag_source`, `drop_target` all delegating to egui widgets. Verified via `tests/styled_text_tests.rs` (13 tests).
- [ ] `oxiui-text` integration: optionally bypass egui's text layout and use `TextPipeline` for shaping, feeding egui custom `TextLayoutJob` results. **DEFERRED: egui 0.34 does not expose a stable `TextLayoutJob` injection point from outside; full integration requires upstream egui API (custom text shaping callback); `oxiui_text::TextPipeline::from_bytes` is already used for font validation in `load_font_into_egui`.**
- [x] `oxiui-theme` integration: consume `DesignTokens`, `TypographyScale`, `ShadowSpec` for full egui style mapping (not just `Visuals`)
- [x] `oxiui-table` integration: `Table::render_egui()` already works; expanded table features (sorting, selection) verified via `tests/table_integration_tests.rs` (12 tests covering `HeaderSortState::toggle`, `SelectionModel` single/multi/clear, `EguiTableState` edit/expand, multi-frame stability). Bridge helpers added to `src/lib.rs` under `table_bridge` feature module.
- [x] `oxiui-accessibility` integration: egui has built-in AccessKit support; `oxiui_egui::a11y` module bridges `A11yTree`→`TreeUpdate` via `oxiui_tree_to_accesskit`, `diff_a11y_trees`, and stateful `A11yEguiBridge`. Verified via `tests/a11y_integration_tests.rs` (15 tests). Feature-gated behind `a11y` Cargo feature.
- [ ] `oxiui-web` integration: eframe's wasm backend uses this adapter; ensure all widget forwarding works in browser context. **BLOCKED: requires wasm32 target + browser environment; all widget forwarding code is pure-Rust and should work on wasm32 in theory, but headless test verification requires a WASM runtime not available in this environment.**
- [x] COOLJAPAN ecosystem: policy validated — oxiui-egui has zero C/C++ dependencies; egui is Pure Rust; eframe's wgpu backend uses OS-provided GPU drivers loaded at runtime (not linked at compile time). Default features are 100% Pure Rust. All new dependencies (oxiui-table, oxiui-accessibility, accesskit) are Pure Rust.

## Proposed follow-ups
- **Multi-font-family support:** FontSpec→FontFamily mapping table; current loader only does Proportional+Monospace.
- **Rich-text spans forwarding:** needs an OxiUI rich-text input type flowing through UiCtx first.
- **Full keyboard/event mapping in `forward_event_to_egui`:** KeyDown/Mouse mapping alongside existing IME.
- **Layout forwarding:** horizontal/vertical/grid layouts need a core UiCtx trait extension first.
