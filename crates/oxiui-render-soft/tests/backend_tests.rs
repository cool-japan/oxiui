use oxiui_core::geometry::{Point, Rect};
use oxiui_core::paint::{GradientStop, PathData, RenderBackend};
use oxiui_core::{Color, DrawList};
use oxiui_render_soft::SoftBackend;

// ── Basic construction ─────────────────────────────────────────────────────────

#[test]
fn softbackend_constructs_and_sizes() {
    let backend = SoftBackend::new(64, 48);
    let sz = backend.surface_size();
    assert!((sz.width - 64.0).abs() < f32::EPSILON);
    assert!((sz.height - 48.0).abs() < f32::EPSILON);
    assert_eq!(backend.width(), 64);
    assert_eq!(backend.height(), 48);
}

#[test]
fn softbackend_capabilities() {
    let backend = SoftBackend::new(100, 100);
    assert!(backend.supports_blur());
    assert!(backend.supports_gradients());
    assert!(backend.supports_paths());
    assert!(backend.supports_images());
    // supports_text() is true when the `text` feature is enabled (default)
    // and the embedded font loads successfully.
    #[cfg(feature = "text")]
    assert!(backend.supports_text());
    #[cfg(not(feature = "text"))]
    assert!(!backend.supports_text());
    assert!((backend.surface_size().width - 100.0).abs() < f32::EPSILON);
}

// ── Empty list ────────────────────────────────────────────────────────────────

#[test]
fn softbackend_empty_list_is_noop() {
    let bg = Color(10, 20, 30, 255);
    let mut backend = SoftBackend::with_background(10, 10, bg);
    let list = DrawList::new();
    backend.execute(&list).expect("execute should succeed");
    // All pixels should still be the background.
    let fb = backend.into_framebuffer();
    for y in 0..10u32 {
        for x in 0..10u32 {
            let (r, g, b, a) = fb.get_rgba(x, y).expect("pixel should exist");
            assert_eq!(
                (r, g, b, a),
                (10, 20, 30, 255),
                "pixel ({x},{y}) should be unchanged background"
            );
        }
    }
}

// ── FillRect ──────────────────────────────────────────────────────────────────

#[test]
fn softbackend_executes_fillrect() {
    let bg = Color(255, 255, 255, 255);
    let fill = Color(255, 0, 0, 255);
    let mut backend = SoftBackend::with_background(20, 20, bg);

    let mut list = DrawList::new();
    list.push_rect(Rect::new(5.0, 5.0, 10.0, 10.0), fill);
    backend.execute(&list).expect("execute");

    // Centre of rect must be filled.
    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(10, 10).expect("centre pixel");
    assert_eq!(
        (r, g, b, a),
        (255, 0, 0, 255),
        "centre of FillRect should be red"
    );
    // Outside the rect must be background.
    let (r2, g2, b2, a2) = fb.get_rgba(0, 0).expect("corner pixel");
    assert_eq!(
        (r2, g2, b2, a2),
        (255, 255, 255, 255),
        "corner outside rect should remain white"
    );
}

// ── PushClip / PopClip ────────────────────────────────────────────────────────

#[test]
fn softbackend_clip_then_fill_matches_canvas() {
    let bg = Color(0, 0, 0, 255);
    let fill = Color(0, 255, 0, 255);
    let w = 20u32;
    let h = 20u32;
    let mut backend = SoftBackend::with_background(w, h, bg);

    let mut list = DrawList::new();
    // Clip to the left 10 columns.
    list.push_clip(Rect::new(0.0, 0.0, 10.0, 20.0));
    // Fill the entire frame — only the clipped region should change.
    list.push_rect(Rect::new(0.0, 0.0, 20.0, 20.0), fill);
    list.pop_clip();
    backend.execute(&list).expect("execute");

    // Inside the clip: green.
    let fb = backend.frame();
    let (r, g, _, _) = fb.get_rgba(5, 10).expect("pixel at 5,10");
    assert!(
        g > r,
        "pixel at (5,10) should be green-dominant (inside clip)"
    );

    // Outside the clip: still black.
    let (r2, g2, b2, a2) = fb.get_rgba(15, 10).expect("pixel at 15,10");
    assert_eq!(
        (r2, g2, b2, a2),
        (0, 0, 0, 255),
        "pixel at (15,10) must be unchanged background (outside clip)"
    );
}

// ── FillPath / clip-leak guard ────────────────────────────────────────────────

#[test]
fn fill_path_respects_active_clip() {
    let w = 100u32;
    let h = 100u32;
    let bg = Color(255, 255, 255, 255);
    let fill = Color(0, 0, 0, 255);
    let mut backend = SoftBackend::with_background(w, h, bg);

    let mut list = DrawList::new();
    // Clip to the left half only.
    list.push_clip(Rect::new(0.0, 0.0, 50.0, 100.0));
    // Full-frame triangle.
    let mut path = PathData::new();
    path.move_to(Point::new(0.0, 0.0));
    path.line_to(Point::new(100.0, 0.0));
    path.line_to(Point::new(100.0, 100.0));
    path.close();
    list.push_path(path, fill);
    list.pop_clip();
    backend.execute(&list).expect("execute");

    // Pixels at x=75 (right half, outside clip) must be unchanged.
    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(75, 50).expect("pixel at 75,50");
    assert_eq!(
        (r, g, b, a),
        (255, 255, 255, 255),
        "clip leak: path painted outside clip rect at (75,50)"
    );
}

// ── StrokePath / clip-leak guard ─────────────────────────────────────────────

#[test]
fn stroke_path_respects_active_clip() {
    let w = 100u32;
    let h = 100u32;
    let bg = Color(255, 255, 255, 255);
    let fill = Color(0, 0, 0, 255);
    let mut backend = SoftBackend::with_background(w, h, bg);

    let mut list = DrawList::new();
    list.push_clip(Rect::new(0.0, 0.0, 50.0, 100.0));
    let mut path = PathData::new();
    path.move_to(Point::new(0.0, 50.0));
    path.line_to(Point::new(100.0, 50.0));
    let style = oxiui_core::paint::StrokeStyle {
        width: 10.0,
        ..Default::default()
    };
    list.push_stroke_path(path, style, fill);
    list.pop_clip();
    backend.execute(&list).expect("execute");

    // Right half must be untouched.
    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(75, 50).expect("pixel at 75,50");
    assert_eq!(
        (r, g, b, a),
        (255, 255, 255, 255),
        "clip leak: stroke painted outside clip at (75,50)"
    );
}

// ── LinearGradient / clip-leak guard ─────────────────────────────────────────

#[test]
fn gradient_respects_active_clip() {
    let w = 100u32;
    let h = 100u32;
    let bg = Color(255, 255, 255, 255);
    let mut backend = SoftBackend::with_background(w, h, bg);

    let mut list = DrawList::new();
    // Clip to the left half.
    list.push_clip(Rect::new(0.0, 0.0, 50.0, 100.0));
    list.push_gradient_linear(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Point::new(0.0, 50.0),
        Point::new(100.0, 50.0),
        vec![
            GradientStop::new(0.0, Color(0, 0, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 0, 255)),
        ],
    );
    list.pop_clip();
    backend.execute(&list).expect("execute");

    // Pixels in the right half must be unchanged (white background).
    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(75, 50).expect("pixel at 75,50");
    assert_eq!(
        (r, g, b, a),
        (255, 255, 255, 255),
        "clip leak: gradient painted outside clip at (75,50)"
    );
}

// ── RadialGradient / clip-leak guard ─────────────────────────────────────────

#[test]
fn radial_gradient_respects_active_clip() {
    let w = 100u32;
    let h = 100u32;
    let bg = Color(255, 255, 255, 255);
    let mut backend = SoftBackend::with_background(w, h, bg);

    let mut list = DrawList::new();
    list.push_clip(Rect::new(0.0, 0.0, 50.0, 100.0));
    list.push_gradient_radial(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Point::new(50.0, 50.0),
        60.0,
        vec![
            GradientStop::new(0.0, Color(0, 0, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 0, 255)),
        ],
    );
    list.pop_clip();
    backend.execute(&list).expect("execute");

    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(75, 50).expect("pixel at 75,50");
    assert_eq!(
        (r, g, b, a),
        (255, 255, 255, 255),
        "clip leak: radial gradient painted outside clip at (75,50)"
    );
}

// ── Rounded rect ──────────────────────────────────────────────────────────────

#[test]
fn softbackend_rounded_rect_centre_filled() {
    let bg = Color(255, 255, 255, 255);
    let fill = Color(0, 0, 0, 255);
    let mut backend = SoftBackend::with_background(100, 100, bg);
    let mut list = DrawList::new();
    list.push_rounded_rect(Rect::new(10.0, 10.0, 80.0, 80.0), 10.0, fill);
    backend.execute(&list).expect("execute");

    // Centre pixel should be fully covered (black).
    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(50, 50).expect("centre pixel");
    assert_eq!(
        (r, g, b, a),
        (0, 0, 0, 255),
        "centre of rounded rect must be solid black"
    );
}

// ── Multiple commands in sequence ─────────────────────────────────────────────

#[test]
fn softbackend_sequential_commands() {
    let bg = Color(0, 0, 0, 255);
    let red = Color(255, 0, 0, 255);
    let blue = Color(0, 0, 255, 255);
    let mut backend = SoftBackend::with_background(20, 20, bg);

    let mut list = DrawList::new();
    // Red rect on the left.
    list.push_rect(Rect::new(0.0, 0.0, 10.0, 20.0), red);
    // Blue rect on the right.
    list.push_rect(Rect::new(10.0, 0.0, 10.0, 20.0), blue);
    backend.execute(&list).expect("execute");

    let fb = backend.frame();
    let (r, _, _, _) = fb.get_rgba(5, 10).expect("left");
    assert!(r > 0, "left half should be reddish");
    let (_, _, b, _) = fb.get_rgba(15, 10).expect("right");
    assert!(b > 0, "right half should be bluish");
}

// ── BoxShadow ─────────────────────────────────────────────────────────────────

#[test]
fn softbackend_box_shadow_deposits_pixels() {
    let bg = Color(0, 0, 0, 0); // transparent background so shadow is visible
    let shadow_color = Color(200, 200, 200, 255);
    let mut backend = SoftBackend::with_background(60, 60, bg);

    let mut list = DrawList::new();
    list.push_shadow(
        Rect::new(10.0, 10.0, 20.0, 20.0),
        Point::new(5.0, 5.0),
        3.0,
        shadow_color,
    );
    backend.execute(&list).expect("execute");

    // At least some pixel near the shadow region should be non-transparent.
    let fb = backend.frame();
    let mut found_shadow = false;
    for y in 10..40u32 {
        for x in 10..40u32 {
            if let Some((_, _, _, a)) = fb.get_rgba(x, y) {
                if a > 0 {
                    found_shadow = true;
                    break;
                }
            }
        }
        if found_shadow {
            break;
        }
    }
    assert!(
        found_shadow,
        "box shadow should deposit at least one non-transparent pixel"
    );
}

// ── into_framebuffer ──────────────────────────────────────────────────────────

#[test]
fn softbackend_into_framebuffer_preserves_pixels() {
    let bg = Color(42, 43, 44, 255);
    let mut backend = SoftBackend::with_background(4, 4, bg);
    let mut list = DrawList::new();
    list.push_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color(1, 2, 3, 255));
    backend.execute(&list).expect("execute");
    let fb = backend.into_framebuffer();
    let (r, g, b, _) = fb.get_rgba(2, 2).expect("pixel");
    assert_eq!(
        (r, g, b),
        (1, 2, 3),
        "into_framebuffer must preserve rendered pixels"
    );
}
