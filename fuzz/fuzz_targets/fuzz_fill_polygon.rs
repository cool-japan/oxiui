#![no_main]
//! Fuzz `fill_polygon` / `fill_polygon_clipped` with arbitrary finite f32
//! vertex coordinates.
//!
//! Regression coverage for the Opus-verified backlog bug where a polygon with
//! extreme (but finite) vertex coordinates drove the scanline AET fill loop
//! across the vertex extent instead of the framebuffer's actual bounds —
//! billions of empty iterations from one attacker-controlled coordinate. The
//! fix clamps the vertical/horizontal span to the framebuffer before
//! iterating (see `scanline.rs`'s `fill_polygon_far_out_of_bounds_is_clamped_and_fast`
//! unit test for the hand-picked regression case this target generalizes).
//!
//! CONFIRMED LIVE BUG (found by this target within seconds of fuzzing, not
//! yet fixed as of the wave that added this harness): the fast-forward loop
//! that the fix above added has its own overflow. At `scanline.rs:259`,
//! `e.x += e.dx * (y_start - edge_y_start) as f32;` subtracts two `i32`s —
//! `y_start` is clamped to `[0, framebuffer_height]`, but `edge_y_start`
//! comes from an *unclamped* `as i32` cast on a raw vertex Y and can be
//! `i32::MIN` for an extreme-but-finite coordinate, so `y_start -
//! edge_y_start` panics with `attempt to subtract with overflow`. Reproduced
//! by the checked-in (not gitignored) regression seed
//! `fuzz/regressions/fuzz_fill_polygon/crash-75bd14507aaf06979c7bdbc7479e667dda9c14ef`
//! — decoded to a 3-point triangle with one point's `y` at `-1.70e38` in
//! `fuzz/regressions/README.md`. Left unfixed intentionally: this crate is
//! out of this wave's assigned scope; a future wave should use
//! `saturating_sub`/checked arithmetic there, matching the checked-arithmetic
//! style `composite_into` already uses for the sibling `blend.rs` bug.
use libfuzzer_sys::fuzz_target;
use oxiui_core::Color;
use oxiui_render_soft::{
    fill_polygon, scanline::fill_polygon_clipped, ClipRect, FillRule, Framebuffer,
};

/// Decode `data` into a bounded list of finite `(f32, f32)` points.
///
/// Non-finite (NaN/Infinity) coordinates are dropped rather than passed
/// through, so this target's *initial* points are always finite — see the
/// module doc for why "finite in" does not (yet) imply "no crash": the
/// fast-forward loop this target found broken doesn't even need a NaN/
/// Infinity input to panic.
fn decode_points(data: &[u8]) -> Vec<(f32, f32)> {
    data.chunks_exact(8)
        .take(64)
        .filter_map(|c| {
            let x = f32::from_le_bytes(c[0..4].try_into().ok()?);
            let y = f32::from_le_bytes(c[4..8].try_into().ok()?);
            (x.is_finite() && y.is_finite()).then_some((x, y))
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let points = decode_points(data);
    if points.len() < 3 {
        return;
    }

    // Both fill rules, both AA settings — each has its own span-fill path.
    let mut fb = Framebuffer::new(16, 16);
    fill_polygon(
        &mut fb,
        &points,
        Color(255, 0, 0, 255),
        FillRule::NonZero,
        false,
    );

    let mut fb_aa = Framebuffer::new(16, 16);
    fill_polygon(
        &mut fb_aa,
        &points,
        Color(0, 255, 0, 255),
        FillRule::EvenOdd,
        true,
    );

    // The clip-aware variant has its own (previously separately unclamped)
    // vertical loop — exercise it too.
    let mut fb_clipped = Framebuffer::new(16, 16);
    let clip = ClipRect::full(16, 16);
    fill_polygon_clipped(
        &mut fb_clipped,
        &points,
        Color(0, 0, 255, 255),
        FillRule::NonZero,
        true,
        clip,
    );
});
