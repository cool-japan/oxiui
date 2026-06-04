//! Headless GPU render tests for [`WgpuBackend`].
//!
//! Each test creates an offscreen GPU target, replays a [`DrawList`], reads the
//! pixels back, and asserts exact RGBA values at known coordinates.  Because
//! the offscreen target uses the *linear* `Rgba8Unorm` format, a solid colour
//! written by the shader is read back byte-for-byte.
//!
//! **Adapter guard:** if no GPU adapter is available, every test prints
//! `"skip: no GPU adapter"` and returns so a no-GPU CI run still passes.  On a
//! host with a working GPU (e.g. Metal on macOS) the pixel assertions run for
//! real.
//!
//! Setup is infallible-by-construction, so `expect` is acceptable here (tests
//! are exempt from the no-`unwrap`/`expect` policy).

use oxiui_core::geometry::{Point, Rect};
use oxiui_core::paint::RenderBackend;
use oxiui_core::{Color, DrawList, UiError};
use oxiui_render_wgpu::WgpuBackend;

/// Try to build a headless backend.  Returns `None` (after printing a skip
/// notice) when no GPU adapter is present, so the test can bail cleanly.
fn try_backend(width: u32, height: u32) -> Option<WgpuBackend> {
    match WgpuBackend::headless(width, height) {
        Ok(b) => Some(b),
        Err(UiError::Unsupported(msg)) => {
            println!("skip: no GPU adapter ({msg})");
            None
        }
        Err(e) => {
            // A hard backend failure (adapter found but device creation failed)
            // is a real error worth surfacing.
            panic!("unexpected headless init error: {e}");
        }
    }
}

const CLEAR: Color = Color(0, 0, 0, 255); // opaque black background
const RED: Color = Color(255, 0, 0, 255);
const GREEN: Color = Color(0, 255, 0, 255);

/// Fetch a pixel as `(r, g, b, a)` from a tightly packed RGBA buffer.
fn pixel(buf: &[u8], width: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
    let idx = ((y * width + x) * 4) as usize;
    (buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3])
}

// ── Readback shape ───────────────────────────────────────────────────────────

#[test]
fn readback_is_tightly_packed() {
    let backend = match try_backend(40, 24) {
        Some(b) => b,
        None => return,
    };
    let buf = backend.readback_rgba().expect("readback must succeed");
    assert_eq!(
        buf.len(),
        (40 * 24 * 4) as usize,
        "readback must be tightly packed width*height*4 with row padding stripped"
    );
}

#[test]
fn surface_size_reports_dimensions() {
    let backend = match try_backend(64, 48) {
        Some(b) => b,
        None => return,
    };
    let sz = backend.surface_size();
    assert!((sz.width - 64.0).abs() < f32::EPSILON);
    assert!((sz.height - 48.0).abs() < f32::EPSILON);
    assert_eq!(backend.width(), 64);
    assert_eq!(backend.height(), 48);
}

// ── Clear ─────────────────────────────────────────────────────────────────────

#[test]
fn empty_list_clears_to_clear_color() {
    let mut backend = match try_backend(16, 16) {
        Some(b) => b,
        None => return,
    };
    backend.set_clear_color(CLEAR);
    let list = DrawList::new();
    backend.execute(&list).expect("execute must succeed");

    let buf = backend.readback_rgba().expect("readback");
    // Every pixel must equal the clear colour.
    for y in 0..16 {
        for x in 0..16 {
            assert_eq!(
                pixel(&buf, 16, x, y),
                (0, 0, 0, 255),
                "pixel ({x},{y}) should be the clear colour"
            );
        }
    }
}

// ── FillRect ───────────────────────────────────────────────────────────────────

#[test]
fn fill_rect_paints_interior_and_leaves_corner_clear() {
    let mut backend = match try_backend(64, 64) {
        Some(b) => b,
        None => return,
    };
    backend.set_clear_color(CLEAR);

    let mut list = DrawList::new();
    list.push_rect(Rect::new(10.0, 10.0, 20.0, 20.0), RED);
    backend.execute(&list).expect("execute");

    let buf = backend.readback_rgba().expect("readback");

    // Centre of the rect (~15,15) must be exactly red (linear format → exact).
    assert_eq!(
        pixel(&buf, 64, 15, 15),
        (255, 0, 0, 255),
        "centre of FillRect must be solid red"
    );
    // A few more interior samples to be sure the whole quad filled.
    assert_eq!(pixel(&buf, 64, 11, 11), (255, 0, 0, 255));
    assert_eq!(pixel(&buf, 64, 28, 28), (255, 0, 0, 255));

    // The (0,0) corner is far outside the rect → still the clear colour.
    assert_eq!(
        pixel(&buf, 64, 0, 0),
        (0, 0, 0, 255),
        "corner outside the rect must remain the clear colour"
    );
    // A pixel just beyond the right edge (x=40) is also untouched.
    assert_eq!(pixel(&buf, 64, 40, 15), (0, 0, 0, 255));
}

// ── FillCircle ──────────────────────────────────────────────────────────────────

#[test]
fn fill_circle_fills_center_and_clears_outside_radius() {
    let mut backend = match try_backend(64, 64) {
        Some(b) => b,
        None => return,
    };
    backend.set_clear_color(CLEAR);

    let mut list = DrawList::new();
    // Circle centred at (32,32), radius 15.
    list.push_circle(Point::new(32.0, 32.0), 15.0, GREEN);
    backend.execute(&list).expect("execute");

    let buf = backend.readback_rgba().expect("readback");

    // Dead centre must be solid green.
    assert_eq!(
        pixel(&buf, 64, 32, 32),
        (0, 255, 0, 255),
        "circle centre must be solid green"
    );

    // A point well inside the radius (8px from centre) must be green-dominant.
    let (r_in, g_in, _b_in, a_in) = pixel(&buf, 64, 32 + 8, 32);
    assert!(
        g_in > 200 && r_in < 40 && a_in > 200,
        "point inside the circle should be green (got r={r_in}, g={g_in}, a={a_in})"
    );

    // A corner pixel is well outside the radius → clear colour.
    assert_eq!(
        pixel(&buf, 64, 2, 2),
        (0, 0, 0, 255),
        "corner outside the circle radius must remain the clear colour"
    );
    // A point just past the radius on the +x axis (radius 15 → x=32+20=52)
    // must also be the clear colour.
    assert_eq!(
        pixel(&buf, 64, 52, 32),
        (0, 0, 0, 255),
        "point beyond the circle radius must be the clear colour"
    );
}

// ── Clipping (PushClip / PopClip → scissor) ─────────────────────────────────────

#[test]
fn clip_restricts_fill_to_clip_rect() {
    let mut backend = match try_backend(64, 64) {
        Some(b) => b,
        None => return,
    };
    backend.set_clear_color(CLEAR);

    let mut list = DrawList::new();
    // Clip to the left half (x in [0,32)).
    list.push_clip(Rect::new(0.0, 0.0, 32.0, 64.0));
    // Fill the entire frame red — only the clipped half should change.
    list.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), RED);
    list.pop_clip();
    backend.execute(&list).expect("execute");

    let buf = backend.readback_rgba().expect("readback");

    // Inside the clip (x=10): red.
    assert_eq!(
        pixel(&buf, 64, 10, 32),
        (255, 0, 0, 255),
        "pixel inside the clip rect must be red"
    );

    // Outside the clip (x=50): unchanged clear colour.
    assert_eq!(
        pixel(&buf, 64, 50, 32),
        (0, 0, 0, 255),
        "pixel outside the clip rect must remain the clear colour"
    );
}

#[test]
fn nested_clip_intersects() {
    let mut backend = match try_backend(64, 64) {
        Some(b) => b,
        None => return,
    };
    backend.set_clear_color(CLEAR);

    let mut list = DrawList::new();
    // Outer clip: top-left 40x40.  Inner clip: a 40x40 offset by (20,20).
    // Intersection is the 20x20 square at (20,20)..(40,40).
    list.push_clip(Rect::new(0.0, 0.0, 40.0, 40.0));
    list.push_clip(Rect::new(20.0, 20.0, 40.0, 40.0));
    list.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), RED);
    list.pop_clip();
    list.pop_clip();
    backend.execute(&list).expect("execute");

    let buf = backend.readback_rgba().expect("readback");

    // Inside the intersection (30,30): red.
    assert_eq!(
        pixel(&buf, 64, 30, 30),
        (255, 0, 0, 255),
        "pixel inside the clip intersection must be red"
    );
    // In the outer-only region (10,10): outside the inner clip → clear.
    assert_eq!(
        pixel(&buf, 64, 10, 10),
        (0, 0, 0, 255),
        "pixel outside the inner clip must remain the clear colour"
    );
    // In the inner-only region (50,50): outside the outer clip → clear.
    assert_eq!(
        pixel(&buf, 64, 50, 50),
        (0, 0, 0, 255),
        "pixel outside the outer clip must remain the clear colour"
    );
}

// ── Sequential commands ──────────────────────────────────────────────────────────

#[test]
fn sequential_rects_paint_distinct_regions() {
    let mut backend = match try_backend(64, 32) {
        Some(b) => b,
        None => return,
    };
    backend.set_clear_color(CLEAR);

    let mut list = DrawList::new();
    list.push_rect(Rect::new(0.0, 0.0, 32.0, 32.0), RED);
    list.push_rect(Rect::new(32.0, 0.0, 32.0, 32.0), GREEN);
    backend.execute(&list).expect("execute");

    let buf = backend.readback_rgba().expect("readback");
    assert_eq!(
        pixel(&buf, 64, 10, 16),
        (255, 0, 0, 255),
        "left region must be red"
    );
    assert_eq!(
        pixel(&buf, 64, 50, 16),
        (0, 255, 0, 255),
        "right region must be green"
    );
}

// ── Capability flags ──────────────────────────────────────────────────────────────

#[test]
fn capabilities_reflect_implemented_features() {
    let backend = match try_backend(8, 8) {
        Some(b) => b,
        None => return,
    };
    // Deferred features (require glyph-atlas pipeline).
    assert!(!backend.supports_text());
    // Implemented in this slice.
    assert!(backend.supports_images());
    assert!(backend.supports_blur());
    assert!(backend.supports_gradients());
    assert!(backend.supports_paths());
}

// ── Device initialisation test (TODO L67) ────────────────────────────────────

#[test]
fn device_init_headless_succeeds_or_skips() {
    // Verify that headless init either succeeds or returns UiError::Unsupported
    // (not any other error variant). This guards against silent panics on
    // GPU-less CI hosts.
    match WgpuBackend::headless(64, 64) {
        Ok(_) => { /* GPU available — good */ }
        Err(UiError::Unsupported(_)) => { /* No GPU — acceptable skip */ }
        Err(e) => panic!("headless() returned unexpected error: {e:?}"),
    }
}

// ── Render pipeline compilation tests (TODO L68 + L69) ───────────────────────

#[test]
fn all_pipelines_compile_without_error() {
    // Constructing WgpuBackend triggers creation of all 5 pipelines
    // (SolidPipeline, GradientPipeline, TexturedPipeline, BlurPipeline,
    //  CompositePipeline) and both quality variants (screen count=N,
    //  mask count=1). If any WGSL shader fails to compile the wgpu validation
    //  layer will panic here.
    let Some(_b) = try_backend(4, 4) else { return };
    // Reaching this point means all pipelines compiled successfully.
}

#[test]
fn balanced_quality_pipelines_compile() {
    // Build with msaa=4 (balanced preset); validates the count=4 pipeline variant.
    match WgpuBackend::headless_with_quality(4, 4, &oxiui_render_wgpu::RenderQuality::balanced()) {
        Ok(_) => { /* pipelines compiled at sample_count=4 (or fell back to 1) */ }
        Err(UiError::Unsupported(_)) => { /* no GPU — acceptable */ }
        Err(e) => panic!("headless_with_quality(balanced) failed: {e:?}"),
    }
}

// ── Scissor rect intersection tests (TODO L72) ───────────────────────────────

#[test]
fn scissor_clip_restricts_draw_to_intersection() {
    let Some(mut b) = try_backend(64, 64) else {
        return;
    };
    let mut list = DrawList::new();
    // Fill whole canvas blue, then push a clip of the top half, fill green inside.
    // Bottom half should remain blue; top half green.
    list.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color(0, 0, 255, 255));
    list.push_clip(Rect::new(0.0, 0.0, 64.0, 32.0)); // top half
    list.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color(0, 255, 0, 255));
    list.pop_clip();
    b.execute(&list).expect("execute");
    // Top-half centre should be green
    let top = b.read_pixel(32, 16).expect("read").expect("pixel");
    assert_eq!(
        (top.0, top.1, top.2, top.3),
        (0, 255, 0, 255),
        "top half should be green (inside clip)"
    );
    // Bottom-half centre should be blue
    let bot = b.read_pixel(32, 48).expect("read").expect("pixel");
    assert_eq!(
        (bot.0, bot.1, bot.2, bot.3),
        (0, 0, 255, 255),
        "bottom half should be blue (outside clip)"
    );
}

#[test]
fn nested_scissor_clips_intersect() {
    let Some(mut b) = try_backend(64, 64) else {
        return;
    };
    let mut list = DrawList::new();
    // Outer clip: top-left 48×48 quadrant. Inner clip: bottom-right 48×48 quadrant.
    // Intersection: 32×32 centre square [16,16]→[48,48].
    list.push_clip(Rect::new(0.0, 0.0, 48.0, 48.0));
    list.push_clip(Rect::new(16.0, 16.0, 48.0, 48.0));
    list.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color(255, 0, 0, 255));
    list.pop_clip();
    list.pop_clip();
    b.execute(&list).expect("execute");
    // Pixel inside intersection → red
    let inside = b.read_pixel(32, 32).expect("read").expect("pixel");
    assert_eq!(inside.3, 255, "pixel in intersection must be opaque (red)");
    // Pixel outside intersection (top-left corner) → transparent
    let outside = b.read_pixel(5, 5).expect("read").expect("pixel");
    assert_eq!(
        outside.3, 0,
        "pixel outside intersection must be transparent"
    );
}

// ── Draw-batching coalescing test (TODO L71) ─────────────────────────────────

#[test]
fn many_solid_rects_coalesce_into_few_draws() {
    // 100 FillRect commands (10×10 non-overlapping 20×20 rects) with no clip
    // changes should all be merged into a single solid draw call (one
    // vertex-buffer range).  We verify:
    //   (a) all rects render correctly (sampling a red column and a blue column),
    //   (b) the operation completes without OOM / timeout (i.e. isn't O(n) GPU
    //       submissions),
    //   (c) the draw-call count is small (coalescing is working).
    let Some(mut b) = try_backend(256, 256) else {
        return;
    };
    let mut list = DrawList::new();
    // Lay out 10×10 = 100 non-overlapping 20×20 rects, tiled across 256×256.
    // Alternate red/blue columns so we can sample both colours.
    for row in 0u32..10 {
        for col in 0u32..10 {
            let x = col as f32 * 25.0 + 2.0;
            let y = row as f32 * 25.0 + 2.0;
            let color = if col % 2 == 0 {
                Color(255, 0, 0, 255)
            } else {
                Color(0, 0, 255, 255)
            };
            list.push_rect(Rect::new(x, y, 20.0, 20.0), color);
        }
    }
    b.execute(&list).expect("execute");
    // Sample a red rect centre (col=0, row=0): pixel at (12, 12).
    let red_px = b.read_pixel(12, 12).expect("read red").expect("pixel");
    assert_eq!(
        (red_px.0, red_px.1, red_px.2),
        (255, 0, 0),
        "col=0 (red column) centre should be red"
    );
    // Sample a blue rect centre (col=1, row=0): pixel at (37, 12).
    let blue_px = b.read_pixel(37, 12).expect("read blue").expect("pixel");
    assert_eq!(
        (blue_px.0, blue_px.1, blue_px.2),
        (0, 0, 255),
        "col=1 (blue column) centre should be blue"
    );
    // After executing, verify draw counts: 100 unclipped same-kind rects with
    // no scissor changes should coalesce into ≤3 draws (typically just 1).
    let stats = b.frame_stats();
    assert!(
        stats.draw_calls <= 3,
        "100 unclipped same-kind rects should coalesce into ≤3 draws, got {}",
        stats.draw_calls
    );
}

// ── Multi-size backends independence test (TODO L74) ─────────────────────────

#[test]
fn different_size_backends_work_independently() {
    // Proxy for resize: two independent backends at different sizes both work
    // correctly.  Verifies that each backend's offscreen texture is correctly
    // sized and that they do not share any GPU state.
    let Some(mut b_small) = try_backend(32, 32) else {
        return;
    };
    let Some(mut b_large) = try_backend(128, 128) else {
        return;
    };
    let mut list = DrawList::new();
    list.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0), Color(0, 255, 0, 255));
    b_small.execute(&list).expect("small execute");
    b_large.execute(&list).expect("large execute");
    // Small backend: pixel within its 32×32 viewport should be green.
    let s = b_small
        .read_pixel(16, 16)
        .expect("read small")
        .expect("pixel");
    assert_eq!(
        (s.0, s.1, s.2, s.3),
        (0, 255, 0, 255),
        "small backend centre should be green"
    );
    // Large backend: same green fill visible well inside 128×128.
    let l = b_large
        .read_pixel(50, 50)
        .expect("read large")
        .expect("pixel");
    assert_eq!(
        (l.0, l.1, l.2, l.3),
        (0, 255, 0, 255),
        "large backend centre should be green"
    );
}

// ── Resize handling tests ─────────────────────────────────────────────────────

/// Verify that `WgpuBackend::resize` works without panic and the new surface
/// reports the correct dimensions.
#[test]
fn resize_updates_surface_dimensions() {
    let Some(mut b) = try_backend(32, 32) else {
        return;
    };
    b.resize(128, 64).expect("resize must succeed");
    assert_eq!(b.width(), 128, "width must be updated after resize");
    assert_eq!(b.height(), 64, "height must be updated after resize");
    let sz = b.surface_size();
    assert!(
        (sz.width - 128.0).abs() < f32::EPSILON,
        "surface_size width must be 128 after resize"
    );
    assert!(
        (sz.height - 64.0).abs() < f32::EPSILON,
        "surface_size height must be 64 after resize"
    );
}

/// Verify that rendering after a resize produces correct pixels.
#[test]
fn resize_then_render_produces_correct_pixels() {
    let Some(mut b) = try_backend(32, 32) else {
        return;
    };
    // Resize to 64×64.
    b.resize(64, 64).expect("resize");

    // Render a red rect covering the entire 64×64 canvas.
    let mut list = DrawList::new();
    list.push_rect(Rect::new(0.0, 0.0, 64.0, 64.0), Color(255, 0, 0, 255));
    b.execute(&list).expect("execute after resize");

    // Sample a pixel from the centre.
    let px = b.read_pixel(32, 32).expect("read").expect("pixel");
    assert_eq!(
        (px.0, px.1, px.2, px.3),
        (255, 0, 0, 255),
        "pixel at (32,32) must be red after resize+render"
    );
}

/// Resize to zero dimensions must return an error, not panic.
#[test]
fn resize_zero_dimension_returns_error() {
    let Some(mut b) = try_backend(32, 32) else {
        return;
    };
    assert!(
        b.resize(0, 32).is_err(),
        "resize(0, 32) must return an error"
    );
    assert!(
        b.resize(32, 0).is_err(),
        "resize(32, 0) must return an error"
    );
    // Backend must still be usable at the original dimensions.
    assert_eq!(b.width(), 32, "width must be unchanged after failed resize");
}
