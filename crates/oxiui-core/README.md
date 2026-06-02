# oxiui-core — Core traits and types for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-core.svg)](https://crates.io/crates/oxiui-core)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-core` defines the public API surface that every OxiUI backend, adapter, and widget crate builds upon. It is the foundation of the COOLJAPAN Pure-Rust GUI toolkit: the immediate-mode trait surface (`UiCtx`, `Widget`, `Theme`), the geometry and event primitives, a retained widget tree, a flexbox/grid layout engine, a Cassowary constraint solver, a draw-command/paint layer, a reactive signal runtime, animation primitives, and the colour-space utilities consumed across the stack.

The crate is `#![forbid(unsafe_code)]` and has **zero mandatory external dependencies** (only an optional `serde` integration). Concrete adapters — `oxiui-egui`, `oxiui-iced`, `oxiui-slint`, `oxiui-dioxus`, `oxiui-render-soft`, `oxiui-render-wgpu` — implement the traits declared here; the `oxiui` facade wires them together. Themes live in `oxiui-theme`, rich text in `oxiui-text`, tables in `oxiui-table`, and the accessibility tree in `oxiui-accessibility`. 100% Pure Rust.

## Installation

```toml
[dependencies]
oxiui-core = "0.1.0"

# Enable serde on UiEvent and its nested event types:
oxiui-core = { version = "0.1.0", features = ["serde"] }
```

## Quick Start

Implement [`UiCtx`] in an adapter, override the three required widget methods, and let every extended widget degrade visibly via the `supported` flag:

```rust
use oxiui_core::{UiCtx, ButtonResponse, TextStyle};
use oxiui_core::response::WidgetResponse;

struct ConsoleUi;

impl UiCtx for ConsoleUi {
    fn heading(&mut self, text: &str) {
        println!("# {text}");
    }
    fn label(&mut self, text: &str) {
        println!("{text}");
    }
    fn button(&mut self, label: &str) -> ButtonResponse {
        println!("[ {label} ]");
        ButtonResponse::default()
    }
}

let mut ui = ConsoleUi;
ui.heading("Welcome");
ui.label("Pure-Rust UI core");
let r = ui.button("OK");
assert!(!r.clicked);

// Extended widgets that the adapter has *not* overridden report
// `supported == false` instead of silently rendering nothing.
let slider = ui.slider(0.5, 0.0..=1.0);
assert!(!slider.supported);
```

### Geometry, layout and paint

```rust
use oxiui_core::{Constraints, Point, Rect, Size};
use oxiui_core::{FlexLayout, FlexItem, JustifyContent};
use oxiui_core::{Color, DrawList};

// Box constraints clamp a size into a [min, max] range.
let c = Constraints::new(Size::new(10.0, 10.0), Size::new(100.0, 100.0));
assert_eq!(c.constrain(Size::new(5.0, 200.0)), Size::new(10.0, 100.0));

// Hit testing on rectangles is inclusive of top/left, exclusive of bottom/right.
let r = Rect::new(10.0, 20.0, 100.0, 50.0);
assert!(r.contains(Point::new(10.0, 20.0)));

// Build a backend-agnostic draw list.
let mut dl = DrawList::new();
dl.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color(122, 162, 247, 255));
assert_eq!(dl.len(), 1);
```

## API Overview

### Core widget traits

| Trait | Role |
|-------|------|
| `UiCtx` | Immediate-mode rendering context. Three required methods (`heading`, `label`, `button`) plus ~25 extended widget/container methods with `supported == false` defaults |
| `Widget` | A renderable element: `render(&mut self, ui: &mut dyn UiCtx)` |
| `Theme` | `Send + Sync` provider of a [`Palette`] and a [`FontSpec`] |
| `Layout` | A layout strategy: `axis()` + `spacing()` |
| `EventSink` | Accepts [`UiEvent`]s for processing via `push` |

#### `UiCtx` extended widget methods

All return a `*Response` whose `supported` field is `false` by default; container methods do **not** invoke their content closure when unsupported, so callers can detect non-support before side effects run.

| Method | Returns |
|--------|---------|
| `text_input(text)` | `TextInputResponse` |
| `checkbox(label, checked)` | `CheckboxResponse` |
| `slider(value, range)` | `SliderResponse` |
| `dropdown(options, selected)` | `DropdownResponse` |
| `image(uri, size)` / `separator()` / `spacer(size)` / `tooltip(text)` | `WidgetResponse` |
| `scroll_area(content)` / `popup(content)` / `modal(title, content)` | `WidgetResponse` |
| `horizontal(content)` / `vertical(content)` / `grid(cols, content)` / `menu_bar(content)` | `WidgetResponse` |
| `rich_text(spans)` | `WidgetResponse` |
| `label_styled(text, style)` / `heading_styled(text, style)` | `WidgetResponse` (defaults delegate to `label`/`heading`, so `supported == true`) |
| `drag_source(id, content)` / `drop_target(accept_ids, content)` | `WidgetResponse` |

### Core types (crate root)

| Type | Description |
|------|-------------|
| `Color(u8, u8, u8, u8)` | RGBA colour, one `u8` per channel |
| `Palette` | Semantic palette: `background`, `surface`, `primary`, `on_primary`, `text`, `muted` |
| `FontSpec` | Font request: `family`, `size`, `weight`, `style`, `letter_spacing`, `line_height`, `features`; builder methods + `is_slanted()` |
| `FontStyle` | `Normal`, `Italic`, `Oblique { degrees }` |
| `FontFeature` | OpenType feature toggle (`tag`, `value`); `on`/`off`/`value` constructors |
| `RichTextSpan` | A styled span for `rich_text` (bold/italic/color/font_size/font_family builder) |
| `ButtonResponse` | `clicked`, `hovered` |
| `Axis` | `Vertical`, `Horizontal` |
| `UiEvent` | `#[non_exhaustive]` backend event enum (resize, mouse, key, wheel, IME, …) |
| `UiError` | `#[non_exhaustive]` error enum (see below) |

### `response` module

Response structs returned by the extended `UiCtx` widgets. Each carries a `supported` flag, a zeroed/identity payload, and `unsupported()` / `supported(…)` constructors.

| Type | Payload fields |
|------|----------------|
| `TextInputResponse` | `changed`, `text`, `supported`, `focused` (+ `supported_focused`) |
| `CheckboxResponse` | `changed`, `checked`, `supported` |
| `SliderResponse` | `changed`, `value`, `supported` |
| `DropdownResponse` | `changed`, `selected`, `supported` |
| `WidgetResponse` | `supported` only |

### `geometry` module

| Type | Highlights |
|------|-----------|
| `Point` | `ZERO`, `new`, `distance`; `Add`/`Sub` |
| `Size` | `ZERO`, `new`, `area`, `is_empty`, `clamp`; `Mul<f32>` |
| `Insets` | `ZERO`, `new`, `all`, `symmetric`, `horizontal`, `vertical` |
| `Rect` | `new`, `from_origin_size`, edges, `center`, `contains`, `intersects`, `intersection`, `union`, `deflate`, `inflate` |
| `Constraints` | `new`, `tight`, `loose`, `unbounded`, `constrain`, `is_tight` |

All coordinates are `f32` logical pixels with the origin top-left.

### `style` module

| Type | Description |
|------|-------------|
| `Padding` | Newtype over `Insets` (inner spacing); `shrink(rect)` |
| `Margin` | Newtype over `Insets` (outer spacing); `grow(rect)` |
| `Border` | Per-side widths + `Color` + `BorderStyle`; `solid`, `content_rect`, `is_none` |
| `BorderStyle` | `Solid`, `Dashed`, `Dotted`, `Double`, `None` |
| `CursorShape` | `#[non_exhaustive]` OS cursor set (`Default`, `Pointer`, `Text`, `ResizeEw`, `Grab`, …) |

### `text_style` module

`TextStyle` — builder-pattern typography intent (size, weight, italic, colour, line-height, letter-spacing, underline, strikethrough). Presets: `bold()`, `italic()`, `heading()`, `body()`, `caption()`; builders `with_size`, `with_weight`, `with_color`, `with_line_height`, `with_letter_spacing`, `with_underline`, `with_strikethrough`.

### `events` module

| Type | Description |
|------|-------------|
| `MouseButton` | `Left`, `Right`, `Middle`, `Other(u16)` |
| `Modifiers` | `ctrl`/`alt`/`shift`/`meta`; `NONE`, `is_empty`, `command()` |
| `Key` | `#[non_exhaustive]` logical key (`Character`, `Enter`, arrows, `Function(u8)`, `Named`, …); `as_text()` |
| `PhysicalKey` | Layout-independent HID code (`"KeyA"`); `new`, `code` |
| `ScrollDelta` | `Lines { x, y }` or `Pixels { x, y }` |
| `MouseEvent` | Pointer events incl. `DoubleClick`, `TripleClick`, `Scroll`, `DragStart/Move/End`; `position()` |
| `KeyboardEvent` | `Down`/`Up`/`CharInput`; `is_repeat()` |
| `TouchEvent` | `Start`/`Move`/`End`/`Cancel`/`Gesture`; `touch_id()` |
| `GestureKind` | `Pinch { scale }`, `Rotate { radians }`, `Swipe { delta }` |
| `Propagation` | `stop_propagation`/`prevent_default`; `CONTINUE`, `stop()`, `prevent()`, `merge()` |

### `paint` module

The canonical paint layer: build a `DrawList`, hand it to a `RenderBackend`.

| Item | Description |
|------|-------------|
| `DrawCommand` | `#[non_exhaustive]` draw op: rects (incl. rounded/per-corner), circles, ellipses, lines (aa/thick/dashed), paths (fill/stroke), gradients (linear/radial), images, 9-slice, box shadow, text, clip push/pop |
| `DrawList` | Ordered command buffer with clip-depth + bounds tracking; `push`, `len`, `iter`, `clear`, `bounds`, `clip_depth`, `is_clip_balanced`, and `push_*` helpers for every command |
| `RenderBackend` | Trait: `execute(&DrawList)`, `surface_size()`, plus `supports_blur/gradients/paths/images/text` capability probes |
| `PathData` / `PathVerb` | Resolution-independent path builder (`move_to`, `line_to`, `quad_to`, `cubic_to`, `close`, `bounds`) |
| `StrokeStyle` | `width`, `join`, `cap`, `miter_limit` |
| `LineJoin` / `LineCap` / `FillRule` | `Miter`/`Bevel`/`Round`; `Butt`/`Round`/`Square`; `EvenOdd`/`NonZero` |
| `GradientStop` | `offset` (clamped to `[0,1]`) + `color` |
| `ImageData` / `ImageFilter` | Owned RGBA pixels; `Nearest`/`Bilinear` sampling |

### `tree` module — retained widget tree

| Item | Description |
|------|-------------|
| `WidgetId` | Stable `u64` node id; `ROOT` reserved |
| `WidgetIdAllocator` | Monotonic id allocator; `alloc`, `allocated` |
| `WidgetNode` | Node with `parent`, `children`, `rect`, `z_index`, `hit_testable`, `focusable`, `dirty`, `clip_rect`, `label` |
| `WidgetTree` | Flat tree: `insert`, `remove`, `reparent` (cycle-rejecting), `depth`, `walk_dfs`, `effective_clip`, `hit_test`, `focus_order`, `paint_order` |

### `layout` module — flexbox engine

`FlexLayout` (single- and multi-line/wrapping) with `FlexItem` (`basis` + `grow`). Enums: `FlexDirection` (`Row`/`Column`), `JustifyContent`, `AlignItems`, `FlexWrap` (`NoWrap`/`Wrap`/`WrapReverse`), `AlignContent`. Constructors `row()`/`column()` plus `with_justify`/`with_align` builders.

### `grid` module — CSS grid

`compute_grid(template, items, available) -> Vec<Rect>`. Types: `GridTemplate`, `GridItem`, `GridPlacement` (`auto`/`at`/`span`), `GridLine`, `GridSpan`, `TrackSizing`.

### `solver` module — Cassowary constraint solver

A linear-arithmetic constraint solver for advanced layout. `Solver` (`add_constraint`, `remove_constraint`, `add_edit_variable`, `suggest_value`, `update_variables`, `value_of`, `reset`), plus `Variable`, `Term`, `Expression`, `Constraint`, `RelOp`, `Strength`, and `SolverError`.

### `reactive` module — signal runtime

`ReactiveRuntime` creates `Signal<T>` (get/set) and `Computed<T>` (derived, `get() -> Result<T, ReactiveError>`) values for fine-grained reactivity. `ReactiveError` reports cycles / poisoned state.

### `anim` module — animation primitives

| Item | Description |
|------|-------------|
| `Easing` | Easing curves; `eval(t)` |
| `Transition` | Duration + easing + delay; `progress`, `sample`, `is_finished` |
| `Spring` | Physically-based spring; `from_frequency`, `position`, `is_settled` |
| `Animator` | Keyed animation manager; `start`, `value`, `advance(dt)`, `cancel` |

### `scheduler` module — timers

`Scheduler` (`after`, `every`, `request_frame`, `cancel`, `tick(dt)`) with `TimerId`, plus `Debounce` and `Throttle` rate-limiters.

### `dispatch` module — event capture/bubble

`EventDispatcher` runs a capture/bubble pipeline over a `WidgetTree`. Types: `DispatchEvent`, `Phase`, `EventHandler` (trait), `HandlerCtx`.

### `focus` module

`FocusManager` — focus traversal over a `WidgetTree`: `focus`, `blur`, `focus_next`, `focus_prev`, `push_trap`/`pop_trap` (focus traps), `autofocus`, `reconcile`.

### `cache` module

`LayoutCache` — memoises computed rectangles keyed by `(WidgetId, available size, content hash)`; `get`, `put`, `invalidate`, `hit_rate`, `is_dirty`.

### `diff` module — tree reconciliation

`diff(old, new) -> Vec<DiffOp>` computes a minimal `Insert`/`Remove`/`Update`/`Move` op set (LCS-based) to reconcile two `WidgetTree`s.

### `color_space` module

| Item | Description |
|------|-------------|
| `LinearRgba`, `Hsla`, `Oklcha` | Colour-space conversions to/from `Color`; `lerp`, `scale_lightness`, `with_lightness` |
| `contrast_ratio(a, b)` | WCAG contrast ratio |
| `WcagLevel` | `from_ratio`; AA / AAA classification |
| `ContrastWarning` | Reported by `PaletteBuilder::validate` |
| `PaletteBuilder` | Fluent `Palette` builder with contrast validation |

### `widget_ext` module — combinators

`WidgetExt` blanket-implemented for every `Widget`, adding chainable wrappers: `padding`, `margin`, `background`, `border`, `on_click`, `on_hover` (yielding `Padded`, `Margined`, `Backgrounded`, `Bordered`, `OnClick`, `OnHover`). Also: `ClipboardProvider` trait, `DragSource`/`DropTarget` traits, `DragData`, `DropEffect`.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `serde` | off | Derive `serde::{Serialize, Deserialize}` on `UiEvent` and its nested event types (`MouseButton`, `Modifiers`, `Key`, `ScrollDelta`) |

## Error variants — `UiError`

`#[non_exhaustive]`; downstream `match` must include a catch-all.

| Variant | Description |
|---------|-------------|
| `Backend(String)` | Windowing / GPU initialisation error |
| `Render(String)` | Render-pipeline error |
| `Window(String)` | Window-management error |
| `Unsupported(String)` | Requested feature or backend unavailable |
| `Layout(String)` | Layout-engine error (e.g. unsatisfiable constraints) |
| `Focus(String)` | Focus-management error |
| `Clipboard(String)` | Clipboard access error |
| `DragDrop(String)` | Drag-and-drop protocol error |
| `Other(String)` | Any other error |

## Related crates

`oxiui-core` is the foundation of the OxiUI workspace:

- [`oxiui`](https://crates.io/crates/oxiui) — the top-level facade that wires adapters together
- [`oxiui-theme`](https://crates.io/crates/oxiui-theme) — COOLJAPAN themes (Tokyo Night) built on `Palette`/`FontSpec`/`Theme`
- [`oxiui-text`](https://crates.io/crates/oxiui-text) — rich text layer over `oxitext`, consuming `TextStyle` and `RichTextSpan`
- [`oxiui-table`](https://crates.io/crates/oxiui-table) — virtualized table widget
- [`oxiui-accessibility`](https://crates.io/crates/oxiui-accessibility) — AccessKit a11y tree built from the widget graph
- `oxiui-render-soft`, `oxiui-render-wgpu` — `RenderBackend` implementations
- `oxiui-egui`, `oxiui-iced`, `oxiui-slint`, `oxiui-dioxus`, `oxiui-web` — `UiCtx` adapters

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
