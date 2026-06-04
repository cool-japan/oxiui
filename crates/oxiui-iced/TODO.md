# oxiui-iced TODO

## Status
Working iced adapter (~235 SLOC across lib.rs, adapter.rs, theme.rs). Provides `IcedUiCtx` that collects widget specs (heading/label/button) and builds an iced `Column` element. Theme mapping via `palette_to_iced_theme()`. IME forwarding is a no-op stub (iced 0.14 lacks public IME injection API). Message round-trip (button click state) is wired. Main gaps: limited widget coverage (only heading/label/button), no text input, no layout control beyond vertical column.

## Core Implementation
- [x] Text input widget: `UiCtx::text_input()` → `iced::widget::text_input`, two-way binding (display current value, emit on-change), placeholder text, password mode (~120 SLOC)
    - **Goal:** iced adapter implements all 14 `UiCtx` methods via a retained Elm/imperative bridge; crate-local only — does NOT touch `crates/oxiui/`. `WidgetSpec` made public + recursive.
    - **Design:** `WidgetSpec` public with new variants (TextInput/Checkbox/Slider/Dropdown/Image/Separator/Spacer/Scroll/Tooltip/Popup/Modal). Single shared `next_id` (replaces per-type `button_count`). `WidgetState{Text,Checked,Slider,Selected}`. `IcedConfig{pending_clicks,state:HashMap<usize,WidgetState>,spacing,padding}` replaces bare HashSet arg. `apply_message(state,clicks,msg)` driver. Expand `Message` with TextChanged/CheckboxToggled/SliderChanged/DropdownSelected. `into_iced_element` → recursive `build_one`. `image()` returns `unsupported()` (iced "image" feature OFF). Modal/popup are best-effort within-cell. `IcedNullCtx::recording()` call-log. Migrate own `tests/adapter.rs` to `IcedConfig::default()`.
    - **Files:** `src/adapter.rs` + `src/lib.rs` only. Do NOT edit `crates/oxiui/`.
    - **Tests:** new `tests/widgets.rs` (headless, pure state machine): spec+synthesis, shared-id regression, recursion+id threading, message round-trip, materialization, NullCtx log.
    - **Risk:** Space::height(f32) may need iced::Length::Fixed(size); tooltip::Position::Top variant name. Report deviated if API differs.
- [x] **iced: verify the 12 already-implemented `[~]` widgets with passing tests, add with_spacing/padding + theme-mapping** (completed 2026-05-29)
  - **Goal:** the round-2 adapter implements checkbox/slider/dropdown/separator/spacer/scroll/tooltip/modal/extended-messages/state/config/NullCtx-recording/public-WidgetSpec but they remain `[~]`. Deliverable: gate each `[~]→[x]` flip on a test that actually passes — not the classification claim.
  - **Design:** add tests/ coverage proving each widget's state-machine round-trip via apply_message (text/checkbox/slider/dropdown), spec collection + materialization (into_iced_element builds), NullCtx recording. Additive API: with_spacing(px)/with_padding(px) builder setters on IcedConfig; palette_to_iced_theme_ext(&dyn Theme) added alongside original; enrich TextInputResponse with focused:bool (headless-approximate).
  - **Files:** `crates/oxiui-iced/src/{adapter.rs,lib.rs,theme.rs}`; `crates/oxiui-core/src/response.rs`; `tests/{widgets.rs,theme.rs}`.
  - **Tests added (10):** test_state_persists_across_frames, test_into_iced_element_with_nested_scroll, test_with_spacing_reflected_in_config, test_with_padding_reflected_in_config, test_text_input_focused_is_false_in_headless, palette_to_iced_theme_ext_dark_produces_custom, palette_to_iced_theme_ext_matches_palette_variant (+ 3 pre-existing ext widget tests).
  - **Deviation:** image() fallback-only (iced "image" feature OFF); palette error/warning/success fields absent from Palette → mapped to muted/surface approximations with doc note.
- [x] Checkbox widget: `UiCtx::checkbox()` → `iced::widget::checkbox`, label + checked state, on-toggle message (~40 SLOC)
- [x] Slider widget: `UiCtx::slider()` → `iced::widget::slider`, range + value + step, on-change message (~50 SLOC)
- [x] Dropdown / pick-list: `UiCtx::dropdown()` → `iced::widget::pick_list`, options list, selected value, on-select message (~60 SLOC)
- [x] Image widget: `UiCtx::image()` → `iced::widget::image`, load from bytes, aspect ratio handling (~40 SLOC) — iced `features=["image","advanced"]` enabled; `build_one` uses `Handle::from_path(uri)`; `IcedUiCtx::image()` and `IcedNullCtx::image()` return `supported:true` (2026-05-29)
    - **Deviation:** data: URI / base64 decoding not implemented — path-based images only; data URIs need further work
- [x] Separator and spacer: `UiCtx::separator()` → `iced::widget::horizontal_rule`, `spacer()` → `iced::widget::Space` (~20 SLOC)
- [x] Scroll area: `UiCtx::scroll_area()` → `iced::widget::scrollable` with direction control (~40 SLOC)
- [x] Layout control: `UiCtx::horizontal()` closure → `iced::widget::row`, nested `vertical()` → `iced::widget::column`, `UiCtx::grid()` for grid layouts (~80 SLOC)
    - **Goal:** wire new `UiCtx::horizontal/vertical/grid/menu_bar/rich_text/drag_source/drop_target` default methods to iced `WidgetSpec` variants after Stage 1's UiCtx extension lands (Stage 2 / Slice I) (planned 2026-05-29)
    - **Design:** `WidgetSpec` gains `Horizontal(Vec<WidgetSpec>)`, `Vertical(Vec<WidgetSpec>)`, `Grid{cols:usize,children:Vec<WidgetSpec>}`, `RichText(Vec<IcedSpan>)` where `IcedSpan{text,color:Option<[u8;4]>,bold,size}`; `IcedUiCtx` overrides horizontal/vertical/grid via child `IcedUiCtx` collector; `menu_bar`/`drag_source`/`drop_target` return `unsupported()` without panic (iced 0.14 has no native menu/drag); `RichText` → `iced::widget::rich_text` (fallback to Column of text() per span if API absent); `build_one` materializes new variants: Horizontal→`Row::with_children`, Vertical→`Column::with_children`, Grid→rows of cols in nested Row/Column
    - **Files:** `crates/oxiui-iced/src/adapter.rs`
    - **Tests:** horizontal N children → Row; vertical → Column; grid 3-col/6-items → 2 rows; rich_text maps color; menu_bar/drag unsupported without panic
    - **Risk:** iced 0.14 `rich_text` API — check if `iced::widget::rich_text` exists; if not, fallback to column of `text()` and report deviated
- [x] Tooltip: `UiCtx::tooltip()` → `iced::widget::tooltip` with position control (~30 SLOC)
- [x] Modal / overlay: layered container for dialog overlays using `iced::widget::stack` (~60 SLOC)
- [x] Extended message types: `Message::TextChanged(usize, String)`, `Message::CheckboxToggled(usize, bool)`, `Message::SliderChanged(usize, f64)`, `Message::DropdownSelected(usize, usize)` -- generalized message enum for all widget interactions (~80 SLOC)
- [x] State management: `IcedUiCtx` should track per-widget state across frames (text input values, checkbox states, slider positions) via `HashMap<usize, WidgetState>` (~100 SLOC)
- [ ] IME support: monitor iced upstream for IME API exposure; implement `forward_ime_event()` properly when iced adds `TextInput::ime_preedit`/`ime_commit` API (~60 SLOC when available) **BLOCKED: upstream iced 0.14 has no public IME injection API (`ime_preedit`/`ime_commit` not exposed on `text_input`); revisit when iced exposes IME hooks**
- [x] Theme mapping expansion: `palette_to_iced_theme()` — added `text_input_style_from_palette`, `scrollable_style_from_palette` (+ `*_from_theme` variants) in `theme.rs`; border.color set from palette.primary; scroller background from palette.primary; rail background from palette.surface (2026-05-29)
- [x] Custom iced widget: `OxiIcedWidget` implementing `iced::advanced::Widget` (~150 SLOC) — `pub struct OxiIcedWidget{spec,width,height}` implementing `Widget<Msg,Theme,Renderer>`; `oxi_widget(spec)` ctor; `size()`, `layout()`, `draw()` (stub) implemented; requires iced `features=["advanced"]` (2026-05-29)
    - **Deviation:** `draw()` is a no-op stub — delegating draw to a materialized Element requires concrete Theme/Renderer + Tree plumbing beyond this slice
- [x] Keyboard shortcut handling: `map_iced_keyboard_event`, `map_iced_key`, `map_iced_modifiers` free fns (~60 SLOC) — maps `KeyPressed`/`KeyReleased` → `UiEvent::KeyDown`/`KeyUp`; character keys → `Key::Character`; named keys (Enter/Escape/Tab/Arrow*/F1-F12/Home/End/PageUp/PageDown) → specific variants; unknown → `Key::Named(format!("{:?}"))` (2026-05-29)
- [x] Window title update: dynamic title changes via `iced::Application::title` callback tracking (~20 SLOC)
    - **Deviation:** `oxiui-iced` does not host an `iced::Application` — it is a WidgetSpec collector. Added `title: String` field + `with_title()` builder to `IcedConfig` as the seam a host's `title()` callback reads. A full live-callback wire requires the host crate (`oxiui`) to plumb `config.title` through its Application wrapper. Tests: `test_iced_title_from_config`, `test_iced_title_default_is_empty`, `test_iced_title_chain_builder`.

## API Improvements
- [x] `IcedUiCtx::new()` should accept a configuration struct instead of just `HashSet<usize>` for pending clicks
- [x] Return `WidgetResponse` from all widget methods (not just `ButtonResponse` from `button()`), carrying interaction state (focused, hovered, changed)
    - **Note:** `UiCtx::heading` and `UiCtx::label` return `()` by trait contract (not `WidgetResponse`). All other `IcedUiCtx` methods return the correct response type (`ButtonResponse`, `CheckboxResponse`, `SliderResponse`, `DropdownResponse`, `WidgetResponse`) matching the `UiCtx` trait signatures. Verified by 8 tests in `tests/polish.rs`.
- [x] `palette_to_iced_theme_ext(&dyn Theme)` added alongside original `palette_to_iced_theme(&Palette)` (original signature preserved)
- [x] `IcedNullCtx` optionally records calls for test assertions via `IcedNullCtx::recording()` call log
- [x] Make `WidgetSpec` public for advanced users who want to inspect/modify the widget tree before materialization
- [x] Add `IcedConfig::with_spacing(px)` and `with_padding(px)` builder methods for layout customization

## Testing
- [x] Text input: apply_message round-trip via `TextChanged` message; state carries forward across frames
- [x] Checkbox: toggle via `CheckboxToggled` message round-trip; reflects prior state
- [x] Slider: `SliderChanged` message round-trip; seeds correctly when no prior state
- [x] Layout: nested `horizontal()` inside `vertical()`, verify element tree structure (~40 SLOC)
  - **Goal:** Test that `horizontal()` inside `vertical()` materializes as a nested `Row` inside a `Column`.
  - **Design:** Build an `IcedUiCtx` (or use the existing `IcedNullCtx`), call `vertical(|ui| { ui.horizontal(|ui| { ui.label("hi"); }); })`, assert the resulting `WidgetSpec` tree has `Vertical([Horizontal([Label("hi")])])`.
  - **Files:** `crates/oxiui-iced/tests/slice_s8_tests.rs`.
  - **Tests:** nested horizontal/vertical builds correct spec tree; spec materializes without panic (2026-05-29).
  - **Risk:** None — unit test of existing adapter logic.
- [x] Theme mapping: `palette_to_iced_theme_ext` and `palette_to_iced_theme` produce Custom variant; name matches
- [x] State persistence: frame 1 apply_message → frame 2 IcedUiCtx::new with same config → text persists
- [x] Message round-trip: button pressed → apply_message → ButtonResponse::clicked true
- [x] `IcedNullCtx` call recording: null_ctx_recording_logs_calls covers all 11 method variants
- [x] Headless app test: boot `iced::application` in headless mode, run 3 frames, verify no panic (~40 SLOC)
  - **Goal:** Smoke test that the iced adapter can be exercised for 3 "frames" of widget spec materialization.
  - **Design:** iced 0.14 has no real headless event loop. Implemented as 3-round spec-materialization smoke test: `IcedUiCtx` built 3 times with label/button/horizontal calls, specs non-empty, round 3 materialises without panic (2026-05-29).
  - **Files:** `crates/oxiui-iced/tests/slice_s8_tests.rs`.
  - **Tests:** `test_headless_spec_materialization_three_rounds` + 3 individual round tests pass.
  - **Deviation:** iced 0.14 has no headless application loop; implemented as spec-materialization smoke test.

## Performance
- [x] Avoid rebuilding the entire `Column` when only one widget changed: diff previous vs current `WidgetSpec` list, reuse unchanged elements
    - **Design:** Added `spec_fingerprint(&WidgetSpec) -> u64` (Debug-hash, deterministic within a process run) and `SpecCache{fingerprints,rebuild_count}` with `sync(&[WidgetSpec]) -> bool` + `rebuild_count()`. `sync` compares per-index fingerprints; on any change (length or content) increments `rebuild_count` once. Exported from `lib.rs`. Tests in `tests/fingerprint_tests.rs` (8 tests). iced `Element` is not `Clone` so elements cannot be cached; fingerprints enable diagnostics and upstream skip-work. (2026-05-29)
    - **Deviation:** iced `Element` is not `Clone` — per-element caching is not feasible. `SpecCache` tracks whether a rebuild was needed; the iced tree is always rebuilt fresh each frame (required by iced). The dirty check is valuable for skipping upstream computation when specs are identical.
- [x] Pre-allocate `specs` vector with expected capacity based on previous frame's widget count
    - **Design:** Added `spec_capacity_hint: usize` to `IcedConfig` + `with_spec_capacity(hint)` builder. `IcedUiCtx::new` uses `Vec::with_capacity(hint.max(8))`. Added `spec_count()` accessor so callers feed last-frame count into next frame's config. Tests: `test_iced_specs_pre_alloc_zero_hint_does_not_panic`, `test_iced_specs_pre_alloc_uses_hint`, `test_iced_spec_count_reflects_widget_count`, `test_iced_spec_capacity_round_trip_across_frames`.
- [x] Cache `palette_to_iced_theme()` result: only recompute when palette actually changes (equality check)
    - **Design:** Added `ThemeCache` struct to `adapter.rs` with `last_palette: Option<Palette>` + `cached_theme: Option<iced::Theme>`. `get_or_compute(&palette)` compares all six `Color` fields individually (since `Palette` doesn't derive `PartialEq`). On a miss, calls `palette_to_iced_theme` and stores both. Exported via `pub use adapter::ThemeCache` in `lib.rs`. Tests: `test_iced_palette_cache_hit_preserves_correctness`, `test_iced_palette_cache_invalidates_on_change`, `test_iced_theme_cache_default_starts_empty`.
- [x] Minimize `String` clones in `WidgetSpec`: use `Cow<'_, str>` where possible
  - **Goal:** `WidgetSpec` text fields use `Cow<'static, str>` so static string literals borrow without allocating.
  - **Design:** Changed `WidgetSpec::Label`, `Heading`, `Button.label`, `TextInput.value/placeholder`, `Checkbox.label`, `Image.uri`, `Tooltip.text`, `Modal.title` from `String` to `Cow<'static, str>`. `IcedUiCtx` uses `Cow::Owned(t.to_owned())` for `&str` args; `build_one` calls `.into_owned()` / `.as_ref()` at iced widget boundary. `adapter.rs` stays 1156 lines (no splitrs needed) (2026-05-29).
  - **Files:** `crates/oxiui-iced/src/adapter.rs`, `crates/oxiui-iced/tests/slice_s8_tests.rs`.
  - **Tests:** `test_cow_borrowed_for_static_str_label/heading/button`, `test_cow_owned_from_runtime_string` all pass.
  - **Risk:** Mitigated — all existing 87 tests pass, 0 clippy warnings.

## Integration
- [x] `oxiui-core` integration: implement expanded `UiCtx` methods as they are added to the trait (2026-06-03)
    - **Implemented:** `text_area()` → `WidgetSpec::TextArea { id, value, min_rows }`, `Message::TextAreaChanged`, `WidgetState::TextArea`. Materialises as a vertical stack of `text_input` widgets (best-effort; iced 0.14's `text_editor` requires `Content<Renderer>` which cannot be stored in a `'static` spec). `IcedNullCtx::text_area()` records the call. 8 tests in `tests/widgets.rs` cover state round-trip, cursor_pos approximation, materialisation, and NullCtx recording.
    - **Deviation:** `text_area` is rendered as stacked `text_input` widgets; true multi-line `text_editor` is deferred until iced exposes a simpler API.
- [ ] `oxiui-text` integration: use `TextPipeline` for consistent text rendering when iced exposes custom text layout hooks (BLOCKED: upstream iced 0.14 has no custom text layout hook API)
- [x] `oxiui-theme` integration: consume `DesignTokens` for spacing, border-radius, shadow in iced widget styles — `DesignTokensAdapter::from_tokens(&DesignTokens, &TypographyScale)` exposes tokens as iced primitives (body_text_size, headline_text_size, standard_padding, border_radius); `palette_and_tokens_to_iced_theme` convenience wrapper added. Deviation: iced 0.14 Theme::Custom holds only colours — tokens cannot be embedded globally; per-widget wiring is a follow-up. (2026-05-29)
- [x] `oxiui-table` integration: `render_iced()` already works; expanded with column sorting, selection, and scroll-offset tracking via `scrollable::Id` (2026-06-03)
    - **Added:** `render_iced_sortable()` — clickable column headers emitting `on_sort_toggle(col_idx)` messages + optional `scrollable_id: Option<iced::widget::Id>` for scroll-event tracking. `render_iced_with_selection()` — row highlighting for selected rows via `SelectionModel`; selected rows wrapped in a highlighted `button` container; optional `scrollable_id`. Both functions exported from `oxiui-table::lib.rs` behind the `iced-table` feature.
    - **Deviation:** iced 0.14 has no way to auto-derive `scroll_offset` from the widget event stream without `scroll_to` subscriptions; `scroll_offset` remains caller-tracked.
- [x] `oxiui-accessibility` integration: bridge OxiUI's `WidgetSpec` tree to `accesskit::TreeUpdate` via `oxiui-accessibility` infrastructure (2026-06-03)
    - **Design:** `src/a11y_bridge.rs` (feature-gated behind `a11y` feature). `spec_to_a11y_tree(&[WidgetSpec], &IcedA11yConfig) -> accesskit::TreeUpdate` — converts a spec tree under a synthetic root `Window` node. `spec_to_a11y_node(WidgetSpec, &mut u64) -> Option<A11yNode>` for per-spec conversion (depth-first counter). `IcedA11yConfig { root_label, id_start }` with builder methods. Decorative specs (Separator, Spacer) are omitted. 15 tests in `tests/a11y_bridge_tests.rs`.
    - **Deviation:** iced 0.14 has no built-in AccessKit support; the bridge operates on `WidgetSpec` (before iced rendering) rather than through iced internals.
- [x] COOLJAPAN ecosystem: iced is Pure Rust; no additional C/C++ dependencies confirmed; IME API absent in iced 0.14 (monitored) (2026-06-03)

## Proposed follow-ups
- **True modal/popup overlay:** lift overlay specs to root stack![base,overlay] in the facade view() — needs facade cooperation.
- **iced resize handles:** drag primitive not available in iced 0.14; defer column resize to a future iced version or custom widget.
- **IME support:** wire when iced exposes ime_preedit/commit upstream (monitored).
