# oxiui-render-soft — Pure-Rust software (CPU) render backend for OxiUI

[![Crates.io](https://img.shields.io/crates/v/oxiui-render-soft.svg)](https://crates.io/crates/oxiui-render-soft)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxiui-render-soft` is the **Pure-Rust software renderer** for OxiUI. It rasterizes a [`oxiui_core::paint::DrawList`] entirely on the CPU into an in-memory `0xAARRGGBB` framebuffer — no GPU, no window, and no display connection required. Every pixel is produced by Rust code in this crate (with `png` for PNG export and the optional `oxiui-text` pipeline for glyph rasterization), making it the canonical backend for **headless rendering**, CI smoke tests, ffi-audit containers, and embedded targets.

The crate is built around a clip-correct [`Canvas`] that replays draw commands through a [`ClipStack`], plus a [`SoftBackend`] that implements [`oxiui_core::paint::RenderBackend`]. It includes a full 2-D rasterization stack — scanline polygon fill with anti-aliasing, Bézier paths, gradients, box shadows via Gaussian blur, ordered dithering, and a tile iterator for (optionally `rayon`-parallel) rendering. `#![forbid(unsafe_code)]` is enforced crate-wide.

## Installation

```toml
[dependencies]
oxiui-render-soft = "0.1.2"
```

To build the GPU-free audit configuration explicitly, disable default features:

```toml
[dependencies]
oxiui-render-soft = { version = "0.1.2", default-features = false }
```

## Quick Start

Render a scene headlessly and export it as a PNG — no display needed:

```rust
use oxiui_core::Color;
use oxiui_render_soft::headless::render_headless_scene;

# fn main() -> Result<(), oxiui_render_soft::SoftRenderError> {
let buffer = render_headless_scene(128, 96, |canvas| {
    canvas.fill_rect(16.0, 16.0, 64.0, 48.0, Color(255, 0, 0, 255));
    canvas.fill_circle(96.0, 48.0, 20.0, Color(0, 128, 255, 255));
});

assert_eq!(buffer.width, 128);
assert!(buffer.has_content());
buffer.save_png(std::path::Path::new("/tmp/scene.png"))?;
# Ok(())
# }
```

### Replaying a `DrawList` through `SoftBackend`

```rust
use oxiui_core::paint::{DrawList, RenderBackend};
use oxiui_core::{geometry::Rect, Color};
use oxiui_render_soft::{SoftBackend, headless::PixelFormat};

# fn main() -> Result<(), oxiui_core::UiError> {
let mut backend = SoftBackend::with_background(64, 64, Color(26, 27, 38, 255));

let mut list = DrawList::new();
list.push_rect(Rect::new(8.0, 8.0, 48.0, 48.0), Color(255, 255, 255, 255));
backend.execute(&list)?;

// Export the framebuffer in any supported byte layout.
let bytes = backend.to_bytes(PixelFormat::Bgra8);
assert_eq!(bytes.len(), 4 * 64 * 64);
# Ok(())
# }
```

## API Overview

### Top-level renderer types

| Item | Kind | Description |
|------|------|-------------|
| `SoftRenderer` | struct | Stateless software renderer. `new()`, `with_size(w, h)`, `clear_frame(w, h, color) -> Result<Vec<u32>, UiError>`, `render(w, h, bg, draw_fn) -> Framebuffer` |
| `SoftBackend` | struct | CPU [`RenderBackend`] that replays a `DrawList` onto a `Framebuffer` |
| `AaMode` | enum | Anti-aliasing strategy: `None`, `Msaa4x`, `Supersampling` |
| `ShadowQuality` | enum | Shadow fidelity: `Off`, `Low`, `High` |
| `SoftRenderQuality` | struct | Combined preset (`aa_mode`, `shadow_quality`); `low()`, `balanced()`, `high()` |
| `SoftRenderError` | enum | Crate error type (see below) |

### `SoftBackend` methods

`SoftBackend` implements [`oxiui_core::paint::RenderBackend`].

| Method | Description |
|--------|-------------|
| `SoftBackend::new(w, h)` | Backend with a transparent framebuffer |
| `SoftBackend::with_background(w, h, bg)` | Backend pre-filled with a solid colour |
| `SoftBackend::with_quality(w, h, quality)` | Backend pre-configured with a [`SoftRenderQuality`] preset |
| `set_quality(quality)` / `quality()` | Set / borrow the active quality preset |
| `frame()` | Borrow the underlying [`Framebuffer`] |
| `into_framebuffer()` | Consume the backend, returning the [`Framebuffer`] |
| `clear(color)` | Fill the framebuffer with a solid colour |
| `width()` / `height()` | Framebuffer dimensions in pixels |
| `to_bytes(format)` | Export pixels as a flat byte vector in a [`PixelFormat`] |
| `apply_shadow_spec(rect, spec)` | Apply an [`oxiui_theme::ShadowSpec`] (requires `theme` feature) |
| `execute(&list)` | (`RenderBackend`) replay a `DrawList` clip-correctly |
| `surface_size()` | (`RenderBackend`) target size as `Size` |
| `supports_blur/gradients/paths/images()` | (`RenderBackend`) all return `true` |
| `supports_text()` | (`RenderBackend`) `true` when the `text` feature loaded a font |

### `headless` module

Display-free rendering helpers for CI / ffi-audit.

| Item | Description |
|------|-------------|
| `render_headless_once(w, h) -> RgbaBuffer` | Fill a buffer with the COOLJAPAN dark-theme background |
| `render_headless_scene(w, h, draw_fn) -> RgbaBuffer` | Paint a custom scene via a [`Canvas`] callback |
| `RgbaBuffer` | RGBA pixel buffer (`width`, `height`, `data`); `has_content()`, `save_png(path)` |
| `PixelFormat` | Output byte layout: `Argb32`, `Bgra8`, `Rgb565` |
| `HEADLESS_BG_COLOR` | `[u8; 4]` default background (`#1A1B26`, Tokyo Night) |

### `framebuffer` module

| Item | Description |
|------|-------------|
| `Framebuffer` | CPU pixel buffer (`0xAARRGGBB`, row-major) |
| `Framebuffer::new(w, h)` / `with_fill(w, h, color)` | Allocate transparent / pre-filled |
| `pixels()` / `pixels_mut()` | Borrow the raw `u32` pixel slice |
| `width()` / `height()` / `clear(color)` | Dimensions and full-buffer fill |
| `pack(&Color)` / `pack_rgba(r,g,b,a)` / `unpack(u32)` | Free functions for `0xAARRGGBB` conversion |

### `draw` module — `Canvas`

`Canvas` borrows a `Framebuffer` and owns a [`ClipStack`]; all drawing is clipped to the current effective clip with straight-alpha source-over compositing.

| Method group | Methods |
|--------------|---------|
| Construction / state | `new(fb)`, `set_aa(bool)`, `aa` field, `framebuffer()` |
| Clipping | `push_clip(x, y, w, h)`, `pop_clip()` |
| Rectangles | `fill_rect`, `stroke_rect`, `fill_rounded_rect`, `fill_rounded_rect_per_corner` |
| Circles / ellipses | `fill_circle`, `fill_ellipse` (analytical coverage AA) |
| Lines | `draw_line`, `draw_line_wu` (Wu AA), `draw_line_thick`, `draw_line_dashed` |
| Paths | `fill_path`, `stroke_path` |
| Gradients | `fill_linear_gradient_cmd`, `fill_radial_gradient_cmd` |
| Bézier | `draw_quad_bezier`, `draw_cubic_bezier` |
| Images | `blit_rgba`, `blit_bilinear`, `blit_nine_slice` |
| Shadows | `box_shadow_cmd` |
| Supporting types | `SrcImage<'a>` (borrowed RGBA view; `new`, `is_valid`), `DashPattern` (`new(dash_len, gap_len)`) |

Glyph blitting helper: `blit_glyph_bitmap(fb, x, y, w, h, pixels, color)` (re-exported at crate root).

### `clip` module

| Item | Description |
|------|-------------|
| `ClipRect` | Integer clip rectangle with `contains`, `from_rect`, `full`, intersection |
| `ClipStack` | Stack of intersected clip rects; `push`, `pop`, `current` |

### `scanline` module

| Item | Description |
|------|-------------|
| `fill_polygon` | Active-Edge-Table scanline fill with vertical-supersample coverage AA |
| `fill_triangle` | Triangle fill convenience |
| `FillRule` | Winding rule: even-odd / non-zero |

### `path` module

| Item | Description |
|------|-------------|
| `Path` / `PathBuilder` | 2-D paths with Bézier flattening |
| `StrokeStyle` | Stroke configuration |
| `Cap` / `Join` | Line-cap and line-join styles |

### `gradient` module

| Item | Description |
|------|-------------|
| `LinearGradient` / `RadialGradient` | Gradient fills with sRGB colour interpolation |
| `GradientStop` | Colour stop |
| `lerp_color` | sRGB colour interpolation helper |

### `blend` module

| Item | Description |
|------|-------------|
| `BlendMode` | Extended blend modes: multiply / screen / overlay / darken / lighten |
| `RgbaUnit` | Premultiplied-alpha unit helper |
| `blend_mode` / `blend_pixel` / `composite_into` | Blend / composite functions |

### `shadow` module

| Item | Description |
|------|-------------|
| `box_shadow` | Box shadow via separable 1-D Gaussian blur |
| `gaussian_blur_alpha` | Alpha-channel Gaussian blur |
| `GaussianCache` | Cache of precomputed Gaussian kernels |

### `dither` module

| Item | Description |
|------|-------------|
| `ordered_dither_rgba` | Bayer-matrix ordered dithering for reduced-bit output |
| `BayerMatrix` | Ordered-dither threshold matrix |

### `tile` module

| Item | Description |
|------|-------------|
| `Tile` / `TileIter` | 64×64 render-tile iteration |
| `tiles_for` / `collect_tiles` / `render_tiles` | Tile enumeration and serial driver |
| `render_parallel` | `rayon`-parallel tile driver (requires `parallel` feature) |
| `DirtyRegion` | Dirty-rectangle tracking |
| `DEFAULT_TILE_SIZE` | Default tile edge length |

### Re-exports

When the `theme` feature is active, [`oxiui_theme::ShadowSpec`] is re-exported as `oxiui_render_soft::ShadowSpec`.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `text` | yes | Enable glyph shaping / rasterization via `oxiui-text`; `DrawText` commands are rendered instead of skipped |
| `theme` | no | Enable `oxiui-theme` integration (`apply_shadow_spec`, `ShadowSpec` re-export) |
| `parallel` | no | Enable the `rayon`-backed parallel tile driver (`render_parallel`) |

Disable default features (`default-features = false`) for the GPU- and text-pipeline-free audit build.

## Error Variants

`SoftRenderError` implements `std::error::Error` and `Display`.

| Variant | Description |
|---------|-------------|
| `Io(String)` | An I/O error (e.g. cannot create or write the PNG output file) |
| `Png(String)` | A PNG encoding / decoding error |

`SoftBackend::execute` returns [`oxiui_core::UiError`] (the shared OxiUI error type).

## Related Crates

- [`oxiui`](../../) — the OxiUI facade crate.
- [`oxiui-core`](../oxiui-core) — `RenderBackend`, `DrawList`, `DrawCommand`, `Color`, `UiError`, geometry types.
- [`oxiui-render-wgpu`](../oxiui-render-wgpu) — the GPU render path (wgpu) for comparison.
- [`oxiui-text`](../oxiui-text) — the text-shaping pipeline used by the `text` feature.
- [`oxiui-theme`](../oxiui-theme) — design tokens and `ShadowSpec` used by the `theme` feature.

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
