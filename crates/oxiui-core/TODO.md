# oxiui-core TODO

## Active /ultra plan (2026-05-28)

**Goal:** `oxiui-core` gains a real event-dispatch system, focus manager,
animation/easing + scheduler, widget-tree diffing, layout cache, and a fuller
type / `UiCtx` surface — all ADDITIVE, so the egui/iced/slint/dioxus adapters
and the `oxiui` facade still compile UNCHANGED. New `UiCtx` widget methods all
ship default impls returning a `*Response` with `supported == false`.

**New modules (files added):**
`src/style.rs`, `src/color_space.rs`, `src/dispatch.rs`, `src/focus.rs`,
`src/anim.rs`, `src/scheduler.rs`, `src/diff.rs`, `src/cache.rs`,
`src/response.rs` (extra: holds the `*Response` structs to keep `lib.rs` slim),
`src/widget_ext.rs`. Additive edits to `events.rs` (rich Mouse/Keyboard/Touch +
`Propagation`), `tree.rs` (`clip_rect` field + clip-aware `hit_test` +
`paint_order`), and `lib.rs` (`UiError` variants + `#[non_exhaustive]`, extended
`UiCtx` default methods, re-exports). `geometry.rs` `Constraints` were already
present (verified). All files < 2000 lines (largest is `color_space.rs`, 559).

**Result:** 108 tests pass (34 pre-existing + 74 new); `clippy -D warnings`
clean; no new dependencies. Workspace-wide build is blocked only by a
pre-existing, unrelated `oxiui-text` → external `oxitext` API breakage (proven
via stash-test); the three other `UiCtx` adapters build unchanged.

## Status
Seed-stage core traits crate (~195 SLOC). Defines `UiCtx`, `Widget`, `Theme`, `Layout`, `EventSink` traits, `Color`/`Palette`/`FontSpec` types, `UiEvent` enum (including IME), `ButtonResponse`, and `UiError`. No widget tree, no layout engine, no event dispatch, no focus management, no hit testing, no reactive state -- all of which are required for a production UI framework.

## Core Implementation
- [x] Widget tree data structure: `WidgetNode` with stable `WidgetId`, parent/child links, dirty flags, depth tracking, insertion/removal/reparenting operations (`tree.rs`) — verified: 7 unit tests pass.
- [x] Widget ID allocator: monotonic arena-style ID generation, guaranteed uniqueness within a tree instance (`tree.rs`) — verified: `id_allocator_is_monotonic_and_unique` passes. (No recycling by design — monotonic ids guarantee uniqueness.)
- [x] Flexbox layout engine (single-line): main/cross-axis sizing, `flex-grow`, all `justify-content` modes, `align-items` (`layout.rs`) — verified: 8 layout tests pass. NOTE: `flex-shrink`/`flex-basis` shorthand and **wrapping are deferred** (see Proposed follow-ups).
- [x] **Grid layout engine: tracks, fr units, minmax(), auto-sizing, template-areas, gap, spanning, auto-placement** (completed 2026-05-29)
  - **Goal:** a real CSS-Grid layout engine in core, additive alongside existing flexbox/layout code.
  - **Design:** new `src/grid.rs` (~600-800 SLOC). `TrackSizing{Fixed(f32),Fr(f32),Auto,MinMax(Box,Box),MinContent,MaxContent}`; `GridTemplate{rows,cols,areas,row_gap,col_gap}`; `GridItem{placement:GridPlacement}`; `GridPlacement{row:Line|Span,col:Line|Span}`. Track-sizing algorithm in full: (1) resolve intrinsic min/max-content per track from item sizes; (2) distribute free space across Fr tracks proportionally; (3) clamp via MinMax; (4) Auto→content-sized. Auto-placement: sparse row-major cursor honoring explicit placements and span, growing implicit tracks. Template-areas: parse area grid into named rectangles, map items by name. `compute_grid(&GridTemplate,&[GridItem],Size)->Vec<Rect>` using core Point/Rect/Size.
  - **Files:** new `src/grid.rs`; `lib.rs` adds `pub mod grid;` + re-exports. Also close the two paint.rs `[~]` items (flip to `[x]` — paint.rs 965 lines already contains everything they describe).
  - **Prerequisites:** none.
  - **Tests (~12+6 conformance):** fixed-track row/col; 3 equal Fr split; minmax(100,1fr) clamps; auto→content; explicit placement at line N; span 2; auto-placement with hole; template-areas; row/col gap; over-constrained shrinks; empty→empty; ≥6 CSS Grid spec scenarios.
  - **Risk:** track-sizing + auto-placement is genuinely hard — implement in full, no stubs. Lift steps from CSS Grid spec. Defer reactive Signal/Computed and cassowary (each own round-4 slice, unrelated algorithms).
- [x] Constraint-based layout solver: cassowary-style linear constraint resolution for advanced layouts (anchoring, proportional sizing, min/max constraints) -- Pure Rust, no OxiZ for this (~400 SLOC) — deferred (see follow-ups)
  - **Goal:** Pure-Rust Cassowary linear constraint solver (`solver.rs`) with `Solver`, `Variable`, `Constraint`, `Expression`, `Strength`; dual-phase simplex restricted tableau (planned 2026-05-29)
  - **Design:** `Variable(u64)`, `Term{variable,coefficient:f64}`, `Expression{terms,constant}`, `Constraint{expression,op:RelOp,strength:f64}`, `Solver{rows:HashMap<Symbol,Row>,vars,constraints,edits}`; API: `add_constraint/remove_constraint/add_edit_variable/suggest_value/update_variables/value_of/reset`; internal: restricted tableau + dual optimize; Bland's rule for anti-cycling; `SolverError{DuplicateConstraint,UnsatisfiableConstraint,UnknownConstraint,UnknownEditVariable}`
  - **Files:** new `crates/oxiui-core/src/solver.rs`; extend `src/lib.rs` (re-export + proptest dev-dep); `crates/oxiui-core/Cargo.toml` (proptest workspace dep)
  - **Tests:** ~8 unit (two-constraint solve, suggest+fetch, over-constrained, proportional a=2b, anchoring, remove+re-solve, strength ordering, reset) + proptest property (random constraints never panic)
  - **Risk:** dual-phase simplex is the hardest part — implement to algorithm depth; test over-constrained/degenerate edges
- [x] Layout cache: memoize computed layout results, invalidate on size/content change, dirty-flag invalidation, hit-rate stats (`cache.rs`) — keyed by `(WidgetId, Size bits, content hash)`; 6 tests.
- [x] `Rect`, `Point`, `Size`, `Insets`, `Constraints` geometry types with f32 precision, builders, arithmetic ops (`geometry.rs`) — verified: `Constraints` already present + tested (`constraints_constrain`).
- [x] Event dispatch system: capture/bubble phases, `StopPropagation`, `PreventDefault`, typed event handlers via trait objects, handler-safe add/remove during dispatch (collect-then-apply) (`dispatch.rs`) — 6 tests.
- [x] Mouse event types: Down/Up/Move/Enter/Leave/DoubleClick/TripleClick, scroll (discrete + smooth), DragStart/Move/End (`events.rs::MouseEvent`).
- [x] Keyboard event types: Down/Up with logical `Key` + `Modifiers` (Ctrl/Alt/Shift/Meta), `CharInput`, repeat flag, physical-vs-logical key (`PhysicalKey`) distinction (`events.rs::KeyboardEvent`).
- [x] Touch event types: Start/Move/End/Cancel, multi-touch with touch-id tracking, gesture recognition (`GestureKind` Pinch/Rotate/Swipe) (`events.rs::TouchEvent`).
- [x] Focus management: tab/shift-tab order traversal, `focus()`/`blur()`, focus-trap for modals, `autofocus`, programmatic move, post-mutation `reconcile` (`focus.rs`) — 6 tests.
- [x] Z-ordering / paint order: `z_index` per node, stable sort for sibling overlap, back-to-front `paint_order()` keyed by `(depth, z_index, source-order)` (`tree.rs`).
- [x] Hit testing: point-in-rect respecting accumulated clip regions, `(depth, z_index)` front-to-back precedence, pass-through (`hit_testable=false`) regions (`tree.rs::hit_test` + `effective_clip`).
- [x] Widget tree diffing: compare old/new trees, minimal Insert/Remove/Update/Move op set keyed by stable `WidgetId`, LCS on children (`diff.rs`) — 6 tests incl. minimal-move reorder.
- [x] Reactive state management: `Signal<T>` / `Computed<T>` primitives, dependency tracking, batched notifications, automatic re-render on state change (~300 SLOC) — deferred (needs design; see follow-ups)
  - **Goal:** oxiui-core exposes fine-grained Send+Sync reactivity — `Signal<T>` (settable cell) and `Computed<T>` (derived value) with automatic dependency tracking, topological recompute, and cycle detection.
  - **Design (ultrathink — no shortcuts):** A `ReactiveRuntime` (shared `Arc`) holds a generational arena of nodes (signal cells + computed thunks), a dependency graph (node → dependents Vec), and a dirty set. `Signal<T>` / `Computed<T>` are handles `{runtime: Arc<ReactiveRuntime>, id: NodeId}`. A read inside a computation registers a dependency edge via a thread-local "current computation" stack. `signal.set(v)` marks transitive dependents dirty and recomputes them in topological order (BFS/Kahn's on the dependency subgraph). Cycle detection via DFS colour-marking on edge insert → `ReactiveError::Cycle` (never panic/deadlock). Send+Sync: runtime internals behind `RwLock`; typed node storage via `Box<dyn Any + Send + Sync>`. **CRITICAL deadlock avoidance:** computed thunks must be run with the RwLock RELEASED — clone any inputs needed, release the lock, run the thunk, re-acquire to store the result. Never hold the lock across a user closure.
  - **Files:** new `crates/oxiui-core/src/reactive.rs`; `crates/oxiui-core/src/lib.rs` (add `pub mod reactive;` + pub re-exports).
  - **Tests:** signal get/set; computed derives + updates on set; chain a→b→c propagates; cycle detection returns `ReactiveError::Cycle` not deadlock/panic; diamond (a→b, a→c, b+c→d) recomputes d once; `fn _assert_send_sync<T: Send+Sync>()` compile-time test; no-deadlock: a read inside a computed that triggers another computed.
  - **Risk:** Deadlock if lock held across recompute (mitigated by compute-then-store design); Send+Sync with type erasure; cycle detection thoroughness.
- [x] Animation / easing system: `Transition` (duration/delay/easing), `Easing` (Linear/EaseIn/Out/InOut/CubicBezier with Newton-Raphson + bisection fallback), `Spring` closed-form damped solver (under/critical/over), `Animator` (`anim.rs`) — 10 tests.
- [x] Timer / scheduler: virtual-clock frame-aligned callback scheduling, `request_frame` (rAF analogue), `after`/`every`, debounce/throttle helpers (`scheduler.rs`) — 6 tests.
- [x] Clipboard abstraction: `ClipboardProvider` trait with `get_text()`/`set_text()` + default MIME methods (`widget_ext.rs`).
- [x] Drag-and-drop protocol: `DragSource`/`DropTarget` traits, `DragData` payload, `DropEffect` (None/Copy/Move/Link) (`widget_ext.rs`). NOTE: visual drag preview is a render-backend concern, not core.
- [x] Cursor management: `CursorShape` enum (pointer/text/resize{Ew,Ns,Nesw,Nwse}/grab/grabbing/crosshair/wait/progress/move/notallowed/none) (`style.rs`). Custom cursor images are a backend concern.
- [ ] Multi-window support: `WindowId` handle, window creation/destruction events, cross-window communication, per-window widget trees (~200 SLOC) — deferred (architecture; see follow-ups)

## API Improvements
- [x] Color-space conversions: `LinearRgba`/`Hsla`/`Oklcha` with round-trip-correct sRGB↔linear↔HSL↔Oklch math + `LinearRgba::lerp`/`relative_luminance` (`color_space.rs`). NOTE: base `Color` stays 8-bit sRGB (additive — not made generic, to avoid breaking adapters); the spaces are conversion views.
- [x] `Palette` builder with validation: `PaletteBuilder` derives unset roles in Oklch + `validate()` returns WCAG contrast `ContrastWarning`s (never hard errors), `contrast_ratio`, `WcagLevel` (`color_space.rs`).
- [x] Expand `FontSpec` with italic/oblique, letter-spacing, line-height, OpenType `features` (`lib.rs` — `FontStyle`, additive fields; legacy `FontSpec::new` retained).
- [x] Add `Padding`, `Margin`, `Border` types with per-side values and shorthand constructors (`style.rs`) — `Padding`/`Margin` are newtypes over `Insets`; `Border { insets, color, style }`.
- [x] Make `UiCtx` richer: `text_input()`, `checkbox()`, `slider()`, `dropdown()`, `image()`, `separator()`, `spacer()`, `scroll_area()`, `tooltip()`, `popup()`, `modal()` — all DEFAULT methods returning `*Response { supported: false }` so existing adapters compile unchanged (`lib.rs` + `response.rs`).
- [x] UiCtx trait extension: 7 new dyn-compatible default layout/container methods (horizontal, vertical, grid, menu_bar, rich_text, drag_source, drop_target) + `RichTextSpan` type (planned 2026-05-29)
  - **Goal:** add 7 new default methods to `UiCtx` (all return `WidgetResponse::unsupported()`) and a `RichTextSpan{text,bold,italic,color:[u8;4],font_size,font_family}` type; all adapters inherit the defaults — no changes to existing impls required
  - **Design:** methods use `&mut dyn FnMut(&mut dyn UiCtx)` closure signature (same as existing `scroll_area`/`popup`/`modal`) — dyn-compatible; `RichTextSpan` mirrors oxiui-text's `TextStyle` shape but is minted fresh in core (no cross-crate import); `pub use solver::*` re-export; 7 methods: `horizontal/vertical/grid(cols,f)/menu_bar/rich_text(spans)/drag_source(id,f)/drop_target(accept,f)`
  - **Files:** `crates/oxiui-core/src/lib.rs` (7 new UiCtx default methods + RichTextSpan struct + re-exports)
  - **Tests:** ~5 (verify each new method returns WidgetResponse::unsupported(), RichTextSpan construction)
  - **Risk:** dyn-compatibility must be preserved — no generics, no Self returns on new methods; verify BareCtx/StubCtx/NullUiCtx still compile without changes
- [x] Add `WidgetExt` trait with combinators `.padding()`, `.margin()`, `.background()`, `.border()`, `.on_click()`, `.on_hover()` (`widget_ext.rs`) — blanket-impl'd for every `Widget`, returns composing wrappers.
- [x] Error enum: added `Layout`, `Focus`, `Clipboard`, `DragDrop` variants to `UiError` (`lib.rs`).
- [x] Derive `Hash` for `Color` — verified already present (`#[derive(... Hash)]` on `Color`).
- [x] `#[non_exhaustive]` on `UiError` for forward compatibility (`lib.rs`) — verified no downstream `match` breaks (all uses are constructors).

## Testing
- [x] Widget tree construction/traversal unit tests: 3-level tree, parent/child relationships, DFS order, paint order, effective clip (`tree::tests` — 7 tests).
- [x] Event dispatch tests: capture→bubble ordering, stop-propagation halts, prevent-default reported, **handler add/remove during dispatch is deferred** (collect-then-apply) (`dispatch::tests` — 6 tests).
- [x] Focus management tests: tab/shift-tab cycle with wrap-around, focus-trap containment, programmatic focus, autofocus, reconcile-after-removal (`focus::tests` — 6 tests).
- [x] Hit-test tests: front-most-wins, pass-through (`hit_testable=false`), clip-rect respect, ancestor-clip intersection (`tree::tests` — 4 tests).
- [x] Animation tests: easing endpoints/midpoint, ease-in-out symmetry, ease-in lag, cubic-bezier identity-by-collinear-controls, **degenerate cubic-bezier does not NaN (Newton+bisection)**, spring critically/under/over-damped, animator restart/cancel/drop-on-finish (`anim::tests` — 10 tests).
- [x] Color-space round-trip tests: sRGB→linear→sRGB, sRGB→HSL→sRGB, sRGB→Oklch→sRGB within ±2/255 across primaries+grays; WCAG ratio black/white = 21.0 (`color_space::tests` — 8 tests).
- [x] Diff tests: LCS basic, identical trees → no ops, field-only Update, Insert+Remove, subtree-root-only Remove, minimal-move reorder (`diff::tests` — 6 tests).
- [x] Layout cache tests: hit/miss bookkeeping, dirty-flag forces miss, invalidate drops stale entries, hit-rate accounting (`cache::tests` — 6 tests).
- [x] Scheduler tests: `after` fires once when due, `every` handles large `dt` without drift, `request_frame` runs once, cancel, debounce trailing-edge, throttle leading-edge (`scheduler::tests` — 6 tests).
- [x] Widget-ext tests: decorators forward render & expose style, `.on_click` fires on `clicked`, `.on_hover` reports state, `DragData::text` round-trip, default-MIME clipboard returns unsupported (`widget_ext::tests` — 7 tests).
- [x] Response/UiCtx defaults tests: `*Response::unsupported()` constructors yield `supported = false` & zeroed payloads; extended `UiCtx` defaults all return `supported = false` and container defaults do **not** invoke their content closure (`response::tests` + `tests::extended_uictx_defaults_report_unsupported` — 4 + 1 tests).
- [x] Flexbox layout conformance tests: port a subset of the CSS Flexbox spec test suite (at least 20 layout scenarios) (~300 SLOC) — deferred (blocked on wrapping support; see follow-ups)
  - **Goal:** 20 CSS flexbox spec conformance scenarios unlocked by the new FlexWrap support added alongside this item (planned 2026-05-29)
  - **Design:** `FlexWrap{NoWrap,Wrap,WrapReverse}` + `AlignContent{Start,Center,End,SpaceBetween,SpaceAround,SpaceEvenly,Stretch}` added to `FlexLayout`; wrap algorithm partitions items into lines where basis-sum ≤ container main-size, then distributes lines per `align_content`; 20-scenario conformance suite covers min-max, wrap-reverse, align-content variants, zero-basis, over-filled, row vs column
  - **Files:** extend `crates/oxiui-core/src/layout.rs` (FlexWrap/AlignContent types + wrap algorithm + with_wrap/with_align_content builders)
  - **Tests:** 20 CSS flexbox spec scenarios as #[test] functions
  - **Risk:** struct-literal trap — grep `FlexLayout {` before writing new fields; fix any test sites
- [x] Grid layout conformance tests: explicit tracks, auto-placement, spanning — 18 tests shipped in `src/grid.rs` (6 CSS spec conformance scenarios)
- [x] Reactive state tests: signal creation, computed derivation, circular dependency detection, batched update coalescing (~100 SLOC) — deferred (blocked on reactive engine)
  - **Goal:** Full test coverage for the reactive runtime.
  - **Design:** Covered by the S3 implementation in `reactive.rs` — tests live in the `#[cfg(test)]` module of `reactive.rs` and cover all the cases listed in item 1's Tests section.
  - **Files:** `crates/oxiui-core/src/reactive.rs` (test module).
  - **Tests:** Same as item 1's test list.
  - **Risk:** None beyond item 1 risks.
- [x] Property-based tests with proptest: random widget trees, random layout constraints, verify no panics (~100 SLOC) — would require adding `proptest` workspace dep
  - **Goal:** proptest property-based tests for solver (random constraints never panic) and layout (random items never panic) (planned 2026-05-29)
  - **Design:** add `proptest = { workspace = true }` to `[dev-dependencies]` in `crates/oxiui-core/Cargo.toml`; add proptest strategies for `FlexItem` (random basis+grow) and `Constraint` (random variable pairs, non-degenerate strengths); verify no panic on 1000 generated inputs
  - **Files:** `crates/oxiui-core/Cargo.toml`; extend test modules in `layout.rs` and `solver.rs`
  - **Tests:** 2 proptest property functions (~30 SLOC)
  - **Risk:** proptest must be a workspace dep — add to root Cargo.toml [workspace.dependencies] first

## Proposed follow-ups

Items deferred from this slice. Each is in scope for a future `oxiui-core`
expansion; some carry an unresolved design decision that should be settled
before implementation.

- `grid-layout-engine` — split into three milestones: (a) explicit-placement
  with `fr` units, gap and `minmax()`; (b) auto-placement algorithm;
  (c) `grid-template-areas` parser. Add `src/grid.rs`; mirror the flexbox test
  conformance approach.
- `constraint-solver` — cassowary-style linear constraint resolution. **Design
  decision**: pure-Rust port (translate the `cassowary` Apache-2.0 paper /
  Adobe Kiwi reference) versus a simpler simplex tailored to layout. No OxiZ
  dep per task brief.
- `reactive-state-management` — `Signal<T>` / `Computed<T>` with dependency
  tracking and batched coalescing. **Design decision**: subscription model
  (`Cell`/`Rc`-based vs typed-arena vs `&mut`-cursor) — pick what composes with
  the existing immediate-mode `UiCtx` without forcing `Send + Sync` everywhere.
- `multi-window-support` — `WindowId`, per-window widget trees, cross-window
  channel for events. Architecture-level work that touches the facade and
  every adapter.
- [x] `integration-render-backend-trait` + unified `DrawList` (cross-crate) — (covered by paint.rs plan above)
  decouple `UiCtx` from rendering. Currently `UiCtx` doubles as a renderer
  surface; introduce a `RenderBackend` trait and a backend-neutral `DrawList`
  in core, then port adapters.
- Flexbox wrapping (`flex-wrap`) — extend `FlexLayout` to lay out into multiple
  cross-axis lines; this unlocks the deferred 20-scenario CSS conformance
  suite.
- Flexbox conformance tests (~20 scenarios) — blocked on wrapping.
- Grid conformance tests — 18 tests shipped with grid engine (6 CSS spec scenarios).
- Reactive state tests — blocked on reactive engine.
- Property-based panic-freedom tests via `proptest` — would add a workspace
  dev-dep on `proptest` (Pure Rust, OK) and is a good fit for the diff and
  cubic-bezier modules in particular.

## Stage 2 deferred follow-ups

- **oxiui-render-wgpu (next round headline):** CPU foundations (texture-atlas, draw-call batcher consuming DrawList, clip-rect stack, RenderQuality, resource handles, GPU-error mapping) — deferred until DrawList is proven on CPU.
- **oxiui-web self-contained items:** Handle return, MountOptions, typed errors, async mount, native-stub test — deferred until core event loop is wired.

## Performance
- [ ] Layout cache hit-rate benchmarks: measure cache effectiveness on realistic widget trees (100/1000/10000 nodes)
- [ ] Event dispatch allocation-free fast path: pre-allocate handler vectors, avoid per-event heap allocation
- [ ] Widget tree arena allocation: use a typed arena (e.g. `bumpalo` or custom) for widget nodes to improve cache locality
- [ ] Parallel layout computation: independent subtrees can be laid out concurrently via rayon work-stealing
- [ ] Benchmark flexbox layout against reference (Yoga/Taffy) on identical inputs

## Integration
- [x] `oxiui-text` integration: `UiCtx::label()`/`heading()` should accept `TextStyle` for rich formatting via oxitext pipeline
- [ ] `oxiui-theme` integration: `Theme` trait should supply `Padding`/`Border`/`Spacing` design tokens, not just `Palette`+`FontSpec`
- [ ] `oxiui-accessibility` integration: automatic a11y node generation from widget tree (every widget gets an `A11yNode`)
- [x] `oxiui-render-*` integration: define `RenderBackend` trait that render crates implement, replacing the current `UiCtx`-as-renderer conflation
    - **Goal:** core gains a backend-neutral, replayable paint command buffer + a backend trait, so CPU (now) and GPU (later) renderers consume one format. Purely additive — `UiCtx` immediate-mode path untouched.
    - **Design:** new module `src/paint.rs` — `DrawCommand` enum (`#[non_exhaustive]`, owned), `PathData`/`PathVerb`/canonical enums (`FillRule`, `LineJoin`, `LineCap`, `StrokeStyle`, `GradientStop`, `ImageFilter`, `ImageData`), `DrawList` with typed push_* builders + bounds union + clip-depth balance, `RenderBackend` trait (`execute(&DrawList)->Result<()>`, `surface_size`, capability probes with default false). Reuse existing `UiError` variants; no new variant.
    - **Files:** new `src/paint.rs`; `lib.rs` adds `pub mod paint` + re-export `DrawCommand/DrawList/RenderBackend` at crate root.
    - **Tests:** `draw_list_builder_records_command_sequence`, `draw_list_len_and_is_empty`, `clip_push_pop_balance`, `bounds_union_of_draw_commands`, `bounds_excludes_clip_commands`, `clear_resets_bounds_and_depth`, `path_data_builder_and_bounds`, `empty_list_iter_is_empty`.
    - **Risk:** keystone — Stage 2 render-soft depends on this surface. Additive; `UiCtx` untouched.
- [ ] COOLJAPAN ecosystem: layout constraint solver must be Pure Rust (no OxiZ dependency for layout); serialization of widget state via oxicode (not bincode)
