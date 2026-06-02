//! Box-shadow via separable 1-D Gaussian blur with a cached kernel.
//!
//! A drop shadow is rendered by:
//! 1. Painting the shadow rectangle at `(rect + offset)` in the shadow colour.
//! 2. Extracting the alpha channel of that rectangle.
//! 3. Applying a separable horizontal + vertical Gaussian blur to the alpha.
//! 4. Compositing the blurred shadow alpha under the framebuffer content.
//!
//! All operations are pure Rust — no SIMD, no FFT, no external crates.

use std::collections::HashMap;

use crate::clip::ClipRect;
use crate::framebuffer::{pack_rgba, Framebuffer};
use oxiui_core::Color;

// ---------------------------------------------------------------------------
// Kernel cache
// ---------------------------------------------------------------------------

/// A cache that stores pre-computed Gaussian kernels keyed by blur-radius bits.
///
/// This avoids recomputing the same kernel when the same `blur_radius` is used
/// repeatedly across frames.
#[derive(Debug, Default)]
pub struct GaussianCache {
    /// Maps `blur_radius.to_bits()` → normalised 1-D Gaussian kernel weights.
    kernels: HashMap<u32, Vec<f32>>,
}

impl GaussianCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a reference to the 1-D Gaussian kernel for `blur_radius`.
    ///
    /// The kernel is half-width — centred at index 0, extending to the right.
    /// Callers should mirror it when convolving.
    ///
    /// `sigma` is derived as `blur_radius / 2.0` (CSS-style).
    pub fn kernel(&mut self, blur_radius: f32) -> &[f32] {
        let key = blur_radius.to_bits();
        self.kernels
            .entry(key)
            .or_insert_with(|| gaussian_kernel(blur_radius))
    }
}

/// Compute a normalised 1-D Gaussian kernel with `sigma = blur_radius / 2`.
///
/// The kernel is stored as a half-width (non-negative taps only) because the
/// full kernel is symmetric.
pub fn gaussian_kernel(blur_radius: f32) -> Vec<f32> {
    let sigma = (blur_radius / 2.0).max(0.01);
    let radius = (sigma * 3.0).ceil() as usize;
    let mut weights: Vec<f32> = (0..=radius)
        .map(|i| {
            let x = i as f32;
            (-x * x / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    // Normalise: sum = weights[0] + 2 * (weights[1] + ... + weights[radius]).
    let sum: f32 = weights[0] + 2.0 * weights[1..].iter().sum::<f32>();
    for w in &mut weights {
        *w /= sum;
    }
    weights
}

// ---------------------------------------------------------------------------
// Separable Gaussian blur (alpha channel)
// ---------------------------------------------------------------------------

/// Apply a separable Gaussian blur to the alpha channel of a rectangular
/// region within `alpha` (a flat `width × height` f32 buffer).
///
/// This is a horizontal pass followed by a vertical pass over the same buffer.
pub fn gaussian_blur_alpha(alpha: &mut [f32], width: usize, height: usize, kernel: &[f32]) {
    if width == 0 || height == 0 || kernel.is_empty() {
        return;
    }
    // Horizontal pass.
    let mut tmp = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut acc = alpha[y * width + x] * kernel[0];
            for (ki, &w) in kernel.iter().enumerate().skip(1) {
                let xl = x as i64 - ki as i64;
                if xl >= 0 {
                    acc += alpha[y * width + xl as usize] * w;
                }
                let xr = x as i64 + ki as i64;
                if xr < width as i64 {
                    acc += alpha[y * width + xr as usize] * w;
                }
            }
            tmp[y * width + x] = acc;
        }
    }
    // Vertical pass.
    for y in 0..height {
        for x in 0..width {
            let mut acc = tmp[y * width + x] * kernel[0];
            for (ki, &w) in kernel.iter().enumerate().skip(1) {
                let ya = y as i64 - ki as i64;
                if ya >= 0 {
                    acc += tmp[ya as usize * width + x] * w;
                }
                let yb = y as i64 + ki as i64;
                if yb < height as i64 {
                    acc += tmp[yb as usize * width + x] * w;
                }
            }
            alpha[y * width + x] = acc.clamp(0.0, 1.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Box shadow
// ---------------------------------------------------------------------------

/// Render a drop (or inset) box shadow for a rectangle `rect` into `fb`.
///
/// # Parameters
/// * `rect` — the widget's bounding rectangle `(x, y, w, h)`.
/// * `offset_x`, `offset_y` — shadow offset in pixels.
/// * `blur_radius` — shadow blur radius in pixels.
/// * `color` — shadow colour (alpha is composited).
/// * `cache` — Gaussian kernel cache (avoids recomputation).
///
/// The shadow is rendered *behind* the widget: callers should draw the widget
/// content over the shadow after this call.
pub fn box_shadow(
    fb: &mut Framebuffer,
    rect: (f32, f32, f32, f32),
    offset_x: f32,
    offset_y: f32,
    blur_radius: f32,
    color: Color,
    cache: &mut GaussianCache,
) {
    let (rx, ry, rw, rh) = rect;
    if rw <= 0.0 || rh <= 0.0 {
        return;
    }
    let br = blur_radius.max(0.0);
    let kernel = cache.kernel(br).to_vec(); // clone to avoid borrow conflict

    // Shadow rect (offset + blur expansion).
    let sx = (rx + offset_x - br).floor() as i64;
    let sy = (ry + offset_y - br).floor() as i64;
    let sw = (rw + br * 2.0).ceil() as usize + 1;
    let sh = (rh + br * 2.0).ceil() as usize + 1;

    if sw == 0 || sh == 0 {
        return;
    }

    // Allocate alpha buffer for the shadow rect.
    let mut alpha = vec![0.0f32; sw * sh];

    // Fill the central (non-blurred) region of the shadow.
    let inner_x0 = (br as usize).min(sw);
    let inner_y0 = (br as usize).min(sh);
    let inner_x1 = (inner_x0 + rw.ceil() as usize).min(sw);
    let inner_y1 = (inner_y0 + rh.ceil() as usize).min(sh);
    let shadow_a = color.3 as f32 / 255.0;

    for row in inner_y0..inner_y1 {
        for col in inner_x0..inner_x1 {
            alpha[row * sw + col] = shadow_a;
        }
    }

    // Blur the alpha buffer.
    if br > 0.0 {
        gaussian_blur_alpha(&mut alpha, sw, sh, &kernel);
    }

    // Composite blurred alpha into framebuffer using straight Porter-Duff
    // source-over. Callers should call `box_shadow` first, then draw the widget
    // content on top, so the shadow appears underneath.
    let Color(sr, sg, sb, base_a) = color;
    let base_a_f = base_a as f32 / 255.0;
    let clip = ClipRect::full(fb.width(), fb.height());
    for row in 0..sh {
        let fy = sy + row as i64;
        if fy < clip.y0 || fy >= clip.y1 {
            continue;
        }
        for col in 0..sw {
            let fx = sx + col as i64;
            if fx < clip.x0 || fx >= clip.x1 {
                continue;
            }
            let a_val = alpha[row * sw + col];
            if a_val <= 0.0 {
                continue;
            }
            // Scale shadow alpha by the base colour's alpha.
            let effective_a = (a_val * base_a_f * 255.0).round() as u8;
            if effective_a == 0 {
                continue;
            }
            let px = fx as u32;
            let py = fy as u32;
            fb.blend(px, py, pack_rgba(sr, sg, sb, effective_a));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_sums_to_one() {
        let k = gaussian_kernel(4.0);
        let sum: f32 = k[0] + 2.0 * k[1..].iter().sum::<f32>();
        assert!((sum - 1.0).abs() < 0.01, "kernel sum = {sum}");
    }

    #[test]
    fn cache_returns_same_kernel() {
        let mut cache = GaussianCache::new();
        let k1 = cache.kernel(4.0).to_vec();
        let k2 = cache.kernel(4.0).to_vec();
        assert_eq!(k1, k2);
    }

    #[test]
    fn blur_spreads_alpha() {
        // A single bright pixel in the centre should spread outward after blur.
        let w = 9;
        let h = 9;
        let mut alpha = vec![0.0f32; w * h];
        alpha[4 * w + 4] = 1.0; // centre pixel
        let kernel = gaussian_kernel(2.0);
        gaussian_blur_alpha(&mut alpha, w, h, &kernel);
        // After blur, the adjacent pixels should be non-zero.
        assert!(alpha[4 * w + 5] > 0.0, "right neighbor should receive blur");
        assert!(alpha[3 * w + 4] > 0.0, "top neighbor should receive blur");
        // Centre should still be the maximum.
        assert!(alpha[4 * w + 4] >= alpha[4 * w + 5]);
    }

    #[test]
    fn shadow_energy_offset() {
        // Shadow with +5,+5 offset: pixel near (rect+offset) should have received
        // shadow energy; pixel far from the shadow should remain black.
        //
        // We use a transparent background so the shadow blends visibly.
        let mut fb = Framebuffer::with_fill(60, 60, oxiui_core::Color(0, 0, 0, 0));
        let mut cache = GaussianCache::new();
        box_shadow(
            &mut fb,
            (10.0, 10.0, 20.0, 20.0),
            5.0, // offset_x → shadow rect centred near (15+5, 15+5) = (20, 20)
            5.0, // offset_y
            3.0, // blur_radius
            Color(200, 200, 200, 255),
            &mut cache,
        );
        // Pixel inside the shadow rect (e.g., centre of the shadow @ approx 20+10, 20+10)
        // should be non-zero.
        let (r_near, _, _, _) = fb.get_rgba(25, 25).unwrap_or((0, 0, 0, 0));
        assert!(
            r_near > 0,
            "shadow center pixel should be non-zero (r={r_near})"
        );
        // Pixel far from shadow (top-left corner) should remain fully transparent.
        let (_, _, _, a_far) = fb.get_rgba(0, 0).unwrap_or((0, 0, 0, 99));
        assert_eq!(a_far, 0, "far corner should remain transparent (a={a_far})");
    }
}
