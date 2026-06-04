# oxiui-accessibility TODO

## Status
Property-rich, incrementally updatable a11y layer. Defines `A11yNode`, `A11yTree`, `WidgetRole`, `A11yNodeProps`, `A11yNodeBuilder`, `CheckedState`, `LiveSetting`, `TextCaret`, `TextSelection`, and `Toggled3`. `A11yTree::build()` and `build_and_store()` walk a node tree depth-first and produce an `accesskit::TreeUpdate`. `A11yTree::diff()` computes minimal deltas. Focus tracking, live-region `announce()`, fluent builder, 30-variant `WidgetRole` with `Display`, full prop set (description, placeholder, key_shortcut, disabled, expanded, selected, checked, value range, labelled_by, described_by, controlled_by, owns). OsA11yPrefs (high-contrast/reduced-motion), A11yForest multi-window, NodePool, DirtyTracker, TabOrder nav, ActionDispatcher, build_table_a11y, synthesize_text_run_children, widget_bridge (Widget→A11yNode bridge, build_a11y_tree, A11yWidgetNode, NodeIdAllocator), text_bridge (text-bridge feature: TextInput/TextArea→A11yNode, 14 tests), FocusRing::ring_rect()/is_visible() render helpers (4 tests). Headless-testable. 144 tests (--all-features), 126 tests (default features), 0 warnings.

## Slice 3 Implementation Plan

### Completed [x]

- [x] Expanded role mapping: `WidgetRole::Checkbox`, `Slider`, `ProgressBar`, `Tab`, `TabPanel`, `Menu`, `MenuItem`, `Dialog`, `Alert`, `Tooltip`, `Tree`, `TreeItem`, `ListItem`, `Link`
- [x] Landmark roles: `WidgetRole::Banner`, `Navigation`, `Main`, `Complementary`, `ContentInfo`
- [x] `WidgetRole` implements `Display` for debugging/logging
- [x] `From<WidgetRole> for accesskit::Role` — all 29 variants mapped
- [x] `A11yNodeProps` struct: description, placeholder, key_shortcut, disabled, expanded, selected, checked, value_now/min/max/step, text_value, text_selection, labelled_by, described_by, controlled_by, owns
- [x] `CheckedState` type alias for `Toggled3` (False/True/Mixed)
- [x] `TextSelection` struct (anchor + focus byte offsets)
- [x] `A11yNode` extended with `props: A11yNodeProps` and `text_content`
- [x] `A11yNode::simple()` constructor for easy creation
- [x] Props applied during `collect_nodes` → `apply_props()`
- [x] `A11yNodeBuilder` fluent builder (src/builder.rs): new, label, description, placeholder, key_shortcut, disabled, expanded, selected, checked, value, text, text_selection, labelled_by, described_by, controlled_by, owns, child, build, build_with_children
- [x] `A11yTree::build_and_store()` — builds and retains snapshot for diff
- [x] `A11yTree::diff(old, new) -> TreeUpdate` — minimal delta computation
- [x] Focus tracking: `set_focus`, `focus`, `focus_update`
- [x] `LiveSetting` enum (Off/Polite/Assertive) with `From` → `accesskit::Live`
- [x] `A11yTree::announce(text, urgency)` — transient live-region node
- [x] 49 tests, 0 warnings, clippy clean

### Tests Added (Slice 3)

- [x] `widget_role_to_accesskit_role_all_variants` — all 29 WidgetRole variants
- [x] `node_property_description_survives_roundtrip`
- [x] `node_property_range_survives_roundtrip` — value_now/min/max/step
- [x] `relationship_labelled_by_propagated`
- [x] `tree_diff_add_child_produces_new_node`
- [x] `tree_diff_no_change_empty_delta`
- [x] `tree_diff_changed_prop_includes_modified_node`
- [x] `focus_set_get_roundtrip`
- [x] `focus_in_update_reflects_set_focus`
- [x] `live_region_announce_id_in_tree`
- [x] `widget_role_display_non_empty`
- [x] `builder_roundtrip_description`, `_placeholder`, `_key_shortcut`, `_disabled`, `_expanded`, `_selected`, `_checked`, `_value`, `_text`, `_labelled_by`
- [x] `large_tree_smoke_under_100ms` — 1000-node tree < 100ms
- [x] `node_property_placeholder_propagated`
- [x] `node_property_disabled_propagated`
- [x] `text_selection_caret`, `text_selection_range` (props module)
- [x] `a11y_node_props_default_is_empty`
- [x] All original 12 headless tree tests preserved

## Remaining / Deferred

- [ ] Platform adapter integration: wire `accesskit_winit::Adapter` to the winit event loop — blocked on real winit window (cannot run headless in CI). Tracked as integration work, not a unit-test blocker. **BLOCKED: requires live winit window; cannot run headless in CI**
- [x] Dynamic dirty-flag tracking on `A11yNode` (instead of Debug-string diff)
    - **Goal:** replace the Debug-string diff fallback with genuine per-node change tracking; add allocation-friendly node reuse and lazy property computation. Headless; crate-local.
    - **Design:** `A11yNode::content_hash()` hashes label/role/text_content/props/child-ID-list using `DefaultHasher`. `A11yTree` stores a `HashMap<NodeId, u64>` hash map alongside the AccessKit snapshot. `A11yTree::diff()` compares hashes O(1) per node instead of `format!("{:?}", accesskit::Node)`. `NodePool` provides active/free-list node reuse with `alloc`, `alloc_recycled`, `recycle`, `clear`. `Lazy<T>` wraps cached computed values with `get_or_compute`, `invalidate`, `set`, `get_if_clean`. Note: `accesskit::Node` doesn't impl PartialEq — dirty tracking lives on OxiUI's A11yNode wrapper only.
    - **Files:** `src/{tree.rs,lib.rs}`; new `src/{dirty.rs,pool.rs}`.
    - **Tests:** 17 new tests added (hash-based diff, NodePool lifecycle, Lazy, content hash stability); all 66 tests pass, 0 warnings.
- [x] **A11y: action handling, text accessibility (TextRun synthesis), tab-order audit, table a11y, OS-pref detection, multi-window TreeId** (implemented 2026-05-29)
  - **Goal:** fill out the headless-testable a11y surface left after round 2's hash-diff/pool work.
  - **Design:**
    - `action.rs` — `map_action(&accesskit::ActionRequest)->Option<A11yAction>` mapping Click/Focus/ScrollIntoView/SetValue/Increment/Decrement into OxiUI A11yAction enum (pure mapping; no live adapter).
    - Text a11y — synthesize Role::TextRun child nodes carrying TextPosition/TextSelection from existing TextSelection type for caret/selection exposure.
    - `nav.rs` — tab-order audit: compute focusable-node order from tree (respecting additive tab_index prop) + tab_order() walker + next_focus/prev_focus.
    - Table a11y — helpers emitting Role::Row/Cell/ColumnHeader with row/col indices.
    - OsA11yPrefs{high_contrast,reduced_motion} — pure-Rust best-effort query (env-var + documented platform hooks; no new dep), default off.
    - Multi-window: make TreeId configurable; support multiple A11yTrees keyed by window id.
  - **Files:** `src/{tree.rs,lib.rs,props.rs}`; new `src/{action.rs,nav.rs}`. Keep tree.rs (532) under 2000.
  - **Prerequisites:** none.
  - **Tests (~12):** each ActionRequest maps to right A11yAction; unknown→None; TextRun child synthesized with correct offsets; tab-order respects tab_index+DOM order; next/prev_focus wraps; table cells carry (row,col); column headers labeled; OS-pref defaults off + reads env override; two TreeIds stay isolated.
  - **Risk:** all headless; accesskit::Node lacks PartialEq so keep change-tracking on OxiUI wrapper. Defer: platform-adapter wiring (needs live winit window).
- [x] High-contrast / reduced-motion OS preference detection (covered by the slice above)
  - **Goal:** Confirm `OsA11yPrefs{high_contrast, reduced_motion}` is fully shipped and flip this marker to `[x]`.
  - **Design:** `OsA11yPrefs` already exists at `oxiui-accessibility/src/lib.rs:58-102` with `query()/query_from()` + tests. Implementation subagent should read the file, confirm presence, and flip `[~]` → `[x]`.
  - **Files:** `crates/oxiui-accessibility/src/lib.rs` (TODO marker only).
  - **Tests:** Existing OsA11yPrefs tests already pass.
  - **Risk:** None — stale marker flip only.
- [x] Table accessibility: row/column headers, cell indices
  - **Goal:** Structured table accessibility: `build_table_a11y(rows, cols, headers) -> A11yNode` wires ColumnHeader, TableRow, TableCell nodes with proper row/column indices.
  - **Design:** Using existing `column_header_node` / `table_row_node` / `table_cell_node` helpers (tree.rs), build `pub fn build_table_a11y(rows: usize, cols: usize, col_headers: &[&str]) -> A11yNode` that: creates a root TableRow containing the header ColumnHeader nodes (each with text + col_index), then for each data row creates a TableRow containing TableCell nodes with row_index + col_index associations. Wire parent-child relationships via the A11yNodeBuilder.
  - **Files:** `crates/oxiui-accessibility/src/{lib.rs,tree.rs}`.
  - **Tests:** `build_table_a11y(2, 3, &["A","B","C"])` → root has 3 ColumnHeader children + 2 TableRow children each with 3 TableCell children; cell[0][1] has row_index=0, col_index=1; column_header[2] has col_index=2.
  - **Risk:** accesskit role mapping correctness; index off-by-one. Mitigated by explicit structural assertions.
- [x] `TreeId` configurability for multi-window support
  - **Goal:** `A11yForest` supports multiple a11y trees for multi-window apps, each identified by a configurable `WindowA11yId`.
  - **Design:** `A11yForest` (lib.rs:116) gains: `register(id: WindowA11yId, tree: A11yTree)`, `unregister(id: WindowA11yId)`, `get(id: WindowA11yId) -> Option<&A11yTree>`, `get_mut(id) -> Option<&mut A11yTree>`, `windows() -> impl Iterator<Item=WindowA11yId>`. Internal storage: `HashMap<WindowA11yId, A11yTree>`. `WindowA11yId` is already `pub struct WindowA11yId(pub u64)` — supports distinct IDs per window.
  - **Files:** `crates/oxiui-accessibility/src/lib.rs`.
  - **Tests:** Register two distinct WindowA11yIds → forest holds both; get() returns the right tree; unregister removes it; windows() iterator yields both IDs.
  - **Risk:** None — additive to existing A11yForest struct.
- [x] `oxiui-core` integration: `Widget` trait `a11y_role()` / `a11y_label()`
- [x] Node pooling / arena for frame-rate-critical paths

## Proposed follow-ups
- **Dirty-flag diff is this round's focus** — the Debug-string fallback (tree.rs:diff) is replaced by hash-based tracking.
- **Dynamic tree updates via pool:** node pooling/arena reduces per-frame allocations.
- **High-contrast / reduced-motion OS detection:** OS query APIs, self-contained.

## Core Implementation (Legacy)
- [x] Expanded role mapping (~60 SLOC)
- [x] Node properties: description, placeholder, key_shortcut, disabled, expanded, selected, checked, value_now/min/max (~80 SLOC)
- [x] Relationship mapping: labelled_by, described_by, controlled_by, owns (~60 SLOC)
- [x] `A11yNode` builder pattern (~200 SLOC)
- [x] `A11yTree::diff(old, new) -> TreeUpdate` (~60 SLOC)
- [x] Focus tracking (~80 SLOC)
- [x] Announce utility / live regions (~30 SLOC)
- [x] `WidgetRole::Display` (~30 SLOC)
- [x] Landmark roles (~30 SLOC)
- [ ] Platform adapter integration: wire `accesskit_winit::Adapter` (~200 SLOC) **BLOCKED: requires live winit window; cannot run headless in CI**
- [x] Dynamic tree updates: dirty-flag tracking (~200 SLOC)
- [x] Action handling (~150 SLOC)
- [x] Text accessibility: full cursor/selection synthesis (~100 SLOC)
- [x] Keyboard navigation enforcement (~80 SLOC)
- [x] High-contrast mode detection (~80 SLOC)
- [x] Reduced-motion detection (~40 SLOC)
- [x] Focus indicator rendering (~40 SLOC)
- [x] Table accessibility (~80 SLOC)

## API Improvements
- [x] `A11yNode` builder pattern: `A11yNodeBuilder::new(id, role).label("OK").description("…").disabled(false).build()`
- [x] Automatic tree generation: `fn build_a11y_tree(root: &dyn Widget) -> A11yNode`
- [x] `A11yTree::diff(old, new) -> TreeUpdate` minimal delta
- [x] `WidgetRole` implements `Display`
- [x] Make `TreeId` configurable; support multiple a11y trees for multi-window
  - **Goal:** Same as item 3 above — covered by the A11yForest extension.
  - **Design:** Covered by item 3 (S6 slice; same files/tests).
  - **Files:** Same as item 3.
  - **Tests:** Same as item 3.
  - **Risk:** None.

## Testing
- [x] All 13 required Slice 3 tests pass
- [x] All 12 original headless tree integration tests pass
- [ ] Platform adapter smoke test (blocked on headless winit) **BLOCKED: requires live winit window**
- [x] Action handling test
- [x] Keyboard navigation audit

## Performance
- [x] Incremental tree updates: hash-based delta (replaced Debug-string comparison with `content_hash()`)
- [x] Node pooling (`NodePool` with active map + free-list)
- [x] Lazy property computation (`Lazy<T>` with `get_or_compute` / `invalidate`)

## Integration
- [x] `oxiui-core` Widget trait a11y methods — `Widget::a11y_role()` / `a11y_label()` / `a11y_description()` in `oxiui-core`; `widget_bridge.rs` provides `core_role_to_widget_role`, `widget_to_a11y_node`, `build_a11y_tree`, `A11yWidgetNode` (11 tests, 0 warnings).
- [x] `oxiui-text` text input a11y — `src/text_bridge.rs` (feature `text-bridge`): `text_input_to_a11y(&TextInput, &TextInputA11yParams) -> A11yNode` and `text_area_to_a11y(&TextArea, &TextInputA11yParams) -> A11yNode` bridge `oxiui-text` widgets to the a11y tree with cursor/selection descriptions (14 tests, 0 warnings).
- [x] `oxiui-theme` high-contrast / reduced-motion — `OsA11yPrefs::query()` / `query_from()` in `lib.rs`; `oxiui-theme::os_prefers_high_contrast()` / `os_prefers_reduced_motion()` in `oxiui-theme`.
- [x] `oxiui-egui` / `oxiui-iced` bridge — `oxiui-egui::a11y` (feature `a11y`): `oxiui_tree_to_accesskit`, `diff_a11y_trees`, `A11yEguiBridge`; `oxiui-iced::a11y_bridge` (feature `a11y`): `spec_to_a11y_node`, `spec_to_a11y_tree`, `IcedA11yConfig`.
- [x] `oxiui-table` structured table a11y — `build_table_a11y` / `column_header_node` / `table_row_node` / `table_cell_node` in `src/tree.rs`; `oxiui-table::accessibility` module with `build_table_a11y_tree` / `build_table_a11y_full`.
- [x] `oxiui-render-wgpu` / `oxiui-render-soft` focus ring rendering — `FocusRing::ring_rect()` and `FocusRing::is_visible()` helpers added to `src/focus.rs` (4 tests); renderers consume `FocusRing` / `FocusIndicator` via the `accessibility` feature gate in `oxiui-render-wgpu`.
