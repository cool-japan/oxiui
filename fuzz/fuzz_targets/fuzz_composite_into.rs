#![no_main]
//! Fuzz `composite_into` with arbitrary `(w, h, dst_x, dst_y, src.len())`.
//!
//! Regression coverage for the Opus-verified backlog bug where the required
//! source-length guard was computed as `w * h * 4` in `u32`, which wraps for
//! large `w`/`h` (e.g. `w = h = 65_536` wraps to exactly `0`) and would defeat
//! the `src.len()` bounds check, letting the pixel-copy loop index past the
//! end of `src`. The fix does the guard arithmetic in checked `usize` (see
//! `blend.rs`'s `composite_into_near_u32_boundary_does_not_overflow_or_panic`
//! / `composite_into_exact_u32_boundary_dims_no_panic` unit tests for the
//! hand-picked regression cases this target generalizes).
use libfuzzer_sys::fuzz_target;
use oxiui_render_soft::{composite_into, BlendMode, Framebuffer};

/// Byte layout consumed from the front of `data`: `w:u32, h:u32, dst_x:i32,
/// dst_y:i32, mode:u8` (17 bytes). Everything after that is used verbatim as
/// `src`, so `src.len()` varies freely with the fuzzer-supplied input length
/// — exactly the parameter the backlog bug turned on.
const HEADER_LEN: usize = 17;

fuzz_target!(|data: &[u8]| {
    if data.len() < HEADER_LEN {
        return;
    }
    let w = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let h = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let dst_x = i64::from(i32::from_le_bytes(data[8..12].try_into().unwrap()));
    let dst_y = i64::from(i32::from_le_bytes(data[12..16].try_into().unwrap()));
    let mode = match data[16] % 6 {
        0 => BlendMode::Normal,
        1 => BlendMode::Multiply,
        2 => BlendMode::Screen,
        3 => BlendMode::Overlay,
        4 => BlendMode::Darken,
        _ => BlendMode::Lighten,
    };
    let src = &data[HEADER_LEN..];

    // Small fixed destination framebuffer; `w`/`h`/`dst_x`/`dst_y` are the
    // fuzzer-controlled (potentially huge or negative) placement parameters.
    let mut fb = Framebuffer::new(16, 16);
    let _ = composite_into(&mut fb, src, w, h, dst_x, dst_y, mode);
});
