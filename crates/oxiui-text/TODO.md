# oxiui-text TODO

## Slice 5 — Implementation Plan (completed 2026-05-29)

### What was done
- Fixed the `from_bytes` bridge: now takes `&[u8]` and returns `Result<Self, TextError>`.
- Added `TextError` type wrapping `oxitext::OxiTextError`.
- Implemented `TextStyle` builder with: `font_family`, `font_size`, `bold`, `italic`,
  `color`, `letter_spacing`, `line_height`, `max_width`.
- Added `GlyphPosition` and `ShapedText` types bridging `PositionedGlyph` → OxiUI types.
- Extended `TextPipeline` with: `shape()`, `measure()`, `glyph_positions()`, `render()`,
  `from_system_font()`, `set_fallback_fonts()`.
- Created `src/layout.rs`: `TextLayout` with `TextAlign`, `align_glyphs()`, `hit_test()`.
- Created `src/selection.rs`: `Selection` with grapheme↔byte conversion, word/line nav,
  highlight rect computation.
- Created `src/rich.rs`: `RichText` + `Span` with `split_at`, `merge_adjacent`,
  `apply_style_range`, `Display`, `From<&str>`.
- Created `src/cache.rs`: hand-rolled LRU `ShapingCache` (no `lru` crate).
- Created `src/fallback.rs`: `FallbackChain` with Unicode-range `is_cjk` / `is_emoji`.
- Created `src/decoration.rs`: `TextDecoration` / `DecorationStyle` / `DecorationSegment`.
- Created `src/truncation.rs`: `truncate()` with `TruncationMode::End` and `Middle`.
- Created `src/hyperlink.rs`: `find_hyperlinks()` detecting `http://`, `https://`, `www.`.
- Updated integration tests (`tests/text_tests.rs`) to match new API.
- 57 tests, all passing. Zero clippy warnings.

### OxiText API findings
- `Pipeline::from_bytes(&[u8]) -> Result<Self, OxiTextError>` — takes slice, not owned Vec.
- `Pipeline::measure(text, style) -> Result<ParagraphMetrics, OxiTextError>` — returns
  `ParagraphMetrics { total_width, total_height, line_count, overflow, truncated }`.
- `Pipeline::shape_and_layout(text, style) -> Result<LayoutResult, OxiTextError>` — returns
  `LayoutResult { glyphs: Vec<PositionedGlyph>, lines: Vec<Line>, metrics, … }`.
- `TextStyle` in oxitext has: `font_size`, `max_width`, `flow_direction`, `alignment`,
  `line_spacing`. No `bold`/`italic`/`color` — those are OxiUI-layer concerns.
- No `Pipeline::glyph_positions()` — implemented in this crate by wrapping
  `shape_and_layout`.
- `LayoutResult::hit_test(x, y)` exists upstream; this crate wraps it via `TextLayout`.

## Core Implementation
- [x] Text layout engine: line-breaking via oxitext UAX #14, alignment (left/center/right/justify), hit-testing (~220 SLOC in layout.rs)
- [x] Text input widget: editable single-line field with cursor positioning, text insertion/deletion at cursor, selection via shift+arrow, mouse click-to-position, double-click word select, triple-click line select (~350 SLOC)
    - **Goal:** turn the existing shaping/layout/selection infrastructure into interactive, headless-testable text widgets. Crate-local; NOT UiCtx wiring (adapters do that).
    - **Design:** `TextInput{text, cursor, selection, scroll_offset}` over existing `selection.rs`/`layout.rs`: insert/delete_backward/forward, cursor motion (left/right/word/home/end), move_cursor_to via hit_test, click/double-click (word)/triple-click (line) selection, selected_text(). `Label{text, max_lines}` with overflow→ellipsis via existing truncation, is_truncated(). Password: mask_char + masked_display() (U+2022) + show/hide toggle. IME: `Preedit{text, cursor_range}` → underline DecorationSegments + composition window rect. `Highlighter` trait `fn highlight_line(&self,&str)->Vec<(Range<usize>,TextStyle)>` + `KeywordHighlighter` reference impl (pure Rust keyword-set, no external dep).
    - **Files:** new `src/{input.rs,label.rs,highlight.rs}` (+ `src/ime.rs` if preedit grows); `lib.rs` re-exports.
    - **Tests:** insert/delete cursor, word/line motion, click/double/triple selection, hit-test positioning, password mask+toggle, label truncation+is_truncated, IME preedit underline+window rect, Highlighter keyword spans+RichText.
    - **Defer:** multi-line textarea (~400 SLOC), emoji COLR/CBDT (needs oxifont color-glyph upstream support).
- [x] **Multi-line text editor: textarea with vertical scroll, line numbers, soft/hard wrap, undo/redo** (completed 2026-05-29)
  - **Goal:** a headless-testable multi-line editor built on existing TextInput/selection/layout infrastructure.
  - **Design:** new `src/editor.rs`. `TextArea{lines:Vec<String>, cursor:(row,col), selection, scroll_offset, wrap:WrapMode}`. Operations: insert/delete with newline handling, cursor motion (up/down/line-home/end/doc-home/end/word), `WrapMode{Hard,Soft(max_width)}` via existing layout/truncation, **undo/redo stack** of reversible `EditOp{Insert,Delete}` with consecutive-char coalescing, `line_numbers()` gutter metadata, `visible_range(viewport_height)` for virtual scroll. Plus **lazy font loading**: OnceCell wrapper around the existing pipeline — defer oxifont face parsing until first glyph request.
  - **Files:** new `src/editor.rs`; `lib.rs` re-exports; lazy-loading touches existing font entry point additively.
  - **Prerequisites:** none (selection/layout/truncation exist).
  - **Tests (~14):** multi-line insert/delete with newline; cursor up/down preserves goal column; soft-wrap splits at width; hard-wrap keeps explicit newlines; undo reverses insert; redo re-applies; undo coalesces consecutive chars; selection across lines; visible_range maps scroll offset; line-number count; lazy font parses once (spy).
  - **Risk:** undo/redo coalescing is the subtle part — model as explicit reversible ops, test coalescing boundaries. Defer: emoji COLR/CBDT (upstream oxifont), incremental reshaping, SIMD hit-test, glyph-atlas.
- [x] Text selection model: `Selection` type with anchor/focus positions, byte-offset to grapheme-cluster mapping, selection rendering (highlight rect computation), copy-to-clipboard integration (~285 SLOC in selection.rs)
- [x] IME composition rendering: display preedit string with underline decoration, cursor-within-composition indicator, composition window positioning relative to input caret, commit handling (~200 SLOC)
- [x] Rich text model: `RichText` type with spans carrying style (bold/italic/underline/strikethrough/color/size/font), span merging, attributed string API (~229 SLOC in rich.rs)
- [x] Syntax highlighting adapter: trait `Highlighter` with `highlight_line(line: &str) -> Vec<(Range, Style)>`, integration point for tree-sitter or regex-based highlighters (~100 SLOC)
- [x] Text shaping cache: LRU cache keyed on `(text, style, max_width)` → `ShapedLine`, configurable capacity, cache statistics (hit/miss/eviction) (~251 SLOC in cache.rs)
- [x] Font fallback chain: ordered list of font faces, automatic fallback for missing glyphs (CJK → Latin → emoji → tofu), per-run font selection (~207 SLOC in fallback.rs)
- [ ] Emoji rendering: color emoji bitmap extraction from CBDT/COLR tables via oxifont, scaling to match text size, inline emoji in mixed text (~120 SLOC)
- [x] Text decoration: underline (solid/dashed/dotted/wavy), overline, strikethrough with configurable thickness and color, offset from baseline (~201 SLOC in decoration.rs)
- [x] Text truncation / ellipsis: single-line overflow with "…" (U+2026), middle truncation for file paths (~309 SLOC in truncation.rs)
- [x] Password masking: display bullet/asterisk characters, toggle show/hide, mask delay timer (~40 SLOC)
- [x] Label widget: static text display with optional max-lines, overflow handling, tooltip-on-hover for truncated text (~80 SLOC)
- [x] Hyperlink detection and rendering: URL pattern matching (http/https/www), clickable underline style, hover cursor change, callback on click (~178 SLOC in hyperlink.rs)

## API Improvements
- [x] `TextPipeline::shape()` accepts `max_width` parameter (via `TextStyle::max_width`)
- [x] Add `TextPipeline::measure(text, style) -> Result<(f32, f32), TextError>`
- [x] Expose `TextPipeline::glyph_positions(text, style) -> Result<Vec<GlyphPosition>, TextError>`
- [x] Support `TextPipeline::from_system_font(family: &str)` with system font discovery
- [x] Builder pattern for `TextStyle` (e.g. `TextStyle::new(16.0).bold().italic()`)
- [x] `RichText` implements `Display` and `From<&str>`

## Testing
- [x] Selection model tests: collapsed, forward, backward, word-select, line-select (9 tests)
- [x] IME composition tests
- [x] Rich text tests: span creation, splitting, style merge, display (8 tests)
- [x] Shaping cache tests: hit, miss, eviction, LRU order, clear (7 tests)
- [x] Text input widget tests
- [x] Font fallback tests: CJK, emoji, chain entries (7 tests)
- [x] Decoration tests: underline, multiple types, empty glyphs (4 tests)
- [x] Truncation tests: mode-none, short-unchanged, end-ellipsis, middle-ellipsis (4 tests)
- [x] Hyperlink tests: https, http, www, plain, multiple, trailing-punct, offsets (8 tests)
- [x] Integration tests: shape glyphs, nonzero metrics, render bitmaps (4 tests)

## Performance
- [x] Incremental reshaping: when text is edited, only reshape the affected paragraph
    - **Goal:** `TextArea` tracks `dirty_paragraphs: HashSet<usize>` + `Vec<Option<ShapedText>>` cache; edits mark only affected line(s) dirty; `shaped_paragraphs(&mut self, pipeline: &mut TextPipeline) -> Vec<ShapedText>` reshapes only dirty lines (planned 2026-05-29)
    - **Design:** on `insert_char(row,col,ch)` / `delete_backward` / `insert_newline` / `delete_newline`: mark only the changed line index(es) dirty; clear dirty set after `shaped_paragraphs` call; `Vec<Option<ShapedText>>` resizes with line count; None = needs reshape; Some = cached
    - **Files:** `crates/oxiui-text/src/editor.rs` (TextArea struct extension + shaped_paragraphs method)
    - **Tests:** spy test: 1000-line TextArea, single insert at line 500 → exactly 1 call to TextPipeline::shape (implement a call-counting wrapper); verify cached lines not re-shaped
    - **Risk:** line count changes (insert_newline/delete_newline) shift indices — invalidate/resize the cache Vec on count change
- [x] Glyph atlas integration: pre-rasterized glyph bitmaps shared with render backends
    - **Goal:** new `GlyphAtlas` (new `atlas.rs`) — LRU glyph-bitmap cache keyed by `GlyphKey{glyph_id,font_size_pixels,subpixel_offset_16ths}`; used by CPU render backends to avoid re-rasterizing glyphs per frame (planned 2026-05-29)
    - **Design:** `GlyphKey{glyph_id:u16, font_size_pixels:u32, subpixel_offset_16ths:(u8,u8)}` (quantize subpixel to 1/16 px steps for cache stability); `GlyphEntry{bitmap:Bitmap, advance_x:f32, bearing:(i32,i32)}`; `GlyphAtlas{cache:HashMap<GlyphKey,GlyphEntry>, lru:VecDeque<GlyphKey>, max_entries:usize}` with `new(max:usize)`, `get(&GlyphKey)->Option<&GlyphEntry>`, `get_or_rasterize(&mut TextPipeline,key,text,style)->Result<&GlyphEntry,UiError>`, `evict_to(max)`, `len()`, `utilization()->f32`; LRU: on hit move key to back of VecDeque, on insert evict front if over max
    - **Files:** new `crates/oxiui-text/src/atlas.rs`; `crates/oxiui-text/src/lib.rs` (re-export GlyphAtlas, GlyphKey, GlyphEntry); also add `TextLayout::hit_test_fast(x:f32)->usize` via `partition_point` in layout.rs
    - **Tests:** atlas hit returns same Bitmap; LRU eviction drops oldest; utilization = len/max; hit_test_fast on 100-glyph line returns correct index; get_or_rasterize calls shape exactly once for same key
    - **Risk:** Bitmap must be Clone for cache insertion; GlyphKey must impl Hash+Eq; subpixel quantization prevents cache explosion
- [ ] SIMD-accelerated hit-test binary search over glyph positions for large texts
- [x] Lazy font loading: defer oxifont face parsing until first glyph request for that font

## Integration
- [ ] `oxiui-core` integration: `UiCtx::text_input()` and `UiCtx::text_area()` methods
- [ ] `oxiui-render-wgpu` integration: glyph atlas texture upload, SDF text rendering pipeline
- [ ] `oxiui-render-soft` integration: glyph bitmap blitting into CPU framebuffer
- [ ] `oxiui-egui` integration: bridge `TextPipeline` output to egui's text rendering
- [ ] `oxiui-accessibility` integration: expose text content, selection range, cursor position
- [ ] `oxiui-table` integration: cell text rendering through `TextPipeline`
- [ ] COOLJAPAN ecosystem: all text shaping via oxitext + oxifont only

## Proposed follow-ups
- **Multi-line textarea:** ~400 SLOC, scroll, line numbers, undo/redo — own follow-up slice.
- **Emoji COLR/CBDT rendering:** depends on upstream oxifont color-glyph table support; cross-project dependency.
- **Glyph atlas integration:** shared with render backends — cross-crate ARCH item.
- **oxiui-core UiCtx::text_input()/text_area() integration:** cross-crate; adapters do the wiring.
