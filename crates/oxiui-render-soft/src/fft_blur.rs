//! FFT-accelerated Gaussian blur using OxiFFT (COOLJAPAN ecosystem).
//!
//! Enabled by the `fft-blur` feature.  Provides a drop-in replacement for the
//! separable 1-D direct-convolution path in [`crate::shadow`] that is
//! asymptotically faster for large kernels (radius > 32 pixels).
//!
//! ## Why FFT for blur?
//!
//! Direct convolution has O(N · K) complexity per pass where N is the signal
//! length (image width or height) and K is the kernel half-width.  FFT-based
//! convolution (`OxiFFT::convolve`) is O(N log N), independent of kernel
//! width.  For small kernels (K ≤ ~16) the constant factor makes the direct
//! path faster; the threshold to switch is configurable via
//! [`FFT_BLUR_MIN_RADIUS`].
//!
//! ## Framebuffer invariant
//!
//! Input and output are `[f32]` alpha buffers (same layout as
//! [`crate::shadow::gaussian_blur_alpha`]).  Pixel packing/unpacking is the
//! caller's responsibility.

#[cfg(feature = "fft-blur")]
use oxifft::{convolve_mode, ConvMode};

/// Kernel-radius threshold: use the FFT path when `blur_radius >= FFT_BLUR_MIN_RADIUS`.
///
/// Below this threshold the direct-convolution path in [`crate::shadow`] is faster.
pub const FFT_BLUR_MIN_RADIUS: f32 = 32.0;

/// Returns `true` if `blur_radius` is large enough to benefit from FFT convolution.
///
/// Call this before deciding whether to invoke [`gaussian_blur_alpha_fft`] or
/// the standard [`crate::shadow::gaussian_blur_alpha`].
#[inline]
pub fn should_use_fft_blur(blur_radius: f32) -> bool {
    blur_radius >= FFT_BLUR_MIN_RADIUS
}

// ---------------------------------------------------------------------------
// FFT-based separable Gaussian blur
// ---------------------------------------------------------------------------

/// Apply a separable Gaussian blur to an alpha buffer using FFT convolution.
///
/// This is a drop-in replacement for [`crate::shadow::gaussian_blur_alpha`] for
/// large kernels.  It performs a horizontal FFT convolution pass followed by a
/// vertical pass.
///
/// # Parameters
/// * `alpha` — mutable flat buffer of `f32` alpha values in `[0, 1]`, row-major,
///   length `width * height`.
/// * `width`, `height` — dimensions of the buffer.
/// * `kernel` — half-width 1-D Gaussian kernel (same layout as
///   [`crate::shadow::gaussian_kernel`]).
///
/// # Panics
/// Panics (debug only) if `alpha.len() != width * height`.
///
/// # Notes
/// Only available when the `fft-blur` feature is enabled.  On non-fft builds,
/// this function is a no-op and callers must fall back to the direct path.
#[cfg(feature = "fft-blur")]
pub fn gaussian_blur_alpha_fft(alpha: &mut [f32], width: usize, height: usize, kernel: &[f32]) {
    debug_assert_eq!(alpha.len(), width * height);
    if width == 0 || height == 0 || kernel.is_empty() {
        return;
    }

    // Build the full symmetric 1-D kernel from the half-width form.
    let full_kernel = build_full_kernel(kernel);

    // --- Horizontal pass ---
    let mut tmp = vec![0.0f32; width * height];
    for y in 0..height {
        let row = &alpha[y * width..(y + 1) * width];
        let result = convolve_mode::<f32>(row, &full_kernel, ConvMode::Same);
        let out_row = &mut tmp[y * width..(y + 1) * width];
        let copy_len = result.len().min(width);
        out_row[..copy_len].copy_from_slice(&result[..copy_len]);
    }

    // --- Vertical pass (operate on transposed rows = columns) ---
    // We transpose, convolve each "row" (which is a column of the original),
    // then transpose back to avoid strided memory accesses.
    let mut col_buf = vec![0.0f32; height];
    for x in 0..width {
        // Gather column into a contiguous buffer.
        for y in 0..height {
            col_buf[y] = tmp[y * width + x];
        }
        let result = convolve_mode::<f32>(&col_buf, &full_kernel, ConvMode::Same);
        // Scatter back.
        for y in 0..height {
            let v = if y < result.len() { result[y] } else { 0.0 };
            alpha[y * width + x] = v.clamp(0.0, 1.0);
        }
    }
}

/// Stub when `fft-blur` feature is disabled — callers should not reach this
/// when they check [`should_use_fft_blur`] correctly.
#[cfg(not(feature = "fft-blur"))]
#[allow(unused_variables)]
pub fn gaussian_blur_alpha_fft(alpha: &mut [f32], width: usize, height: usize, kernel: &[f32]) {
    // No-op: fall back to the direct path in `shadow.rs`.
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Expand a half-width kernel `[w0, w1, ..., wR]` into the full symmetric
/// kernel `[wR, ..., w1, w0, w1, ..., wR]` of length `2R+1`.
#[cfg_attr(not(feature = "fft-blur"), allow(dead_code))]
fn build_full_kernel(half: &[f32]) -> Vec<f32> {
    if half.is_empty() {
        return Vec::new();
    }
    let r = half.len() - 1; // radius
    let mut full = Vec::with_capacity(2 * r + 1);
    for i in (1..=r).rev() {
        full.push(half[i]);
    }
    full.extend_from_slice(half);
    full
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    // `gaussian_kernel` is only referenced by the `fft-blur`-gated tests below,
    // so gate the import identically to avoid an unused-import warning when the
    // feature is off.
    #[cfg(feature = "fft-blur")]
    use crate::shadow::gaussian_kernel;

    #[test]
    fn should_use_fft_blur_threshold() {
        assert!(!should_use_fft_blur(16.0));
        assert!(!should_use_fft_blur(31.9));
        assert!(should_use_fft_blur(32.0));
        assert!(should_use_fft_blur(64.0));
    }

    #[test]
    fn build_full_kernel_symmetric() {
        let half = vec![1.0f32, 0.5, 0.25];
        let full = build_full_kernel(&half);
        assert_eq!(full.len(), 5);
        // Should be symmetric: [0.25, 0.5, 1.0, 0.5, 0.25]
        assert!((full[0] - 0.25).abs() < 1e-6);
        assert!((full[2] - 1.0).abs() < 1e-6);
        assert!((full[4] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn build_full_kernel_single_tap() {
        let half = vec![1.0f32];
        let full = build_full_kernel(&half);
        assert_eq!(full.len(), 1);
        assert!((full[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn build_full_kernel_empty() {
        let full = build_full_kernel(&[]);
        assert!(full.is_empty());
    }

    #[cfg(feature = "fft-blur")]
    #[test]
    fn fft_blur_spreads_energy() {
        // A single bright pixel in the centre should spread outward.
        let w = 15;
        let h = 15;
        let mut alpha = vec![0.0f32; w * h];
        alpha[7 * w + 7] = 1.0; // centre pixel
        let kernel = gaussian_kernel(4.0);
        gaussian_blur_alpha_fft(&mut alpha, w, h, &kernel);
        // Adjacent pixels must receive non-zero energy.
        assert!(alpha[7 * w + 8] > 0.0, "right neighbour should be non-zero");
        assert!(alpha[6 * w + 7] > 0.0, "top neighbour should be non-zero");
        // Centre should be largest.
        assert!(alpha[7 * w + 7] >= alpha[7 * w + 8]);
    }

    #[cfg(feature = "fft-blur")]
    #[test]
    fn fft_blur_matches_direct_approximately() {
        use crate::shadow::gaussian_blur_alpha;

        let w = 17;
        let h = 17;
        let init = |buf: &mut Vec<f32>| {
            *buf = vec![0.0f32; w * h];
            buf[8 * w + 8] = 1.0; // centre pixel
        };

        let kernel = gaussian_kernel(6.0);

        let mut direct = Vec::new();
        init(&mut direct);
        gaussian_blur_alpha(&mut direct, w, h, &kernel);

        let mut fft = Vec::new();
        init(&mut fft);
        gaussian_blur_alpha_fft(&mut fft, w, h, &kernel);

        for (i, (&d, &f)) in direct.iter().zip(fft.iter()).enumerate() {
            assert!(
                (d - f).abs() < 0.02,
                "pixel {i}: direct={d:.4} fft={f:.4} differ by more than 2%"
            );
        }
    }

    #[cfg(feature = "fft-blur")]
    #[test]
    fn fft_blur_empty_alpha_noop() {
        let kernel = gaussian_kernel(4.0);
        let mut alpha: Vec<f32> = Vec::new();
        gaussian_blur_alpha_fft(&mut alpha, 0, 0, &kernel);
        // Just should not panic.
    }
}
