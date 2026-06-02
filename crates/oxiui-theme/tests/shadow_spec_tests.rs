//! Tests for [`oxiui_theme::ShadowSpec`] constructors, builder methods,
//! pixel-color encoding, and the `elevation_to_shadow` helper.

use oxiui_theme::{elevation_to_shadow, ShadowSpec};

// ---------------------------------------------------------------------------
// to_pixel_color
// ---------------------------------------------------------------------------

/// RGBA [255, 0, 0, 128] must encode as 0x80FF0000.
#[test]
fn test_shadow_spec_to_pixel_color() {
    let spec = ShadowSpec::new(0.0, 0.0, 0.0, [255, 0, 0, 128]);
    // 0xAARRGGBB: alpha=128=0x80, red=255=0xFF, green=0, blue=0.
    assert_eq!(spec.to_pixel_color(), 0x80FF_0000);
}

/// Fully transparent black encodes to 0x00000000.
#[test]
fn test_shadow_spec_to_pixel_color_transparent() {
    let spec = ShadowSpec::new(0.0, 0.0, 0.0, [0, 0, 0, 0]);
    assert_eq!(spec.to_pixel_color(), 0x0000_0000);
}

/// Fully opaque white encodes to 0xFFFFFFFF.
#[test]
fn test_shadow_spec_to_pixel_color_white() {
    let spec = ShadowSpec::new(0.0, 0.0, 0.0, [255, 255, 255, 255]);
    assert_eq!(spec.to_pixel_color(), 0xFFFF_FFFF);
}

// ---------------------------------------------------------------------------
// elevation_to_shadow
// ---------------------------------------------------------------------------

/// Elevation 8.0 must have a larger blur than elevation 2.0.
#[test]
fn test_elevation_to_shadow_scale() {
    let s2 = elevation_to_shadow(2.0);
    let s8 = elevation_to_shadow(8.0);
    assert!(
        s8.blur > s2.blur,
        "elevation 8 blur ({}) must exceed elevation 2 blur ({})",
        s8.blur,
        s2.blur,
    );
}

/// Elevation 0 must produce a fully transparent (invisible) shadow.
#[test]
fn test_elevation_to_shadow_zero_is_invisible() {
    let s = elevation_to_shadow(0.0);
    assert!(
        s.is_invisible(),
        "elevation 0 shadow must be invisible (alpha=0)"
    );
}

/// Negative elevation is clamped and treated like zero.
#[test]
fn test_elevation_to_shadow_negative_clamped() {
    let s = elevation_to_shadow(-5.0);
    assert!(
        s.is_invisible(),
        "negative elevation must yield invisible shadow"
    );
}
