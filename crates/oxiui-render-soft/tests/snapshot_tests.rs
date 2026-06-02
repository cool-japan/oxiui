//! Deterministic pixel-value snapshot tests for oxiui-render-soft.
//!
//! Each test renders a reference scene using `SoftBackend` or direct
//! `Framebuffer` + `Canvas` APIs, then asserts specific pixel values at
//! known coordinates. No golden files — expected values are embedded here.
//!
//! **Design notes:**
//! - All expected values were derived from the algorithms in `gradient.rs`,
//!   `framebuffer.rs`, `dither.rs`, etc. and confirmed by running the tests.
//! - Tolerance ±1 per channel is used where floating-point rounding can shift
//!   the LSB. Exact matches are used where the implementation is deterministic
//!   to the bit.
//! - Text rendering is deliberately avoided (font rasterisation is not
//!   deterministic across platforms/features).

use oxiui_core::geometry::{Point, Rect};
use oxiui_core::paint::{GradientStop, PathData, RenderBackend};
use oxiui_core::{Color, DrawList};
use oxiui_render_soft::{
    ordered_dither_rgba, BayerMatrix, ClipRect, Framebuffer, LinearGradient, SoftBackend,
};

// ---------------------------------------------------------------------------
// Helper: assert two u8 values are within `tol` of each other.
// ---------------------------------------------------------------------------

fn near(actual: u8, expected: u8, tol: u8, label: &str) {
    let diff = (actual as i32 - expected as i32).unsigned_abs() as u8;
    assert!(
        diff <= tol,
        "{label}: expected {expected} ± {tol}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// Test 1: Linear gradient scene — red→blue, horizontal, 100×100.
//
// The gradient axis goes from (0,50) to (100,50), sampling at pixel centre.
// At pixel (50,50): t = (50.5 - 0) / 100 = 0.505
//   r = round(255 - 255*0.505) = round(127.225) = 127
//   g = 0
//   b = round(255*0.505)       = round(128.775) = 129
// ---------------------------------------------------------------------------

#[test]
fn snapshot_gradient_midpoint_is_purple() {
    let mut backend = SoftBackend::with_background(100, 100, Color(0, 0, 0, 255));
    let mut list = DrawList::new();
    list.push_gradient_linear(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Point::new(0.0, 50.0),
        Point::new(100.0, 50.0),
        vec![
            GradientStop::new(0.0, Color(255, 0, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 255, 255)),
        ],
    );
    backend.execute(&list).expect("execute");

    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(50, 50).expect("pixel (50,50)");

    // Midpoint should be purple: high R and B, zero G, fully opaque.
    assert_eq!(a, 255, "gradient midpoint must be opaque");
    assert_eq!(g, 0, "gradient midpoint g must be 0 (no green)");
    near(r, 127, 2, "gradient midpoint red");
    near(b, 129, 2, "gradient midpoint blue");
    // Structural check: both channels in the purple range.
    assert!(
        r >= 100 && b >= 100,
        "midpoint must be purple (r={r} b={b})"
    );

    // Left edge pixel (0,50): should be nearly pure red.
    let (r0, g0, b0, _) = fb.get_rgba(0, 50).expect("pixel (0,50)");
    assert!(r0 > 200, "left edge should be nearly red, got r={r0}");
    assert!(b0 < 55, "left edge should not be very blue, got b={b0}");
    assert_eq!(g0, 0, "no green at left edge");

    // Right edge pixel (99,50): should be nearly pure blue.
    let (r99, g99, b99, _) = fb.get_rgba(99, 50).expect("pixel (99,50)");
    assert!(b99 > 200, "right edge should be nearly blue, got b={b99}");
    assert!(r99 < 55, "right edge should not be very red, got r={r99}");
    assert_eq!(g99, 0, "no green at right edge");
}

// ---------------------------------------------------------------------------
// Test 2: Layered alpha blend — red rect, then semi-transparent blue rect.
//
// Background: white (255,255,255,255)
// Layer 1: red (255,0,0,255) fills entire 50×50 canvas.
// Layer 2: blue (0,0,255,128) over the same region.
//
// Source-over: a_src=128/255≈0.502, a_dst=1.0, a_out=1.0
//   r = round( (0*0.502 + 255*(1-0.502)) / 1.0 ) = round(127.0) = 127
//   b = round( (255*0.502 + 0*(1-0.502)) / 1.0 ) = round(128.0) = 128
//   g = 0
// ---------------------------------------------------------------------------

#[test]
fn snapshot_layered_alpha_blend() {
    let mut backend = SoftBackend::with_background(50, 50, Color(0, 0, 0, 255));
    let mut list = DrawList::new();
    // Full opaque red.
    list.push_rect(Rect::new(0.0, 0.0, 50.0, 50.0), Color(255, 0, 0, 255));
    // Semi-transparent blue on top.
    list.push_rect(Rect::new(0.0, 0.0, 50.0, 50.0), Color(0, 0, 255, 128));
    backend.execute(&list).expect("execute");

    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(25, 25).expect("pixel (25,25)");

    assert_eq!(a, 255, "alpha must be fully opaque after blending");
    assert_eq!(g, 0, "no green in red-blue blend");
    // Red should be around 127, blue around 128.
    near(r, 127, 2, "blended red channel");
    near(b, 128, 2, "blended blue channel");
    // Structural: equal weight blend should have r and b within 4 of each other.
    let diff = (r as i32 - b as i32).unsigned_abs();
    assert!(
        diff <= 4,
        "r and b should be near-equal in 50/50 blend (r={r} b={b})"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Rounded rect scene — 80×80 rounded rect inside 100×100 canvas.
//
// Rect at (10,10) with size (80,80) and corner radius 15.
// - Corner pixel (10,10): sample centre (10.5,10.5), dx = 15-10.5 = 4.5,
//   dy = 15-10.5 = 4.5, dist = sqrt(4.5²+4.5²) ≈ 6.36, coverage = (15-6.36+0.5) ≈ 9.14 → 1.0
//   Actually (10,10) is inside the rect in x,y range but at the corner arc.
//   Let's check extreme corner (0,0): clearly outside.
// - Centre pixel (50,50): well inside — coverage = 1.0, black.
// - Extreme corner (0,0): outside rect entirely — coverage = 0, stays white.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_rounded_rect_corners_vs_center() {
    let bg = Color(255, 255, 255, 255);
    let fill = Color(0, 0, 0, 255);
    let mut backend = SoftBackend::with_background(100, 100, bg);
    let mut list = DrawList::new();
    // 80×80 rounded rect at (10,10) with radius 15.
    list.push_rounded_rect(Rect::new(10.0, 10.0, 80.0, 80.0), 15.0, fill);
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // Centre must be solid black.
    let (r, g, b, a) = fb.get_rgba(50, 50).expect("centre pixel");
    assert_eq!(
        (r, g, b, a),
        (0, 0, 0, 255),
        "centre of rounded rect must be solid black"
    );

    // Canvas corners (well outside the rounded rect) must be white background.
    for &(cx, cy) in &[(0u32, 0u32), (99, 0), (0, 99), (99, 99)] {
        let (r2, g2, b2, a2) = fb.get_rgba(cx, cy).expect("corner pixel");
        assert_eq!(
            (r2, g2, b2, a2),
            (255, 255, 255, 255),
            "canvas corner ({cx},{cy}) should remain white background"
        );
    }

    // Pixels just inside the rect edges (not in corner arcs) must be filled.
    // x=50, y=11: straight top edge — inside the rect, outside any corner arc.
    let (r3, g3, b3, _a3) = fb.get_rgba(50, 11).expect("top edge pixel");
    assert!(
        r3 < 128 && g3 < 128 && b3 < 128,
        "top-edge pixel inside rect must be dark"
    );

    // Pixels clearly outside the rect bounding box must be white.
    let (r4, g4, b4, a4) = fb.get_rgba(5, 5).expect("outside pixel");
    assert_eq!(
        (r4, g4, b4, a4),
        (255, 255, 255, 255),
        "pixel (5,5) should be white (outside rect)"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Clipped gradient — gradient within clip, outside untouched.
//
// 100×100 canvas, white background.
// Clip: [30, 70) × [0, 100) (left half of a 40-px stripe).
// Gradient: red→blue horizontal across full width.
// Pixels at x=15 (outside clip) must remain white.
// Pixels at x=50 (inside clip) must be purple (from gradient).
// ---------------------------------------------------------------------------

#[test]
fn snapshot_clipped_gradient_outside_untouched() {
    let bg = Color(255, 255, 255, 255);
    let mut backend = SoftBackend::with_background(100, 100, bg);
    let mut list = DrawList::new();
    list.push_clip(Rect::new(30.0, 0.0, 40.0, 100.0));
    list.push_gradient_linear(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Point::new(0.0, 50.0),
        Point::new(100.0, 50.0),
        vec![
            GradientStop::new(0.0, Color(255, 0, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 255, 255)),
        ],
    );
    list.pop_clip();
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // Outside clip (left side x=15): must be white.
    let (r, g, b, a) = fb.get_rgba(15, 50).expect("outside-clip pixel");
    assert_eq!(
        (r, g, b, a),
        (255, 255, 255, 255),
        "pixel (15,50) outside clip must remain white"
    );

    // Outside clip (right side x=80): must be white.
    let (r2, g2, b2, a2) = fb.get_rgba(80, 50).expect("outside-clip pixel right");
    assert_eq!(
        (r2, g2, b2, a2),
        (255, 255, 255, 255),
        "pixel (80,50) outside clip must remain white"
    );

    // Inside clip (x=50): gradient should be purple.
    let (ri, gi, bi, ai) = fb.get_rgba(50, 50).expect("inside-clip gradient pixel");
    assert_eq!(ai, 255, "inside-clip gradient pixel must be opaque");
    assert_eq!(gi, 0, "gradient midpoint has no green");
    assert!(
        ri > 50 && bi > 50,
        "inside-clip gradient midpoint must be purple (r={ri} b={bi})"
    );
}

// ---------------------------------------------------------------------------
// Test 5: Shadow scene — box shadow verifies shadow pixels exist.
//
// Transparent background so shadow alpha is detectable.
// Shadow at box (20,20) size (20,20) with offset (5,5) blur 5.
// Shadow pixels should appear around (25..50, 25..50).
// After rendering, at least one pixel in that region must be non-transparent.
// Also verify the fill rect over the shadow.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_shadow_pixels_deposited() {
    let bg = Color(0, 0, 0, 0); // transparent background
    let shadow_color = Color(80, 80, 80, 220);
    let mut backend = SoftBackend::with_background(80, 80, bg);
    let mut list = DrawList::new();
    list.push_shadow(
        Rect::new(20.0, 20.0, 20.0, 20.0),
        Point::new(5.0, 5.0),
        5.0,
        shadow_color,
    );
    // Paint the widget rectangle on top so shadow is behind it.
    list.push_rect(Rect::new(20.0, 20.0, 20.0, 20.0), Color(200, 200, 200, 255));
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // The box itself (25,25) must be filled with the grey rect.
    let (r, _g, _b, a) = fb.get_rgba(25, 25).expect("box pixel");
    assert_eq!(a, 255, "box pixel must be opaque");
    assert!(r > 150, "box pixel should be light grey (r={r})");

    // Pixels below and right of the box (offset region) should have shadow.
    // Shadow is at (20+5, 20+5) = (25,25) center, blurred. Check around (45,45).
    let mut found_shadow = false;
    'outer: for y in 28..65u32 {
        for x in 28..65u32 {
            if let Some((_, _, _, a)) = fb.get_rgba(x, y) {
                if a > 0 {
                    found_shadow = true;
                    break 'outer;
                }
            }
        }
    }
    assert!(
        found_shadow,
        "shadow region must have at least one non-transparent pixel"
    );

    // Pixel at (0,0) (far from any content) must remain transparent.
    let (_, _, _, a0) = fb.get_rgba(0, 0).expect("corner pixel");
    assert_eq!(
        a0, 0,
        "top-left corner must remain transparent (no shadow reach)"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Path fill scene — triangular path fill.
//
// 100×100 canvas, white background.
// Triangle: (10,90) → (50,10) → (90,90) → close.
// Interior pixel (50,70): should be filled black.
// Exterior pixel (5,5): should remain white.
// Exterior pixel (95,5): should remain white.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_path_fill_triangle() {
    let bg = Color(255, 255, 255, 255);
    let fill = Color(0, 0, 0, 255);
    let mut backend = SoftBackend::with_background(100, 100, bg);

    let mut list = DrawList::new();
    let mut path = PathData::new();
    path.move_to(Point::new(10.0, 90.0));
    path.line_to(Point::new(50.0, 10.0));
    path.line_to(Point::new(90.0, 90.0));
    path.close();
    list.push_path(path, fill);
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // Interior pixel (50,70) — well inside the triangle.
    let (r, g, b, a) = fb.get_rgba(50, 70).expect("interior pixel");
    assert_eq!(a, 255, "interior pixel must be opaque");
    // Should be dark (black fill composited over white bg with some coverage).
    assert!(
        r < 128 && g < 128 && b < 128,
        "interior pixel must be dark (r={r} g={g} b={b})"
    );

    // Exterior pixel well above and left of triangle.
    let (r2, g2, b2, a2) = fb.get_rgba(5, 5).expect("exterior pixel top-left");
    assert_eq!(
        (r2, g2, b2, a2),
        (255, 255, 255, 255),
        "exterior pixel (5,5) must remain white"
    );

    // Exterior pixel below triangle apex but outside shape (top-right area).
    let (r3, g3, b3, a3) = fb.get_rgba(95, 5).expect("exterior pixel top-right");
    assert_eq!(
        (r3, g3, b3, a3),
        (255, 255, 255, 255),
        "exterior pixel (95,5) must remain white"
    );
}

// ---------------------------------------------------------------------------
// Test 7: Dithered scene — Bayer-dithered grey rect, verify pattern.
//
// Direct Framebuffer + ordered_dither_rgba (no DrawList needed; this is the
// established pattern in soft_tests.rs and dither.rs unit tests).
//
// Input: 8×8 framebuffer filled with grey level 10, alpha 255.
// bits_to_drop = 4 → step = 16.
// For each pixel: q = floor((10 + thresh*16) / 16) * 16
//   thresh = matrix[y][x] / 64.0
//
// Pixel (0,0): matrix[0][0] = 0 → thresh=0.0 → 10+0=10 → floor(10/16)*16=0
// Pixel (1,0): matrix[0][1] = 32 → thresh=0.5 → 10+8=18 → floor(18/16)*16=16
// These values are deterministic per the Bayer matrix.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_dithered_pattern_matches_bayer() {
    let mut fb = Framebuffer::with_fill(8, 8, Color(10, 10, 10, 255));
    let clip = ClipRect::full(8, 8);
    ordered_dither_rgba(&mut fb, clip, 4);

    let matrix = BayerMatrix::standard_8x8();
    let input_level: u8 = 10;
    let step = 16u8; // 2^bits_to_drop = 2^4

    // Verify several specific pixels against the Bayer formula.
    let check_pixel = |x: u32, y: u32| {
        let thresh = matrix.threshold(x, y); // in [0,1)
        let v = input_level as f32 + thresh * (step as f32);
        let expected = ((v / step as f32).floor() * step as f32).clamp(0.0, 255.0) as u8;
        let (r, g, b, a) = fb.get_rgba(x, y).expect("pixel");
        assert_eq!(a, 255, "alpha must be unchanged at ({x},{y})");
        assert_eq!(
            r, expected,
            "R channel mismatch at ({x},{y}): expected {expected}, got {r}"
        );
        assert_eq!(
            g, expected,
            "G channel mismatch at ({x},{y}): expected {expected}, got {g}"
        );
        assert_eq!(
            b, expected,
            "B channel mismatch at ({x},{y}): expected {expected}, got {b}"
        );
    };

    // Sample known cells from the 8×8 Bayer matrix.
    check_pixel(0, 0); // matrix[0][0]=0  → thresh=0.000 → q=0
    check_pixel(1, 0); // matrix[0][1]=32 → thresh=0.500 → q=16
    check_pixel(2, 0); // matrix[0][2]=8  → thresh=0.125 → q=0
    check_pixel(3, 0); // matrix[0][3]=40 → thresh=0.625 → q=16
    check_pixel(0, 1); // matrix[1][0]=48 → thresh=0.750 → q=16
    check_pixel(4, 4); // matrix[4][4]=1  → thresh=0.016 → q=0
    check_pixel(7, 7); // matrix[7][7]=21 → thresh=0.328 → q=0
}

// ---------------------------------------------------------------------------
// Test 8: Multi-layer semi-transparent rects — accumulated blend.
//
// Background: black (0,0,0,255).
// Layer 1: red (255,0,0,128) — 50% opacity.
// Layer 2: green (0,255,0,64) — 25% opacity.
// Layer 3: blue (0,0,255,32) — 12.5% opacity.
//
// After layer 1: r=127, g=0, b=0, a=255 (approx).
// After layer 2 over that: src_a=64/255≈0.251
//   r = round((0*0.251 + 127*(1-0.251)) / 1.0) ≈ 95
//   g = round((255*0.251 + 0*0.749) / 1.0) ≈ 64
// After layer 3: src_a=32/255≈0.125
//   Final is approximately r≈83, g≈56, b≈8.
//
// Rather than exact values, assert monotonic contributions.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_multi_layer_blend_accumulates() {
    let mut backend = SoftBackend::with_background(40, 40, Color(0, 0, 0, 255));
    let mut list = DrawList::new();
    list.push_rect(Rect::new(0.0, 0.0, 40.0, 40.0), Color(255, 0, 0, 128));
    list.push_rect(Rect::new(0.0, 0.0, 40.0, 40.0), Color(0, 255, 0, 64));
    list.push_rect(Rect::new(0.0, 0.0, 40.0, 40.0), Color(0, 0, 255, 32));
    backend.execute(&list).expect("execute");

    let fb = backend.frame();
    let (r, g, b, a) = fb.get_rgba(20, 20).expect("centre pixel");

    // All layers together must be fully opaque (each composed over opaque bg).
    assert_eq!(a, 255, "multi-layer blend must be opaque (bg is opaque)");

    // Each colour channel should have a non-zero contribution.
    assert!(
        r > 50,
        "red contribution from layer 1 should be visible (r={r})"
    );
    assert!(
        g > 10,
        "green contribution from layer 2 should be visible (g={g})"
    );
    assert!(
        b > 0,
        "blue contribution from layer 3 should be visible (b={b})"
    );

    // Red dominates (highest alpha), green is secondary, blue is minor.
    assert!(r > g, "red should dominate over green (r={r} g={g})");
    assert!(g > b, "green should dominate over blue (g={g} b={b})");
}

// ---------------------------------------------------------------------------
// Test 9: Gradient vertical axis — verify top is red, bottom is blue.
//
// 100×100 canvas. Gradient axis: (50,0)→(50,100) (vertical).
// pixel (50,1): t ≈ 0.015 → nearly pure red.
// pixel (50,98): t ≈ 0.985 → nearly pure blue.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_gradient_vertical_top_red_bottom_blue() {
    let mut backend = SoftBackend::with_background(100, 100, Color(0, 0, 0, 255));
    let mut list = DrawList::new();
    list.push_gradient_linear(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Point::new(50.0, 0.0),
        Point::new(50.0, 100.0),
        vec![
            GradientStop::new(0.0, Color(255, 0, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 255, 255)),
        ],
    );
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // Near-top pixel (50,1): mostly red.
    let (r_top, g_top, b_top, a_top) = fb.get_rgba(50, 1).expect("top pixel");
    assert_eq!(a_top, 255);
    assert_eq!(g_top, 0);
    assert!(r_top > 200, "top pixel should be mostly red (r={r_top})");
    assert!(b_top < 50, "top pixel should have little blue (b={b_top})");

    // Near-bottom pixel (50,98): mostly blue.
    let (r_bot, g_bot, b_bot, a_bot) = fb.get_rgba(50, 98).expect("bottom pixel");
    assert_eq!(a_bot, 255);
    assert_eq!(g_bot, 0);
    assert!(
        b_bot > 200,
        "bottom pixel should be mostly blue (b={b_bot})"
    );
    assert!(
        r_bot < 50,
        "bottom pixel should have little red (r={r_bot})"
    );
}

// ---------------------------------------------------------------------------
// Test 10: Radial gradient — centre vs periphery.
//
// 100×100 canvas, black background.
// Radial gradient centred at (50,50), radius 40: white at centre, black at edge.
// pixel (50,50): t ≈ 0 → white.
// pixel (0,0): t > 1 → black (clamped).
// ---------------------------------------------------------------------------

#[test]
fn snapshot_radial_gradient_center_vs_edge() {
    use oxiui_core::paint::GradientStop as CoreStop;

    let mut backend = SoftBackend::with_background(100, 100, Color(0, 0, 0, 255));
    let mut list = DrawList::new();
    list.push_gradient_radial(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Point::new(50.0, 50.0),
        40.0,
        vec![
            CoreStop::new(0.0, Color(255, 255, 255, 255)),
            CoreStop::new(1.0, Color(0, 0, 0, 255)),
        ],
    );
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // Centre (50,50): should be white (or very close).
    let (r, g, b, a) = fb.get_rgba(50, 50).expect("centre pixel");
    assert_eq!(a, 255);
    assert!(
        r > 200 && g > 200 && b > 200,
        "radial gradient centre must be bright (r={r} g={g} b={b})"
    );

    // Far corner (0,0): outside radius, must be black (background unchanged or clamped gradient).
    let (r2, g2, b2, a2) = fb.get_rgba(0, 0).expect("corner pixel");
    assert_eq!(a2, 255);
    assert!(
        r2 < 30 && g2 < 30 && b2 < 30,
        "radial gradient periphery must be dark (r={r2} g={g2} b={b2})"
    );
}

// ---------------------------------------------------------------------------
// Test 11: Clip stack with nested rects — draw respects innermost clip.
//
// 100×100 canvas, black background.
// Outer clip: [20, 80) × [20, 80).
// Inner clip: [40, 60) × [40, 60).
// Fill entire canvas with white.
// Expected: only [40,60)×[40,60) is white; everything else is black.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_nested_clip_respects_inner() {
    let mut backend = SoftBackend::with_background(100, 100, Color(0, 0, 0, 255));
    let mut list = DrawList::new();
    list.push_clip(Rect::new(20.0, 20.0, 60.0, 60.0));
    list.push_clip(Rect::new(40.0, 40.0, 20.0, 20.0));
    list.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0), Color(255, 255, 255, 255));
    list.pop_clip();
    list.pop_clip();
    backend.execute(&list).expect("execute");

    let fb = backend.frame();

    // Inside inner clip (50,50): white.
    let (r, g, b, a) = fb.get_rgba(50, 50).expect("inner pixel");
    assert_eq!(
        (r, g, b, a),
        (255, 255, 255, 255),
        "inner clip pixel must be white"
    );

    // Between clips (30,30): black (outside inner clip).
    let (r2, g2, b2, a2) = fb.get_rgba(30, 30).expect("between-clips pixel");
    assert_eq!(
        (r2, g2, b2, a2),
        (0, 0, 0, 255),
        "between-clips pixel must be black"
    );

    // Outside outer clip (10,10): black.
    let (r3, g3, b3, a3) = fb.get_rgba(10, 10).expect("outside-clip pixel");
    assert_eq!(
        (r3, g3, b3, a3),
        (0, 0, 0, 255),
        "outside-clip pixel must be black"
    );
}

// ---------------------------------------------------------------------------
// Test 12: Direct LinearGradient API on Framebuffer — two-stop, 8px wide.
//
// Uses the low-level `LinearGradient::fill_rect` directly (pattern from
// `soft_tests.rs`), bypassing DrawList/SoftBackend.
// 8×1 canvas, red→blue horizontal gradient.
// Pixel 0: t=0.0625 → mostly red.
// Pixel 7: t=0.9375 → mostly blue.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_direct_linear_gradient_framebuffer() {
    let mut fb = Framebuffer::with_fill(8, 1, Color(0, 0, 0, 255));
    let clip = ClipRect::full(8, 1);
    let g = LinearGradient::two_stop(
        (0.0, 0.0),
        (8.0, 0.0),
        Color(255, 0, 0, 255),
        Color(0, 0, 255, 255),
    );
    g.fill_rect(&mut fb, &clip, 0.0, 0.0, 8.0, 1.0);

    // Pixel at x=0 (centre 0.5, t=0.5/8=0.0625): r≈253, b≈2.
    let (r0, g0, b0, a0) = fb.get_rgba(0, 0).expect("pixel 0");
    assert_eq!(a0, 255);
    assert_eq!(g0, 0);
    assert!(r0 > 200, "pixel 0 should be mostly red (r={r0})");
    assert!(b0 < 50, "pixel 0 should have little blue (b={b0})");

    // Pixel at x=7 (centre 7.5, t=7.5/8=0.9375): r≈16, b≈239.
    let (r7, g7, b7, a7) = fb.get_rgba(7, 0).expect("pixel 7");
    assert_eq!(a7, 255);
    assert_eq!(g7, 0);
    assert!(b7 > 200, "pixel 7 should be mostly blue (b={b7})");
    assert!(r7 < 50, "pixel 7 should have little red (r={r7})");

    // Monotonic: each successive pixel should have decreasing red and increasing blue.
    let mut prev_r = r0;
    let mut prev_b = b0;
    for x in 1..8u32 {
        let (rx, _gx, bx, _ax) = fb.get_rgba(x, 0).expect("pixel");
        assert!(
            rx <= prev_r + 2,
            "red should decrease left→right at x={x} (prev={prev_r} cur={rx})"
        );
        assert!(
            bx >= prev_b.saturating_sub(2),
            "blue should increase left→right at x={x} (prev={prev_b} cur={bx})"
        );
        prev_r = rx;
        prev_b = bx;
    }
}
