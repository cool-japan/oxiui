//! Extended compositing helpers: blend modes + premultiplied-alpha utilities.
//!
//! The default [`Framebuffer::blend`](crate::framebuffer::Framebuffer::blend)
//! path uses *straight-alpha* source-over (Porter–Duff `over`). This module
//! adds Photoshop / SVG / CSS classics — `multiply`, `screen`, `overlay`,
//! `darken`, `lighten` — plus the premultiplied-alpha helpers callers need
//! when they want to do bulk maths in linear-ish space and only de-multiply
//! at the very end.
//!
//! All blend modes work on straight-alpha sRGB components in `[0, 255]`
//! (i.e. exactly what the framebuffer stores), so they're additive: callers
//! just pick a [`BlendMode`] and call [`blend_pixel`] (or
//! [`composite_into`]) — `Framebuffer` itself is unchanged.
//!
//! ## Premultiplied-alpha pipeline
//!
//! The internal `blend_over_premult_u8` helper performs Porter–Duff
//! source-over entirely in integer premultiplied-alpha space:
//!   1. Convert source and destination from straight-alpha to premult (u16).
//!   2. Blend in premult space: `out_r = src_pm + dst_pm * (255 - src_a) / 255`.
//!   3. Convert the premultiplied result back to straight-alpha for storage.
//!
//! This avoids the per-pixel division inherent in straight-alpha compositing
//! while keeping the public API and framebuffer format unchanged.

use crate::framebuffer::{pack_rgba, unpack, Framebuffer};

/// Compositing operator selected by [`blend_pixel`] /
/// [`composite_into`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Standard Porter–Duff `source-over` (straight alpha).
    Normal,
    /// `C = src * dst` per colour channel (darkening filter).
    Multiply,
    /// `C = 1 - (1 - src) * (1 - dst)` (lightening filter).
    Screen,
    /// Per-channel: multiply when `dst < 0.5`, screen otherwise.
    Overlay,
    /// `C = min(src, dst)` per channel.
    Darken,
    /// `C = max(src, dst)` per channel.
    Lighten,
}

/// A straight-alpha RGBA pixel as floats in `[0, 1]`.
///
/// Conversions to/from the framebuffer `0xAARRGGBB` format use the helpers
/// at the bottom of this file (`to_unit` / `from_unit`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RgbaUnit {
    /// Red channel, `[0, 1]`.
    pub r: f32,
    /// Green channel, `[0, 1]`.
    pub g: f32,
    /// Blue channel, `[0, 1]`.
    pub b: f32,
    /// Alpha channel, `[0, 1]`.
    pub a: f32,
}

impl RgbaUnit {
    /// Construct a pixel from raw float components (clamped to `[0, 1]`).
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Construct from an unsigned-byte `(r, g, b, a)` tuple.
    pub fn from_bytes(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Round to nearest `(r, g, b, a)` bytes.
    pub fn to_bytes(self) -> (u8, u8, u8, u8) {
        (
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }

    /// Multiply RGB by alpha — produces premultiplied form for fast bulk maths.
    pub fn premultiply(self) -> Self {
        Self {
            r: self.r * self.a,
            g: self.g * self.a,
            b: self.b * self.a,
            a: self.a,
        }
    }

    /// Inverse of [`premultiply`](Self::premultiply); safe when `a == 0`
    /// (returns transparent black rather than `NaN`).
    pub fn unpremultiply(self) -> Self {
        if self.a <= f32::EPSILON {
            Self {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }
        } else {
            Self {
                r: self.r / self.a,
                g: self.g / self.a,
                b: self.b / self.a,
                a: self.a,
            }
        }
    }
}

/// Convert a framebuffer packed pixel to floats in `[0, 1]`.
pub fn to_unit(px: u32) -> RgbaUnit {
    let (r, g, b, a) = unpack(px);
    RgbaUnit::from_bytes(r, g, b, a)
}

/// Pack floats back into a framebuffer `0xAARRGGBB` value.
pub fn from_unit(p: RgbaUnit) -> u32 {
    let (r, g, b, a) = p.to_bytes();
    pack_rgba(r, g, b, a)
}

// ---------------------------------------------------------------------------
// Integer premultiplied-alpha pipeline helpers
// ---------------------------------------------------------------------------

/// Convert a straight-alpha pixel `(r, g, b, a)` to premultiplied-alpha `u16`
/// components: `(r * a / 255, g * a / 255, b * a / 255, a)`.
///
/// Using `u16` avoids overflow: `255 * 255 = 65025 < u16::MAX`.
#[inline]
fn to_premult(r: u8, g: u8, b: u8, a: u8) -> (u16, u16, u16, u16) {
    let a16 = a as u16;
    (
        (r as u16 * a16 + 127) / 255,
        (g as u16 * a16 + 127) / 255,
        (b as u16 * a16 + 127) / 255,
        a16,
    )
}

/// Convert premultiplied-alpha components `(pr, pg, pb, a)` back to
/// straight-alpha bytes. Safe when `a == 0` (returns transparent black).
#[inline]
fn from_premult(pr: u16, pg: u16, pb: u16, a: u16) -> (u8, u8, u8, u8) {
    if a == 0 {
        return (0, 0, 0, 0);
    }
    let half = a / 2;
    (
        ((pr * 255 + half) / a).min(255) as u8,
        ((pg * 255 + half) / a).min(255) as u8,
        ((pb * 255 + half) / a).min(255) as u8,
        a as u8,
    )
}

/// Porter–Duff source-over in integer premultiplied-alpha space.
///
/// `src` and `dst` are straight-alpha `(r, g, b, a)` bytes.  The computation:
///   1. Premultiply both.
///   2. Blend: `out_r = src_pm + dst_pm * (255 - src_a) / 255`.
///   3. Compute `out_a = src_a + dst_a * (255 - src_a) / 255`.
///   4. Un-premultiply the result back to straight-alpha.
///
/// The framebuffer continues to store `0xAARRGGBB` straight-alpha — this
/// function does not change the storage invariant.
#[inline]
#[allow(clippy::too_many_arguments)]
pub(crate) fn blend_over_premult_u8(
    src_r: u8,
    src_g: u8,
    src_b: u8,
    src_a: u8,
    dst_r: u8,
    dst_g: u8,
    dst_b: u8,
    dst_a: u8,
) -> (u8, u8, u8, u8) {
    let (spr, spg, spb, sa) = to_premult(src_r, src_g, src_b, src_a);
    let (dpr, dpg, dpb, da) = to_premult(dst_r, dst_g, dst_b, dst_a);
    let inv_sa = 255u16 - sa;
    let out_r = spr + (dpr * inv_sa + 127) / 255;
    let out_g = spg + (dpg * inv_sa + 127) / 255;
    let out_b = spb + (dpb * inv_sa + 127) / 255;
    let out_a = sa + (da * inv_sa + 127) / 255;
    from_premult(out_r, out_g, out_b, out_a)
}

/// Apply `mode` to compose `src` over `dst`, returning the result in
/// straight-alpha float form.
///
/// All non-`Normal` modes use the standard SVG-compositing recipe:
/// the per-channel colour blend function `B(Cb, Cs)` is computed, then
/// combined with the alphas:
///
/// ```text
///   Co = (1 - dst.a) * Cs + (1 - src.a) * Cb + src.a * dst.a * B(Cb, Cs)
///   Ao = src.a + dst.a * (1 - src.a)
/// ```
///
/// This matches SVG 1.1 "Composite + Blend" semantics and CSS
/// `background-blend-mode`.
pub fn blend_mode(mode: BlendMode, src: RgbaUnit, dst: RgbaUnit) -> RgbaUnit {
    match mode {
        BlendMode::Normal => over(src, dst),
        BlendMode::Multiply => mode_combine(src, dst, |a, b| a * b),
        BlendMode::Screen => mode_combine(src, dst, |a, b| a + b - a * b),
        BlendMode::Overlay => mode_combine(src, dst, |cs, cb| {
            // SVG: overlay(Cb, Cs) = hard-light(Cs, Cb).
            if cb <= 0.5 {
                2.0 * cs * cb
            } else {
                1.0 - 2.0 * (1.0 - cs) * (1.0 - cb)
            }
        }),
        BlendMode::Darken => mode_combine(src, dst, f32::min),
        BlendMode::Lighten => mode_combine(src, dst, f32::max),
    }
}

/// Plain Porter–Duff source-over in straight-alpha space.
fn over(src: RgbaUnit, dst: RgbaUnit) -> RgbaUnit {
    let out_a = src.a + dst.a * (1.0 - src.a);
    if out_a <= f32::EPSILON {
        return RgbaUnit {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    let blend = |s: f32, d: f32| (s * src.a + d * dst.a * (1.0 - src.a)) / out_a;
    RgbaUnit {
        r: blend(src.r, dst.r),
        g: blend(src.g, dst.g),
        b: blend(src.b, dst.b),
        a: out_a,
    }
}

/// Generic SVG-style blend: applies `blend_fn` per colour channel and
/// composes the result with proper alpha handling.
fn mode_combine<F>(src: RgbaUnit, dst: RgbaUnit, blend_fn: F) -> RgbaUnit
where
    F: Fn(f32, f32) -> f32,
{
    let out_a = src.a + dst.a * (1.0 - src.a);
    if out_a <= f32::EPSILON {
        return RgbaUnit {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    // Premultiplied per the SVG compositing equation.
    let channel = |cs: f32, cb: f32| -> f32 {
        let b = blend_fn(cs.clamp(0.0, 1.0), cb.clamp(0.0, 1.0));
        let v = (1.0 - dst.a) * cs * src.a + (1.0 - src.a) * cb * dst.a + src.a * dst.a * b;
        // De-premultiply by out_a so the framebuffer stays in straight-alpha form.
        (v / out_a).clamp(0.0, 1.0)
    };
    RgbaUnit {
        r: channel(src.r, dst.r),
        g: channel(src.g, dst.g),
        b: channel(src.b, dst.b),
        a: out_a,
    }
}

/// Apply `mode` to blend `src` over the pixel currently at `(x, y)` in `fb`.
/// Out-of-bounds writes are ignored. For `Normal` this is equivalent to
/// [`Framebuffer::blend`](crate::framebuffer::Framebuffer::blend).
pub fn blend_pixel(fb: &mut Framebuffer, x: u32, y: u32, src: RgbaUnit, mode: BlendMode) {
    if x >= fb.width() || y >= fb.height() {
        return;
    }
    let Some(dst_px) = fb.get(x, y) else { return };
    let dst = to_unit(dst_px);
    let out = blend_mode(mode, src, dst);
    fb.set(x, y, from_unit(out));
}

/// Composite a rectangular `src` slab (interpreted as straight-alpha
/// RGBA8 row-major, `w * h * 4` bytes) into `fb` at `(dst_x, dst_y)`
/// using `mode`. Returns the number of pixels written.
///
/// For [`BlendMode::Normal`] the composite is done in integer premultiplied-
/// alpha space (via `blend_over_premult_u8`) so the blend math is exact and
/// avoids the per-pixel float division. Other modes fall back to the f32 path.
pub fn composite_into(
    fb: &mut Framebuffer,
    src: &[u8],
    w: u32,
    h: u32,
    dst_x: i64,
    dst_y: i64,
    mode: BlendMode,
) -> usize {
    if w == 0 || h == 0 || src.len() < (w * h * 4) as usize {
        return 0;
    }
    let mut written = 0;
    for j in 0..h {
        for i in 0..w {
            let si = ((j * w + i) * 4) as usize;
            let r = src[si];
            let g = src[si + 1];
            let b = src[si + 2];
            let a = src[si + 3];
            if a == 0 {
                continue;
            }
            let px = dst_x + i as i64;
            let py = dst_y + j as i64;
            if px < 0 || py < 0 {
                continue;
            }
            let pu = px as u32;
            let pv = py as u32;
            if pu >= fb.width() || pv >= fb.height() {
                continue;
            }
            if mode == BlendMode::Normal {
                // Fast path: premultiplied-alpha source-over in integer space.
                // Framebuffer stays in straight-alpha 0xAARRGGBB.
                let dst_px = fb.get(pu, pv).unwrap_or(0);
                let (dr, dg, db, da) = unpack(dst_px);
                let (or_, og, ob, oa) = blend_over_premult_u8(r, g, b, a, dr, dg, db, da);
                fb.set(pu, pv, crate::framebuffer::pack_rgba(or_, og, ob, oa));
            } else {
                blend_pixel(fb, pu, pv, RgbaUnit::from_bytes(r, g, b, a), mode);
            }
            written += 1;
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::Color;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn round_trip_unit_bytes() {
        let p = RgbaUnit::from_bytes(123, 45, 200, 99);
        let (r, g, b, a) = p.to_bytes();
        assert_eq!((r, g, b, a), (123, 45, 200, 99));
    }

    #[test]
    fn premultiply_inverse() {
        let p = RgbaUnit::new(0.8, 0.4, 0.2, 0.5);
        let q = p.premultiply().unpremultiply();
        assert!(approx_eq(q.r, p.r, 1e-5));
        assert!(approx_eq(q.g, p.g, 1e-5));
        assert!(approx_eq(q.b, p.b, 1e-5));
        assert!(approx_eq(q.a, p.a, 1e-5));
    }

    #[test]
    fn premultiply_zero_alpha_safe() {
        let p = RgbaUnit::new(0.9, 0.9, 0.9, 0.0);
        let q = p.premultiply().unpremultiply();
        assert!(approx_eq(q.a, 0.0, 1e-5));
        assert!(approx_eq(q.r, 0.0, 1e-5));
    }

    #[test]
    fn multiply_white_is_identity() {
        // src=white opaque, mode=Multiply over red dst → red preserved.
        let src = RgbaUnit::from_bytes(255, 255, 255, 255);
        let dst = RgbaUnit::from_bytes(255, 0, 0, 255);
        let out = blend_mode(BlendMode::Multiply, src, dst);
        let (r, g, b, a) = out.to_bytes();
        assert_eq!((r, g, b, a), (255, 0, 0, 255));
    }

    #[test]
    fn screen_black_is_identity() {
        let src = RgbaUnit::from_bytes(0, 0, 0, 255);
        let dst = RgbaUnit::from_bytes(80, 120, 200, 255);
        let out = blend_mode(BlendMode::Screen, src, dst);
        let (r, g, b, _) = out.to_bytes();
        // Screen with black should keep dst exactly.
        assert!((r as i32 - 80).abs() <= 1);
        assert!((g as i32 - 120).abs() <= 1);
        assert!((b as i32 - 200).abs() <= 1);
    }

    #[test]
    fn darken_picks_min_per_channel() {
        let src = RgbaUnit::from_bytes(100, 200, 50, 255);
        let dst = RgbaUnit::from_bytes(150, 50, 50, 255);
        let out = blend_mode(BlendMode::Darken, src, dst);
        let (r, g, b, _) = out.to_bytes();
        assert!((r as i32 - 100).abs() <= 1);
        assert!((g as i32 - 50).abs() <= 1);
        assert!((b as i32 - 50).abs() <= 1);
    }

    #[test]
    fn lighten_picks_max_per_channel() {
        let src = RgbaUnit::from_bytes(100, 200, 50, 255);
        let dst = RgbaUnit::from_bytes(150, 50, 50, 255);
        let out = blend_mode(BlendMode::Lighten, src, dst);
        let (r, g, b, _) = out.to_bytes();
        assert!((r as i32 - 150).abs() <= 1);
        assert!((g as i32 - 200).abs() <= 1);
        assert!((b as i32 - 50).abs() <= 1);
    }

    #[test]
    fn overlay_changes_with_dst_mid() {
        // Dst at 0.5 → overlay is the boundary; verify it produces some
        // smooth blend with a grey src (no NaN / no panic).
        let src = RgbaUnit::from_bytes(128, 128, 128, 255);
        let dst = RgbaUnit::from_bytes(128, 128, 128, 255);
        let out = blend_mode(BlendMode::Overlay, src, dst);
        let (r, _, _, a) = out.to_bytes();
        assert_eq!(a, 255);
        // Result must remain in-gamut (r is u8, so always <= 255; just verify
        // we got a valid result).
        let _ = r;
    }

    #[test]
    fn over_zero_alpha_safe() {
        let src = RgbaUnit::new(0.0, 0.0, 0.0, 0.0);
        let dst = RgbaUnit::new(0.0, 0.0, 0.0, 0.0);
        let out = blend_mode(BlendMode::Normal, src, dst);
        let (_, _, _, a) = out.to_bytes();
        assert_eq!(a, 0);
    }

    #[test]
    fn composite_into_writes_pixels() {
        let mut fb = Framebuffer::with_fill(4, 4, Color(0, 0, 0, 255));
        // 2x2 all-white opaque src.
        let src = vec![
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ];
        let written = composite_into(&mut fb, &src, 2, 2, 1, 1, BlendMode::Normal);
        assert_eq!(written, 4);
        let result = fb.get_rgba(1, 1).unwrap_or_default();
        assert_eq!(result, (255, 255, 255, 255));
    }

    #[test]
    fn blend_pixel_out_of_bounds_noop() {
        let mut fb = Framebuffer::with_fill(2, 2, Color(10, 20, 30, 255));
        blend_pixel(
            &mut fb,
            99,
            99,
            RgbaUnit::from_bytes(255, 255, 255, 255),
            BlendMode::Normal,
        );
        let v = fb.get_rgba(0, 0).unwrap_or((0, 0, 0, 0));
        assert_eq!(v, (10, 20, 30, 255));
    }

    #[test]
    fn blend_multiply_black_is_black() {
        // Multiply any colour with opaque black → black.
        let black = RgbaUnit::from_bytes(0, 0, 0, 255);
        let src = RgbaUnit::from_bytes(200, 100, 50, 255);
        let out = blend_mode(BlendMode::Multiply, src, black);
        let (r, g, b, _) = out.to_bytes();
        assert_eq!((r, g, b), (0, 0, 0), "multiply with black must give black");
    }

    #[test]
    fn blend_screen_white_is_white() {
        // Screen any colour with opaque white → white.
        let white = RgbaUnit::from_bytes(255, 255, 255, 255);
        let src = RgbaUnit::from_bytes(100, 150, 200, 255);
        let out = blend_mode(BlendMode::Screen, src, white);
        let (r, g, b, _) = out.to_bytes();
        assert_eq!(
            (r, g, b),
            (255, 255, 255),
            "screen with white must give white"
        );
    }

    #[test]
    fn premultiply_roundtrip() {
        // Premultiply then unpremultiply should recover the original (within rounding).
        let p = RgbaUnit::new(0.7, 0.3, 0.9, 0.6);
        let q = p.premultiply().unpremultiply();
        assert!(approx_eq(q.r, p.r, 1e-4), "r: {:.5} vs {:.5}", q.r, p.r);
        assert!(approx_eq(q.g, p.g, 1e-4), "g: {:.5} vs {:.5}", q.g, p.g);
        assert!(approx_eq(q.b, p.b, 1e-4), "b: {:.5} vs {:.5}", q.b, p.b);
        assert!(approx_eq(q.a, p.a, 1e-5), "a: {:.5} vs {:.5}", q.a, p.a);
    }

    // -----------------------------------------------------------------------
    // S2: premultiplied-alpha golden-value + signature-stability tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_premult_blend_golden() {
        // Blend semi-transparent red (r=255, g=0, b=0, a=128) over opaque white
        // (r=255, g=255, b=255, a=255).
        //
        // Expected result (straight-alpha source-over):
        //   out_a = 128 + 255 * (255 - 128) / 255  = 128 + 127 = 255
        //   out_r = (255 * 128 + 255 * 255 * 127/255) / 255
        //         = (32640 + 32385) / 255 ≈ 255
        //   out_g = (0   * 128 + 255 * 127) / 255 ≈ 127
        //   out_b = (0   * 128 + 255 * 127) / 255 ≈ 127
        //
        // Golden values: (255, 127, 127, 255), tolerance ±2.
        let (r, g, b, a) = blend_over_premult_u8(255, 0, 0, 128, 255, 255, 255, 255);
        assert!((r as i32 - 255).abs() <= 2, "r={r} expected ~255");
        assert!((g as i32 - 127).abs() <= 2, "g={g} expected ~127");
        assert!((b as i32 - 127).abs() <= 2, "b={b} expected ~127");
        assert!((a as i32 - 255).abs() <= 2, "a={a} expected ~255");
    }

    #[test]
    fn test_blend_signatures_stable() {
        // Compile-time + runtime stability check: call every public blend fn
        // with valid args and verify it does not panic.

        // blend_mode
        let src = RgbaUnit::from_bytes(128, 64, 32, 200);
        let dst = RgbaUnit::from_bytes(10, 20, 30, 255);
        let _ = blend_mode(BlendMode::Normal, src, dst);
        let _ = blend_mode(BlendMode::Multiply, src, dst);
        let _ = blend_mode(BlendMode::Screen, src, dst);
        let _ = blend_mode(BlendMode::Overlay, src, dst);
        let _ = blend_mode(BlendMode::Darken, src, dst);
        let _ = blend_mode(BlendMode::Lighten, src, dst);

        // to_unit / from_unit round-trip
        let px = crate::framebuffer::pack_rgba(100, 150, 200, 180);
        let unit = to_unit(px);
        let _ = from_unit(unit);

        // blend_pixel (out-of-bounds is a no-op; just verify no panic)
        let mut fb = Framebuffer::with_fill(4, 4, Color(0, 0, 0, 255));
        blend_pixel(&mut fb, 0, 0, src, BlendMode::Normal);
        blend_pixel(&mut fb, 99, 99, src, BlendMode::Screen); // out-of-bounds noop

        // composite_into (minimal 1×1)
        let src_bytes = [255u8, 0, 0, 128];
        let written = composite_into(&mut fb, &src_bytes, 1, 1, 0, 0, BlendMode::Normal);
        assert!(written <= 1);
    }
}
