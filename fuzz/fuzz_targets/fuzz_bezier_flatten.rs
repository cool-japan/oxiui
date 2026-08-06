#![no_main]
//! Fuzz `path.rs`'s adaptive bezier flattening (`flatten_quad_bezier` /
//! `flatten_cubic_bezier`) with arbitrary finite f32 control points and
//! tolerance.
//!
//! Both functions recurse via De Casteljau subdivision until a
//! chord-deviation comparison falls below `tolerance` (clamped to a `0.01`
//! floor). Only the *initial* control points are filtered to finite values
//! here (NaN/Infinity inputs are a separate, already-known non-termination:
//! a NaN chord-deviation comparison is always `false`, so the base case never
//! triggers).
//!
//! CONFIRMED LIVE BUG (found by this target within seconds of fuzzing, not
//! yet fixed as of the wave that added this harness): finite inputs are
//! *not* actually safe. `path.rs`'s private `mid(a, b)` helper computes
//! `(a.0 + b.0) * 0.5`; when both coordinates are large and same-signed,
//! `a.0 + b.0` can itself overflow `f32` to `Infinity` *before* the halving,
//! manufacturing a non-finite midpoint from finite inputs. That Infinity then
//! propagates into the chord-deviation comparison exactly as the NaN case
//! above does, and recursion never terminates — `AddressSanitizer:
//! stack-overflow` in `flatten_quad`, reproduced by the checked-in (not
//! gitignored) regression seed
//! `fuzz/regressions/fuzz_bezier_flatten/crash-bc9e57a5522d24fc0508b9f3935bd9ab75686094`
//! — decoded to `p0 = (0.0, -2.35e38)` (plus `p1`, `p2`, `tolerance`) in
//! `fuzz/regressions/README.md`. Left unfixed and the target left unbounded
//! intentionally: this crate is out of this wave's assigned scope; a future
//! wave should saturate/clamp `mid()` or cap recursion depth.
use libfuzzer_sys::fuzz_target;
use oxiui_render_soft::path::{flatten_cubic_bezier, flatten_quad_bezier};

/// Read `N` consecutive finite `f32`s (4-byte little-endian) from the front
/// of `data`. Returns `None` if there are not enough bytes or any value is
/// NaN/Infinity.
fn read_finite_f32s<const N: usize>(data: &[u8]) -> Option<[f32; N]> {
    let mut out = [0.0f32; N];
    for (i, slot) in out.iter_mut().enumerate() {
        let off = i * 4;
        let bytes: [u8; 4] = data.get(off..off + 4)?.try_into().ok()?;
        let v = f32::from_le_bytes(bytes);
        if !v.is_finite() {
            return None;
        }
        *slot = v;
    }
    Some(out)
}

fuzz_target!(|data: &[u8]| {
    // Quadratic: p0, p1, p2 + tolerance (7 f32s = 28 bytes).
    if let Some([x0, y0, x1, y1, x2, y2, tol]) = read_finite_f32s::<7>(data) {
        let _ = flatten_quad_bezier((x0, y0), (x1, y1), (x2, y2), tol);
    }

    // Cubic: p0, p1, p2, p3 + tolerance (9 f32s = 36 bytes).
    if let Some([x0, y0, x1, y1, x2, y2, x3, y3, tol]) = read_finite_f32s::<9>(data) {
        let _ = flatten_cubic_bezier((x0, y0), (x1, y1), (x2, y2), (x3, y3), tol);
    }
});
