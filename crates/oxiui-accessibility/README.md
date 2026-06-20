# oxiui-accessibility — AccessKit a11y tree builder for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-accessibility.svg)](https://crates.io/crates/oxiui-accessibility)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-accessibility` is the accessibility bridge of the COOLJAPAN OxiUI toolkit. It converts an OxiUI widget graph — expressed as a tree of `A11yNode`s — into an [`accesskit::TreeUpdate`] that can be pushed to any AccessKit platform adapter (Windows UIA, macOS, AT-SPI, etc.). On top of the basic tree build it provides incremental tree diffing, focus tracking, a multi-window tree registry, tab-order navigation, action mapping/dispatch, text-run synthesis for screen readers, table-structure helpers, a node-recycling pool, and lazy/dirty-tracked recomputation.

The crate is intentionally **headless**: no windowing toolkit or platform adapter is imported, so the entire tree-building logic is exercisable in plain unit tests without a display server. It is `#![forbid(unsafe_code)]` and 100% Pure Rust; its dependencies are `oxiui-core` and `accesskit`.

## Installation

```toml
[dependencies]
oxiui-accessibility = "0.1.3"
```

## Quick Start

```rust
use accesskit::NodeId;
use oxiui_accessibility::tree::{A11yNode, A11yTree, WidgetRole};

let root = A11yNode::simple(NodeId(1), WidgetRole::Window, Some("My App".to_string()));
let update = A11yTree::build(&root);
assert_eq!(update.nodes.len(), 1);
```

### Building rich nodes

```rust
use accesskit::NodeId;
use oxiui_accessibility::{A11yNodeBuilder, tree::{A11yTree, WidgetRole}, props::CheckedState};

let node = A11yNodeBuilder::new(NodeId(2), WidgetRole::Checkbox)
    .label("Enable notifications")
    .description("Toggles desktop notifications")
    .checked(CheckedState::True)
    .tab_index(1)
    .build();

let update = A11yTree::build(&node);
assert_eq!(update.nodes.len(), 1);
```

### Incremental updates

```rust
use accesskit::NodeId;
use oxiui_accessibility::tree::{A11yNode, A11yTree, WidgetRole};

let mut old = A11yTree::default();
old.build_and_store(&A11yNode::simple(NodeId(1), WidgetRole::Button, Some("Old".into())));

let mut new = A11yTree::default();
new.build_and_store(&A11yNode::simple(NodeId(1), WidgetRole::Button, Some("New".into())));

// Only the changed node appears in the delta.
let delta = A11yTree::diff(&old, &new);
assert_eq!(delta.nodes.len(), 1);
```

## API Overview

### `tree` module — node tree & build

| Item | Description |
|------|-------------|
| `A11yNode` | A widget node: `id`, `role`, `label`, `children`, `props`, `text_content`. `simple(id, role, label)`, `content_hash` |
| `A11yTree` | The tree + a stored snapshot. `build(root) -> TreeUpdate`, `build_and_store`, `set_focus`/`focus`, `focus_update`, `announce(text, urgency)`, `diff(old, new) -> TreeUpdate` |
| `WidgetRole` | 30 roles mapping to `accesskit::Role` (see below). Implements `Display` and `From<WidgetRole> for Role` |
| `build_table_a11y(rows, cols, headers)` | Synthesize a full table subtree (column headers + rows + cells) |
| `table_row_node` / `table_cell_node` / `column_header_node` | Build individual table a11y nodes (carry row/col descriptions) |
| `synthesize_text_run_children(text, selection)` | Split text into selected/unselected `TextRunChild` segments for screen readers |

`WidgetRole` variants: `Window`, `Group`, `Button`, `Label`, `TextInput`, `TableRow`, `TableCell`, `ScrollView`, `Image`, `Unknown`, `Checkbox`, `Slider`, `ProgressBar`, `Tab`, `TabPanel`, `Menu`, `MenuItem`, `Dialog`, `Alert`, `Tooltip`, `Tree`, `TreeItem`, `ListItem`, `Link`, `ColumnHeader`, `Banner`, `Navigation`, `Main`, `Complementary`, `ContentInfo`.

### `builder` module — fluent node builder

`A11yNodeBuilder::new(id, role)` then chain: `label`, `description`, `placeholder`, `key_shortcut`, `disabled`, `expanded(bool)`, `selected(bool)`, `checked(CheckedState)`, `value(now, min, max, step)`, `text`, `text_selection`, relationships (`labelled_by`, `described_by`, `controlled_by`, `owns`), `tab_index`, `child`; finish with `build()` or `build_with_children(children)`.

### `props` module — node properties

| Item | Description |
|------|-------------|
| `A11yNodeProps` | The rich property bag: description, placeholder, key shortcut, disabled, expanded, selected, checked, value range, tab index, relationship id lists, … |
| `CheckedState` (= `Toggled3`) | `False`, `True`, `Mixed` (tri-state) |
| `LiveSetting` | Live-region politeness (`Polite`, `Assertive`, …) → `accesskit::Live` |
| `TextCaret` | Caret/selection over text; `lo`, `hi`, `is_caret` |
| `TextSelection` | `anchor`/`focus` selection; `is_caret` |
| `TextRunChild` | One synthesized text-run segment (text, char/byte offset, `is_selected`) |
| `character_lengths_utf8(text)` | Per-character UTF-8 byte lengths (AccessKit text-run encoding) |
| `byte_offset_to_char_index(text, byte)` | Byte → character-index conversion |

### `action` module — action mapping

`A11yAction` — `Click`, `Focus`, `ScrollIntoView`, `SetValue(String)`, `Increment`, `Decrement`, `Custom(String)`. `map_action(&ActionRequest) -> Option<A11yAction>` maps AccessKit requests; `ActionDispatcher` (`on_action`, `dispatch`) fans them out to registered handlers.

### `focus` module

`FocusIndicator` — tracks the focused node and its focus ring: `set_focus`, `focused_node`, `ring`, `with_ring`. `FocusRing` describes the visual ring.

### `nav` module — tab order

`TabOrder::compute(root)` builds the tab sequence; `next_focus`/`prev_focus` walk it. Free functions `tab_next(order, current)` and `tab_prev(order, current)`.

### `dirty` module — incremental recomputation

| Item | Description |
|------|-------------|
| `Lazy<T>` | Dirty-tracked memoized value: `get_or_compute`, `invalidate`, `set`, `get_if_clean`, `into_inner` |
| `DirtyTracker` | Per-window dirty flags + a monotonic generation counter: `mark_dirty`, `is_dirty`, `clear`, `generation` |

### `pool` module — node recycling

`NodePool` — recycles `A11yNode` allocations across frames: `alloc`, `alloc_recycled`, `get`/`get_mut`, `recycle`, `active_count`, `free_count`, `iter_active`.

### `text_a11y` module — text-input a11y

`TextSelection` (text-position selection; `cursor`, `range`, `start`, `end`, `is_collapsed`), `TextPosition`, `build_text_input_a11y(...)`, and `update_text_cursor(node, selection)`.

### Crate-root items

| Item | Description |
|------|-------------|
| `OsA11yPrefs` | OS accessibility preferences (`high_contrast`, `reduced_motion`); `query()` reads `OXIUI_HIGH_CONTRAST` / `OXIUI_REDUCED_MOTION`, `query_from(lookup)` for testable injection |
| `WindowA11yId(u64)` | A per-window tree identifier (distinct from `accesskit::NodeId`) |
| `A11yForest` | Registry of one `A11yTree` per window: `insert`/`register`, `get`/`get_mut`, `remove`/`unregister`, `iter`, `windows` |

## Re-exports

These are re-exported at the crate root for convenience:

- From `tree`: `A11yNode`, `A11yTree`, `WidgetRole`, `build_table_a11y`, `table_row_node`, `table_cell_node`, `column_header_node`, `synthesize_text_run_children`
- From `builder`: `A11yNodeBuilder`
- From `props`: `A11yNodeProps`, `CheckedState`, `LiveSetting`, `TextCaret`, `TextSelection`, `TextRunChild`, `Toggled3`, `character_lengths_utf8`, `byte_offset_to_char_index`
- From `action`: `A11yAction`, `ActionDispatcher`, `map_action`
- From `focus`: `FocusIndicator`, `FocusRing`
- From `nav`: `TabOrder`, `tab_next`, `tab_prev`
- From `dirty`: `DirtyTracker`, `Lazy`
- From `pool`: `NodePool`

## AccessKit version

Built against `accesskit` 0.24. `WidgetRole` maps onto `accesskit::Role`, `CheckedState`/`LiveSetting` onto `accesskit::Toggled`/`Live`. Only the platform-independent `accesskit` core crate is required — no `accesskit_winit`, so this crate stays headless.

## Related crates

- [`oxiui-core`](https://crates.io/crates/oxiui-core) — supplies the widget graph this crate mirrors
- [`oxiui`](https://crates.io/crates/oxiui) — the OxiUI facade
- [`oxiui-table`](https://crates.io/crates/oxiui-table) — the table widget whose structure `build_table_a11y` mirrors
- [`oxiui-theme`](https://crates.io/crates/oxiui-theme) — high-contrast palettes paired with `OsA11yPrefs`

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
