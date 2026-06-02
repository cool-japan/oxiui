//! Colour-manipulation utilities operating on [`oxiui_core::Color`].
//!
//! Interpolation and lighten/darken operate in sRGB component space (fast,
//! perceptually adequate for UI hover/pressed shading). WCAG luminance and
//! contrast live in [`crate::high_contrast`] and are re-exported here.

use oxiui_core::Color;

pub use crate::high_contrast::{wcag_contrast, wcag_luminance};

/// Linearly interpolate two colours in sRGB space at `t` in `[0, 1]`.
///
/// `t = 0.0` yields `a`, `t = 1.0` yields `b`.
pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 {
        let v = x as f32 + (y as f32 - x as f32) * t;
        v.round().clamp(0.0, 255.0) as u8
    };
    Color(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2), mix(a.3, b.3))
}

/// Mix two colours equally (50/50 blend in sRGB space).
pub fn mix(a: Color, b: Color) -> Color {
    lerp(a, b, 0.5)
}

/// Lighten `c` toward white by `amount` in `[0, 1]` (alpha unchanged).
pub fn lighten(c: Color, amount: f32) -> Color {
    let white = Color(255, 255, 255, c.3);
    lerp(c, white, amount)
}

/// Darken `c` toward black by `amount` in `[0, 1]` (alpha unchanged).
pub fn darken(c: Color, amount: f32) -> Color {
    let black = Color(0, 0, 0, c.3);
    lerp(c, black, amount)
}

/// Return `c` with its alpha channel replaced by `alpha`.
pub fn with_alpha(c: Color, alpha: u8) -> Color {
    Color(c.0, c.1, c.2, alpha)
}

/// Return `c` with its alpha scaled by `factor` in `[0, 1]`.
pub fn scale_alpha(c: Color, factor: f32) -> Color {
    let a = (c.3 as f32 * factor.clamp(0.0, 1.0)).round() as u8;
    Color(c.0, c.1, c.2, a)
}

// ── HSL colour model ─────────────────────────────────────────────────────────

/// Convert an sRGB [`Color`] to `(hue °, saturation [0,1], lightness [0,1])`.
///
/// Hue is in the range `[0.0, 360.0)`. Achromatic colours return hue `0.0`.
pub fn to_hsl(c: Color) -> (f32, f32, f32) {
    let r = c.0 as f32 / 255.0;
    let g = c.1 as f32 / 255.0;
    let b = c.2 as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min) / 2.0;
    if delta < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h_raw = if (max - r).abs() < 1e-6 {
        (g - b) / delta
    } else if (max - g).abs() < 1e-6 {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    let h = (h_raw * 60.0).rem_euclid(360.0);
    (h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0))
}

/// Convert `(hue °, saturation [0,1], lightness [0,1])` to an opaque sRGB [`Color`].
pub fn from_hsl(h: f32, s: f32, l: f32) -> Color {
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = chroma * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    let to_u8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color(to_u8(r1), to_u8(g1), to_u8(b1), 255)
}

/// Increase the HSL saturation of `c` by `amount` (clamped to `[0, 1]`).
///
/// Alpha channel is preserved.
pub fn saturate(c: Color, amount: f32) -> Color {
    let (h, s, l) = to_hsl(c);
    let new_s = (s + amount).clamp(0.0, 1.0);
    let rgb = from_hsl(h, new_s, l);
    Color(rgb.0, rgb.1, rgb.2, c.3)
}

/// Decrease the HSL saturation of `c` by `amount` (clamped to `[0, 1]`).
///
/// Alpha channel is preserved.
pub fn desaturate(c: Color, amount: f32) -> Color {
    let (h, s, l) = to_hsl(c);
    let new_s = (s - amount).clamp(0.0, 1.0);
    let rgb = from_hsl(h, new_s, l);
    Color(rgb.0, rgb.1, rgb.2, c.3)
}

// ── Oklch colour model ───────────────────────────────────────────────────────
//
// Conversion path: sRGB → linear sRGB → OKLab → Oklch
// Reference: Björn Ottosson, https://bottosson.github.io/posts/oklab/

/// Convert an sRGB [`Color`] to Oklch `(L, C, H)` where
/// - `L` ∈ `[0, 1]` (perceptual lightness),
/// - `C` ≥ 0 (chroma),
/// - `H` ∈ `[0°, 360°)` (hue angle).
pub fn to_oklch(c: Color) -> (f32, f32, f32) {
    // sRGB to linear sRGB.
    let srgb_to_linear = |v: u8| {
        let f = v as f32 / 255.0;
        if f <= 0.04045 {
            f / 12.92
        } else {
            ((f + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = srgb_to_linear(c.0);
    let g = srgb_to_linear(c.1);
    let b = srgb_to_linear(c.2);
    // linear sRGB → OKLab (Ottosson matrix, D65 illuminant).
    // Constants truncated to f32 representable precision (≤ 7 significant digits).
    let l_raw = 0.412_221_5 * r + 0.536_332_5 * g + 0.051_445_99 * b;
    let m_raw = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s_raw = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let l_cbrt = l_raw.cbrt();
    let m_cbrt = m_raw.cbrt();
    let s_cbrt = s_raw.cbrt();
    let ok_l = 0.210_454_26 * l_cbrt + 0.793_617_8 * m_cbrt - 0.004_072_047 * s_cbrt;
    let ok_a = 1.977_998_5 * l_cbrt - 2.428_592_2 * m_cbrt + 0.450_593_7 * s_cbrt;
    let ok_b = 0.025_904_04 * l_cbrt + 0.782_771_8 * m_cbrt - 0.808_675_8 * s_cbrt;
    // OKLab → Oklch.
    let chroma = (ok_a * ok_a + ok_b * ok_b).sqrt();
    let hue = ok_b.atan2(ok_a).to_degrees().rem_euclid(360.0);
    (ok_l.clamp(0.0, 1.0), chroma, hue)
}

/// Convert Oklch `(L, C, H)` back to an opaque sRGB [`Color`].
pub fn from_oklch(l: f32, chroma: f32, hue_deg: f32) -> Color {
    // Oklch → OKLab.
    let h_rad = hue_deg.to_radians();
    let ok_a = chroma * h_rad.cos();
    let ok_b = chroma * h_rad.sin();
    // OKLab → linear sRGB (Ottosson inverse matrix).
    // Constants truncated to f32 representable precision (≤ 7 significant digits).
    let l_cbrt = l + 0.396_337_8 * ok_a + 0.215_803_76 * ok_b;
    let m_cbrt = l - 0.105_561_35 * ok_a - 0.063_854_17 * ok_b;
    let s_cbrt = l - 0.089_484_18 * ok_a - 1.291_485_5 * ok_b;
    let l_raw = l_cbrt * l_cbrt * l_cbrt;
    let m_raw = m_cbrt * m_cbrt * m_cbrt;
    let s_raw = s_cbrt * s_cbrt * s_cbrt;
    let r_lin = 4.076_741_7 * l_raw - 3.307_711_6 * m_raw + 0.230_969_94 * s_raw;
    let g_lin = -1.268_438 * l_raw + 2.609_757_4 * m_raw - 0.341_319_4 * s_raw;
    let b_lin = -0.004_196_086 * l_raw - 0.703_418_6 * m_raw + 1.707_614_7 * s_raw;
    // linear sRGB → sRGB (gamma).
    let linear_to_srgb = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    let to_u8 = |v: f32| (linear_to_srgb(v) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color(to_u8(r_lin), to_u8(g_lin), to_u8(b_lin), 255)
}

/// Interpolate two colours in **Oklch** space at `t` ∈ `[0, 1]`.
///
/// Oklch is perceptually uniform, so mid-points appear visually equidistant
/// rather than washing through grey (as sRGB lerp does).
///
/// `t = 0.0` → `a`, `t = 1.0` → `b`. Alpha is interpolated linearly in sRGB.
pub fn oklch_lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (la, ca, ha) = to_oklch(a);
    let (lb, cb, hb) = to_oklch(b);
    // Interpolate L and C linearly.
    let l = la + (lb - la) * t;
    let c = ca + (cb - ca) * t;
    // Hue is circular — take the shorter arc.
    let diff = (hb - ha + 540.0).rem_euclid(360.0) - 180.0;
    let h = (ha + diff * t).rem_euclid(360.0);
    let rgb = from_oklch(l, c, h);
    // Alpha in sRGB space.
    let alpha = (a.3 as f32 + (b.3 as f32 - a.3 as f32) * t).round() as u8;
    Color(rgb.0, rgb.1, rgb.2, alpha)
}

/// Pick whichever of `light` / `dark` has the higher WCAG contrast against `bg`.
///
/// Useful for choosing readable text colour on an arbitrary background.
pub fn best_contrast(bg: Color, light: Color, dark: Color) -> Color {
    let cl = wcag_contrast((light.0, light.1, light.2), (bg.0, bg.1, bg.2));
    let cd = wcag_contrast((dark.0, dark.1, dark.2), (bg.0, bg.1, bg.2));
    if cl >= cd {
        light
    } else {
        dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_endpoints() {
        let a = Color(0, 0, 0, 255);
        let b = Color(255, 255, 255, 255);
        assert_eq!(lerp(a, b, 0.0), a);
        assert_eq!(lerp(a, b, 1.0), b);
        let m = lerp(a, b, 0.5);
        assert!((126..=129).contains(&m.0));
    }

    #[test]
    fn lighten_darken_bounds() {
        let c = Color(100, 100, 100, 255);
        let lighter = lighten(c, 1.0);
        assert_eq!(lighter, Color(255, 255, 255, 255));
        let darker = darken(c, 1.0);
        assert_eq!(darker, Color(0, 0, 0, 255));
        // Zero amount is a no-op.
        assert_eq!(lighten(c, 0.0), c);
        assert_eq!(darken(c, 0.0), c);
    }

    #[test]
    fn alpha_helpers() {
        let c = Color(10, 20, 30, 200);
        assert_eq!(with_alpha(c, 50).3, 50);
        assert_eq!(scale_alpha(c, 0.5).3, 100);
        // RGB preserved.
        assert_eq!(scale_alpha(c, 0.5).0, 10);
    }

    #[test]
    fn best_contrast_picks_readable() {
        let white = Color(255, 255, 255, 255);
        let black = Color(0, 0, 0, 255);
        // On a dark background, white wins.
        assert_eq!(best_contrast(Color(20, 20, 30, 255), white, black), white);
        // On a light background, black wins.
        assert_eq!(
            best_contrast(Color(240, 240, 240, 255), white, black),
            black
        );
    }

    #[test]
    fn known_contrast_ratio() {
        // Black vs white is the maximal 21:1 ratio.
        let r = wcag_contrast((0, 0, 0), (255, 255, 255));
        assert!((r - 21.0).abs() < 0.1);
    }

    // ── HSL ──────────────────────────────────────────────────────────────────

    #[test]
    fn hsl_roundtrip() {
        let c = Color(120, 80, 200, 255);
        let (h, s, l) = to_hsl(c);
        let back = from_hsl(h, s, l);
        // Each channel must recover within ±1.
        assert!(
            (c.0 as i32 - back.0 as i32).abs() <= 1,
            "R: {} vs {}",
            c.0,
            back.0
        );
        assert!(
            (c.1 as i32 - back.1 as i32).abs() <= 1,
            "G: {} vs {}",
            c.1,
            back.1
        );
        assert!(
            (c.2 as i32 - back.2 as i32).abs() <= 1,
            "B: {} vs {}",
            c.2,
            back.2
        );
    }

    #[test]
    fn saturate_increases_saturation() {
        // A desaturated-ish colour (brownish grey).
        let c = Color(150, 120, 100, 255);
        let (_, s_before, _) = to_hsl(c);
        let saturated = saturate(c, 0.2);
        let (_, s_after, _) = to_hsl(saturated);
        assert!(
            s_after > s_before,
            "saturation must increase: {s_before} → {s_after}"
        );
    }

    #[test]
    fn desaturate_to_zero_is_gray() {
        let c = Color(200, 100, 50, 255);
        let grey = desaturate(c, 1.0);
        // A fully desaturated colour must have R == G == B (pure grey).
        assert_eq!(grey.0, grey.1, "R != G for full desaturation");
        assert_eq!(grey.1, grey.2, "G != B for full desaturation");
    }

    // ── Oklch ────────────────────────────────────────────────────────────────

    #[test]
    fn oklch_lerp_endpoints() {
        let a = Color(30, 50, 200, 255);
        let b = Color(220, 180, 40, 255);
        let at_zero = oklch_lerp(a, b, 0.0);
        let at_one = oklch_lerp(a, b, 1.0);
        // Allow ±2 rounding tolerance per channel.
        for (x, y) in [(at_zero.0, a.0), (at_zero.1, a.1), (at_zero.2, a.2)] {
            assert!((x as i32 - y as i32).abs() <= 2, "{x} vs {y}");
        }
        for (x, y) in [(at_one.0, b.0), (at_one.1, b.1), (at_one.2, b.2)] {
            assert!((x as i32 - y as i32).abs() <= 2, "{x} vs {y}");
        }
    }

    #[test]
    fn oklch_lerp_midpoint_is_perceptual() {
        // lerp(black, white, 0.5) should yield L ≈ 0.5 in Oklch space.
        let black = Color(0, 0, 0, 255);
        let white = Color(255, 255, 255, 255);
        let mid = oklch_lerp(black, white, 0.5);
        let (l, _, _) = to_oklch(mid);
        // In Oklch, L=0.5 corresponds to a perceptually mid grey
        // (~sRGB 119-127 range). Allow a wide tolerance.
        assert!(l > 0.35 && l < 0.65, "L should be near 0.5, got {l}");
    }
}
