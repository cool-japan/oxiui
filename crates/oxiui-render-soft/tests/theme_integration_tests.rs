//! Integration tests wiring [`ShadowSpec`] from `oxiui-theme` into the
//! `oxiui-render-soft` shadow rendering path.
//!
//! These tests are compiled only when the `theme` feature is active.

#![cfg(feature = "theme")]

use oxiui_render_soft::{Framebuffer, ShadowSpec, SoftBackend};

// ---------------------------------------------------------------------------
// ShadowSpec constructor tests (re-exported through render-soft)
// ---------------------------------------------------------------------------

/// `ShadowSpec::new` must store offset_x, offset_y, blur, and colour correctly.
#[test]
fn test_shadow_spec_new_fields() {
    let spec = ShadowSpec::new(3.0, 4.0, 5.0, [10, 20, 30, 200]);
    assert!((spec.offset_x - 3.0).abs() < f32::EPSILON);
    assert!((spec.offset_y - 4.0).abs() < f32::EPSILON);
    assert!((spec.blur - 5.0).abs() < f32::EPSILON);
    // Colour channels stored correctly.
    assert_eq!(spec.color.0, 10);
    assert_eq!(spec.color.1, 20);
    assert_eq!(spec.color.2, 30);
    assert_eq!(spec.color.3, 200);
}

/// `ShadowSpec::drop_shadow` must produce a non-invisible drop shadow.
#[test]
fn test_shadow_spec_drop_shadow() {
    let spec = ShadowSpec::drop_shadow(2.0, 3.0, 6.0);
    assert!((spec.offset_x - 2.0).abs() < f32::EPSILON);
    assert!((spec.offset_y - 3.0).abs() < f32::EPSILON);
    assert!((spec.blur - 6.0).abs() < f32::EPSILON);
    // Default colour is semi-transparent black — must not be invisible.
    assert!(!spec.is_invisible(), "drop_shadow must have non-zero alpha");
    // inset defaults to false.
    assert!(!spec.inset);
}

/// `with_inset(true)` must set the `inset` flag.
#[test]
fn test_shadow_spec_inset_flag() {
    let spec = ShadowSpec::drop_shadow(0.0, 2.0, 4.0).with_inset(true);
    assert!(spec.inset, "inset flag must be true after with_inset(true)");
}

/// `with_spread` must update the spread field.
#[test]
fn test_shadow_spec_with_spread() {
    let spec = ShadowSpec::drop_shadow(0.0, 0.0, 2.0).with_spread(5.0);
    assert!((spec.spread - 5.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// elevation_to_shadow wired through render-soft
// ---------------------------------------------------------------------------

/// elevation_to_shadow(0) must produce a zero-blur (invisible) shadow.
#[test]
fn test_elevation_to_shadow_zero() {
    use oxiui_theme::elevation_to_shadow;
    let s = elevation_to_shadow(0.0);
    assert!(s.is_invisible(), "elevation 0 must be invisible");
}

/// elevation_to_shadow(4.0) must produce blur > 0.
#[test]
fn test_elevation_to_shadow_positive() {
    use oxiui_theme::elevation_to_shadow;
    let s = elevation_to_shadow(4.0);
    assert!(
        s.blur > 0.0,
        "elevation 4 must produce blur > 0, got {}",
        s.blur
    );
}

// ---------------------------------------------------------------------------
// apply_shadow_spec paints pixels
// ---------------------------------------------------------------------------

/// `SoftBackend::apply_shadow_spec` must deposit non-transparent pixels in the
/// shadow region of a 100×100 framebuffer.
#[test]
fn test_apply_shadow_spec_deposits_pixels() {
    let mut backend = SoftBackend::new(100, 100);

    let spec = ShadowSpec::new(0.0, 0.0, 3.0, [0, 0, 0, 200]);
    backend.apply_shadow_spec((20.0, 20.0, 40.0, 40.0), &spec);

    // At least one pixel inside the shadow region must be non-zero.
    let fb: &Framebuffer = backend.frame();
    let mut found_shadow = false;
    // Check the interior of the shadow bounding box (blur extends ±3 px outside).
    for y in 17u32..63 {
        for x in 17u32..63 {
            if let Some((_r, _g, _b, a)) = fb.get_rgba(x, y) {
                if a > 0 {
                    found_shadow = true;
                }
            }
        }
    }
    assert!(
        found_shadow,
        "apply_shadow_spec must deposit at least one non-transparent pixel"
    );
}

/// `apply_shadow_spec` with a spread radius inflates the shadow beyond the rect.
#[test]
fn test_apply_shadow_spec_spread_inflates() {
    // Shadow at rect (50,50,10,10) with spread 5 should reach pixel (44, 44).
    let mut backend_spread = SoftBackend::new(100, 100);
    let spec_spread = ShadowSpec::new(0.0, 0.0, 0.0, [0, 0, 0, 255]).with_spread(5.0);
    backend_spread.apply_shadow_spec((50.0, 50.0, 10.0, 10.0), &spec_spread);

    // With 5px spread the shadow rect becomes (45,45,20,20).  Pixel at (47,47) should be lit.
    let has_pixel = backend_spread
        .frame()
        .get_rgba(47, 47)
        .map(|(_, _, _, a)| a > 0)
        .unwrap_or(false);
    assert!(has_pixel, "spread shadow must reach inflated region");
}
