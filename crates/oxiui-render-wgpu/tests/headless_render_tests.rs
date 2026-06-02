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
    // Deferred features (require texture-atlas / blur pipeline).
    assert!(!backend.supports_text());
    assert!(!backend.supports_images());
    assert!(!backend.supports_blur());
    // Implemented in this slice.
    assert!(backend.supports_gradients());
    assert!(backend.supports_paths());
}
