//! Bayer-matrix ordered dithering for reduced-bit output paths.
//!
//! Bayer dithering adds a spatially varying threshold to each pixel before
//! quantisation, spreading quantisation error in a visually pleasing pattern
//! rather than banding. The 8×8 matrix covers a large enough area to avoid
//! coarse moiré artefacts.

use crate::clip::ClipRect;
use crate::framebuffer::{pack_rgba, unpack, Framebuffer};

/// An 8×8 Bayer threshold matrix (indices 0–63), normalised to `[0, 1)`.
///
/// The canonical Bayer matrix values range from 0 to 63. We store the
/// unnormalised integer values and normalise on use so callers can inspect
/// the raw matrix if needed.
#[derive(Clone, Debug)]
pub struct BayerMatrix {
    /// 8×8 raw threshold values in `[0, 63]`.
    pub matrix: [[u8; 8]; 8],
}

impl BayerMatrix {
    /// Construct the standard 8×8 Bayer threshold matrix.
    ///
    /// Values follow the recursive construction:
    /// `B(2n) = [[4*B(n), 4*B(n)+2], [4*B(n)+3, 4*B(n)+1]]`
    /// starting from `B(1) = [[0]]`.
    pub fn standard_8x8() -> Self {
        // Standard Bayer 8×8 matrix (0-indexed, range 0–63).
        Self {
            matrix: [
                [0, 32, 8, 40, 2, 34, 10, 42],
                [48, 16, 56, 24, 50, 18, 58, 26],
                [12, 44, 4, 36, 14, 46, 6, 38],
                [60, 28, 52, 20, 62, 30, 54, 22],
                [3, 35, 11, 43, 1, 33, 9, 41],
                [51, 19, 59, 27, 49, 17, 57, 25],
                [15, 47, 7, 39, 13, 45, 5, 37],
                [63, 31, 55, 23, 61, 29, 53, 21],
            ],
        }
    }

    /// Return the normalised threshold for position `(x, y)`, in `[0, 1)`.
    ///
    /// The position wraps modulo 8 so the matrix tiles over arbitrary regions.
    pub fn threshold(&self, x: u32, y: u32) -> f32 {
        let tx = (x % 8) as usize;
        let ty = (y % 8) as usize;
        self.matrix[ty][tx] as f32 / 64.0
    }
}

impl Default for BayerMatrix {
    fn default() -> Self {
        Self::standard_8x8()
    }
}

/// Apply Bayer ordered dithering to the RGBA pixels within `rect` in `fb`.
///
/// Each colour channel is shifted right by `bits_to_drop` bits, dithered
/// using the 8×8 Bayer matrix, and then clamped back to `[0, 255]`. This
/// simulates a display with `8 - bits_to_drop` bits per channel.
///
/// A `bits_to_drop` of 0 is a no-op. Values > 7 are clamped to 7.
pub fn ordered_dither_rgba(fb: &mut Framebuffer, rect: ClipRect, bits_to_drop: u32) {
    if bits_to_drop == 0 {
        return;
    }
    let bits = bits_to_drop.min(7);
    let step = (1u32 << bits) as f32; // quantisation step size
    let matrix = BayerMatrix::standard_8x8();

    let fb_w = fb.width();
    let fb_h = fb.height();
    let x0 = rect.x0.max(0) as u32;
    let y0 = rect.y0.max(0) as u32;
    let x1 = (rect.x1 as u32).min(fb_w);
    let y1 = (rect.y1 as u32).min(fb_h);

    for y in y0..y1 {
        for x in x0..x1 {
            let Some(px) = fb.get(x, y) else { continue };
            let (r, g, b, a) = unpack(px);
            let thresh = matrix.threshold(x, y); // in [0, 1)
                                                 // Add dither offset, then quantise.
            let dither_channel = |c: u8| -> u8 {
                let v = c as f32 + thresh * step;
                // Quantise: round down to the nearest multiple of `step`.
                let q = (v / step).floor() * step;
                q.clamp(0.0, 255.0) as u8
            };
            let nr = dither_channel(r);
            let ng = dither_channel(g);
            let nb = dither_channel(b);
            fb.set(x, y, pack_rgba(nr, ng, nb, a));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;

    fn grey_fb(w: u32, h: u32, level: u8) -> Framebuffer {
        use oxiui_core::Color;
        Framebuffer::with_fill(w, h, Color(level, level, level, 255))
    }

    #[test]
    fn bayer_matrix_range() {
        let m = BayerMatrix::standard_8x8();
        let mut all_values = std::collections::HashSet::new();
        for row in &m.matrix {
            for &v in row {
                all_values.insert(v);
            }
        }
        // All 64 values 0–63 must be present (bijection property).
        assert_eq!(
            all_values.len(),
            64,
            "8x8 Bayer matrix must have 64 unique values"
        );
        assert!(!all_values.contains(&64), "max value must be 63");
    }

    #[test]
    fn bayer_threshold_range() {
        let m = BayerMatrix::standard_8x8();
        for y in 0..8 {
            for x in 0..8 {
                let t = m.threshold(x, y);
                assert!(
                    (0.0..1.0).contains(&t),
                    "threshold {t} out of range at ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn bayer_determinism() {
        // Same input → same dithered output on repeated calls.
        use crate::clip::ClipRect;
        let mut fb1 = grey_fb(8, 8, 100);
        let mut fb2 = grey_fb(8, 8, 100);
        let rect = ClipRect::full(8, 8);
        ordered_dither_rgba(&mut fb1, rect, 2);
        ordered_dither_rgba(&mut fb2, rect, 2);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(fb1.get(x, y), fb2.get(x, y), "mismatch at ({x},{y})");
            }
        }
    }

    #[test]
    fn zero_bits_noop() {
        use crate::clip::ClipRect;
        use oxiui_core::Color;
        let mut fb = Framebuffer::with_fill(4, 4, Color(123, 45, 67, 200));
        let original = fb.get(0, 0);
        ordered_dither_rgba(&mut fb, ClipRect::full(4, 4), 0);
        assert_eq!(fb.get(0, 0), original, "0 bits_to_drop must be a no-op");
    }

    #[test]
    fn dither_changes_some_pixels() {
        // With bits_to_drop=4, step=16. Level=10 → [10, 10+15]. Some cross boundary at 16.
        // The Bayer matrix has entries from 0 to 63/64, so 10 + (x/64)*16 ranges from 10 to 24.9.
        // floor(10/16)*16 = 0, floor(24/16)*16 = 16. So some pixels will change.
        use crate::clip::ClipRect;
        let original = grey_fb(8, 8, 10);
        let mut dithered = grey_fb(8, 8, 10);
        ordered_dither_rgba(&mut dithered, ClipRect::full(8, 8), 4);
        let mut changed = 0u32;
        for y in 0..8 {
            for x in 0..8 {
                if original.get(x, y) != dithered.get(x, y) {
                    changed += 1;
                }
            }
        }
        assert!(
            changed > 0,
            "dithering should change at least some pixels (level=10, bits=4)"
        );
    }
}
