# oxiui-text — Rich text layer bridging OxiUI to OxiText/OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxiui-text.svg)](https://crates.io/crates/oxiui-text)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-text` is the text-integration layer of the COOLJAPAN OxiUI toolkit. It sits between `oxiui-core` and the Pure-Rust [`oxitext`](https://crates.io/crates/oxitext) / `oxifont` shaping-and-rasterization pipeline, giving OxiUI widgets text measurement, shaping, layout, hit-testing, selection, a single-line text input, a multi-line text-area editor, an LRU glyph atlas and shaping cache, font fallback (CJK / emoji), decorations (underline / strikethrough), truncation/ellipsis, IME pre-edit handling, and hyperlink detection.

Built entirely on `oxitext` + `oxifont` — no C/C++ shaping libraries (no HarfBuzz, no FreeType). The crate is `#![forbid(unsafe_code)]` and 100% Pure Rust.

## Installation

```toml
[dependencies]
oxiui-text = "0.1.1"
```

## Quick Start

Create a [`TextPipeline`] from font bytes, then shape, measure, or rasterize:

```rust,no_run
use oxiui_text::{TextPipeline, TextStyle};

# fn main() -> Result<(), oxiui_text::TextError> {
let font_bytes: Vec<u8> = std::fs::read("Inter-Regular.ttf")
    .map_err(|e| oxiui_text::TextError::Other(e.to_string()))?;
let mut pipeline = TextPipeline::from_bytes(&font_bytes)?;

let style = TextStyle::new(18.0).bold().color([192, 202, 245, 255]);

// Measure without rasterizing.
let (w, h) = pipeline.measure("Hello, OxiUI", &style)?;

// Shape into per-line glyph positions for hit testing.
let shaped = pipeline.shape("Hello, OxiUI", &style)?;
assert!(shaped.total_width <= w + 1.0);

// Rasterize into per-glyph bitmaps.
let rendered = pipeline.render("Hello, OxiUI", &style)?;
let _ = (h, rendered);
# Ok(())
# }
```

### Headless editing widgets

`TextInput` and `TextArea` are render-free state machines — drive them from key events and read back the display string and selection:

```rust
use oxiui_text::{TextInput, TextArea, WrapMode};

let mut input = TextInput::with_text("hello");
input.move_end(false);
input.insert(" world");
assert_eq!(input.text(), "hello world");

let mut area = TextArea::new("line one", WrapMode::Hard);
area.insert_newline();
area.insert_char('!');
assert_eq!(area.line_count(), 2);
```

## API Overview

### Text pipeline (crate root)

| Item | Description |
|------|-------------|
| `TextPipeline` | End-to-end shaping + rasterization over `oxitext::Pipeline`. `from_bytes`, `from_system_font`, `set_fallback_fonts`, `shape`, `measure`, `glyph_positions`, `render` |
| `LazyTextPipeline` | Defers font parsing until the first `get()` (fonts can be large); `new`, `get` |
| `TextStyle` | Builder style: `font_family`, `font_size`, `bold`, `italic`, `color`, `letter_spacing`, `line_height`, `max_width`; builders `family`/`bold`/`italic`/`color`/`letter_spacing`/`line_height`/`max_width` |
| `ShapedText` | Shaping result: `lines: Vec<Vec<GlyphPosition>>`, `total_width`, `total_height` |
| `GlyphPosition` | One glyph cluster: `byte_offset`, `x`, `y`, `width`, `height` |

### Re-exports from `oxitext`

`ParagraphMetrics`, `PositionedGlyph`, `RenderResult` are re-exported at the crate root for convenience.

### `editor` module — multi-line editor

`TextArea` — headless multi-line editor with undo/redo, selection, and soft/hard wrap. Cursor movement (`move_up/down/left/right`, `move_home/end`, `move_doc_start/end`), editing (`insert_char`, `insert_newline`, `delete_backward/forward`), `undo`/`redo`, `select_all`, `selected_text`, `line_numbers`, `visible_range`, `scroll_to_cursor`, `display_lines`, `shaped_paragraphs`, `is_modified`. `WrapMode` is `Hard` or `Soft(f32)`.

### `input` module — single-line input

`TextInput` — headless single-line field with optional password masking. `with_text`, `with_password`, `text`, `cursor`, `selection`, `display_text`, `toggle_show_password`, `insert`/`insert_char`, `delete_backward/forward`, cursor + word movement (`move_left/right`, `move_word_left/right`, `move_home/end`), pointer hit-testing (`move_cursor_to_x`, `click`, `double_click`, `triple_click`), `select_all`.

### `label` module

`Label` — a measured text label with optional `max_lines` truncation; `with_max_lines`, `text`, `is_truncated`, `display_text`.

### `layout` module

`TextLayout` — laid-out text supporting alignment and hit testing: `align_glyphs`, `hit_test_fast(x)`, `hit_test(x, y)`. `TextAlign` enum (`Left`/`Center`/`Right`/…).

### `selection` module

`Selection` — anchor/focus caret model with grapheme-aware navigation. `new`, `extend_to`, `is_collapsed`, `normalized`, byte/grapheme conversions (`byte_to_grapheme`, `grapheme_to_byte`), `highlight_rects`, word/line extension (`extend_word_forward/backward`, `extend_line_start/end`).

### `rich` module

`RichText` — a run of styled `Span`s. `push_span`, `spans`, `text`, `split_at`, `merge_adjacent`, `apply_style_range`; `From<&str>` and `Display`.

### `atlas` module — glyph caching

| Item | Description |
|------|-------------|
| `GlyphAtlas` | LRU cache of rasterized glyphs; `new(max_entries)`, `get`, `get_or_rasterize`, `evict_to`, `utilization` |
| `GlyphKey` | Cache key: `glyph_id`, `font_size`, sub-pixel x/y |
| `GlyphEntry` | A cached rasterized glyph bitmap |

### `cache` module

`ShapingCache` — LRU cache of `ShapedText` keyed by `CacheKey`; `new(capacity)`, `get`, `insert`, `hit_rate`, `stats`, `clear`.

### `fallback` module — script fallback

`FallbackChain` — resolves a substitute family for `.notdef` glyphs. `default_chain`, `add_family`, `resolve_glyph`, `families`. Helpers: `is_cjk(char)`, `is_emoji(char)`; `FamilyEntry` type alias.

### `highlight` module — syntax highlighting

`Highlighter` trait + `KeywordHighlighter` (`new(keywords, style)`, `with_rust_keywords(style)`).

### `decoration` module

`TextDecoration` (underline / strikethrough); `line_segments` returns `DecorationSegment`s. `DecorationStyle` enum.

### `truncation` module

`truncate(...)` with `TruncationMode` (start / middle / end ellipsis).

### `ime` module

`Preedit` — IME composition state. `new(text, cursor_range)`, `is_empty`, `underline_segments`, `composition_window_rect`.

### `hyperlink` module

`find_hyperlinks(text) -> Vec<HyperlinkSpan>` — auto-detects URLs in plain text.

## Error variants — `TextError`

| Variant | Description |
|---------|-------------|
| `Pipeline(oxitext::OxiTextError)` | An error from the underlying OxiText shaping/rasterization pipeline |
| `Other(String)` | A miscellaneous text error |

Conversions: `From<oxitext::OxiTextError>` for `TextError`, and `From<TextError>` for `oxiui_core::UiError` (mapped to `UiError::Render`).

## Related crates

- [`oxiui-core`](https://crates.io/crates/oxiui-core) — supplies `UiError`, `TextStyle`, and `RichTextSpan`
- [`oxitext`](https://crates.io/crates/oxitext) / `oxifont` — the Pure-Rust shaping + rasterization pipeline this crate bridges
- [`oxiui`](https://crates.io/crates/oxiui) — the OxiUI facade
- [`oxiui-theme`](https://crates.io/crates/oxiui-theme) — themes whose `FontSpec` feeds the text pipeline

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
