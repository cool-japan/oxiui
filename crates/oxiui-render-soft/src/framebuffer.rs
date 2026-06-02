//! CPU pixel framebuffer in `0xAARRGGBB` (non-premultiplied) format.

use crate::headless::RgbaBuffer;
use oxiui_core::Color;

/// Pack an [`oxiui_core::Color`] into a `0xAARRGGBB` `u32`.
pub fn pack(c: &Color) -> u32 {
    let Color(r, g, b, a) = *c;
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Unpack a `0xAARRGGBB` `u32` into `(r, g, b, a)` components.
pub fn unpack(px: u32) -> (u8, u8, u8, u8) {
    let a = ((px >> 24) & 0xFF) as u8;
    let r = ((px >> 16) & 0xFF) as u8;
    let g = ((px >> 8) & 0xFF) as u8;
    let b = (px & 0xFF) as u8;
    (r, g, b, a)
}

/// Pack raw `(r, g, b, a)` components into `0xAARRGGBB`.
pub fn pack_rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// A CPU-side pixel buffer storing `0xAARRGGBB` values row-major.
#[derive(Clone, Debug)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Framebuffer {
    /// Allocate a transparent (all-zero) framebuffer of the given size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0u32; (width * height) as usize],
        }
    }

    /// Allocate a framebuffer pre-filled with `color`.
    pub fn with_fill(width: u32, height: u32, color: Color) -> Self {
        Self {
            width,
            height,
            pixels: vec![pack(&color); (width * height) as usize],
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Borrow the raw pixel slice (`0xAARRGGBB`, row-major).
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Mutably borrow the raw pixel slice.
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    /// Fill the entire buffer with `color`, discarding existing content.
    pub fn clear(&mut self, color: Color) {
        let v = pack(&color);
        for px in &mut self.pixels {
            *px = v;
        }
    }

    #[inline]
    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x < self.width && y < self.height {
            Some((y * self.width + x) as usize)
        } else {
            None
        }
    }

    /// Get the packed pixel at `(x, y)`, or `None` if out of bounds.
    pub fn get(&self, x: u32, y: u32) -> Option<u32> {
        self.index(x, y).map(|i| self.pixels[i])
    }

    /// Get the `(r, g, b, a)` components at `(x, y)`, or `None` if out of bounds.
    pub fn get_rgba(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        self.get(x, y).map(unpack)
    }

    /// Set the packed pixel at `(x, y)` (overwrites; no blending). Out-of-bounds
    /// writes are ignored.
    pub fn set(&mut self, x: u32, y: u32, px: u32) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = px;
        }
    }

    /// Borrow row `y` as a slice, or `None` if out of bounds.
    pub fn row(&self, y: u32) -> Option<&[u32]> {
        if y < self.height {
            let start = (y * self.width) as usize;
            Some(&self.pixels[start..start + self.width as usize])
        } else {
            None
        }
    }

    /// Composite `src` over the existing pixel at `(x, y)` using straight-alpha
    /// source-over blending. Out-of-bounds writes are ignored.
    pub fn blend(&mut self, x: u32, y: u32, src: u32) {
        let i = match self.index(x, y) {
            Some(i) => i,
            None => return,
        };
        let (sr, sg, sb, sa) = unpack(src);
        if sa == 0 {
            return;
        }
        if sa == 255 {
            self.pixels[i] = src;
            return;
        }
        let (dr, dg, db, da) = unpack(self.pixels[i]);
        let sa_f = sa as f32 / 255.0;
        let da_f = da as f32 / 255.0;
        let out_a = sa_f + da_f * (1.0 - sa_f);
        if out_a <= 0.0 {
            self.pixels[i] = 0;
            return;
        }
        let blend_ch = |s: u8, d: u8| -> u8 {
            let s = s as f32 / 255.0;
            let d = d as f32 / 255.0;
            let v = (s * sa_f + d * da_f * (1.0 - sa_f)) / out_a;
            (v.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        let r = blend_ch(sr, dr);
        let g = blend_ch(sg, dg);
        let b = blend_ch(sb, db);
        let a = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        self.pixels[i] = pack_rgba(r, g, b, a);
    }

    /// Blend a colour `c` with an extra coverage factor `coverage` in `[0, 1]`
    /// (used for anti-aliased edges). The colour's own alpha is multiplied by
    /// `coverage` before compositing.
    pub fn blend_coverage(&mut self, x: u32, y: u32, c: &Color, coverage: f32) {
        let cov = coverage.clamp(0.0, 1.0);
        if cov <= 0.0 {
            return;
        }
        let Color(r, g, b, a) = *c;
        let a = (a as f32 * cov).round() as u8;
        self.blend(x, y, pack_rgba(r, g, b, a));
    }

    /// Convert to an [`RgbaBuffer`] (R, G, B, A bytes) suitable for PNG export.
    pub fn to_rgba_buffer(&self) -> RgbaBuffer {
        let mut data = Vec::with_capacity(self.pixels.len() * 4);
        for &px in &self.pixels {
            let (r, g, b, a) = unpack(px);
            data.extend_from_slice(&[r, g, b, a]);
        }
        RgbaBuffer {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let c = Color(10, 20, 30, 200);
        let packed = pack(&c);
        assert_eq!(unpack(packed), (10, 20, 30, 200));
    }

    #[test]
    fn fill_and_get() {
        let fb = Framebuffer::with_fill(4, 4, Color(255, 0, 0, 255));
        assert_eq!(fb.get_rgba(0, 0), Some((255, 0, 0, 255)));
        assert_eq!(fb.get_rgba(3, 3), Some((255, 0, 0, 255)));
        assert_eq!(fb.get(4, 4), None);
    }

    #[test]
    fn opaque_blend_overwrites() {
        let mut fb = Framebuffer::with_fill(2, 2, Color(0, 0, 0, 255));
        fb.blend(0, 0, pack(&Color(255, 255, 255, 255)));
        assert_eq!(fb.get_rgba(0, 0), Some((255, 255, 255, 255)));
    }

    #[test]
    fn half_alpha_blend() {
        // 50% white over opaque black => mid-grey, fully opaque.
        let mut fb = Framebuffer::with_fill(1, 1, Color(0, 0, 0, 255));
        fb.blend(0, 0, pack(&Color(255, 255, 255, 128)));
        let (r, g, b, a) = fb.get_rgba(0, 0).expect("pixel");
        assert!((120..=135).contains(&r), "r={r}");
        assert_eq!(g, r);
        assert_eq!(b, r);
        assert_eq!(a, 255);
    }

    #[test]
    fn transparent_blend_is_noop() {
        let mut fb = Framebuffer::with_fill(1, 1, Color(10, 20, 30, 255));
        fb.blend(0, 0, pack(&Color(255, 255, 255, 0)));
        assert_eq!(fb.get_rgba(0, 0), Some((10, 20, 30, 255)));
    }

    #[test]
    fn coverage_scales_alpha() {
        let mut fb = Framebuffer::with_fill(1, 1, Color(0, 0, 0, 255));
        // Full-opaque white at 0 coverage = no change.
        fb.blend_coverage(0, 0, &Color(255, 255, 255, 255), 0.0);
        assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 255)));
        // Full coverage = full overwrite.
        fb.blend_coverage(0, 0, &Color(255, 255, 255, 255), 1.0);
        assert_eq!(fb.get_rgba(0, 0), Some((255, 255, 255, 255)));
    }

    #[test]
    fn to_rgba_buffer_layout() {
        let fb = Framebuffer::with_fill(2, 1, Color(1, 2, 3, 4));
        let rgba = fb.to_rgba_buffer();
        assert_eq!(rgba.width, 2);
        assert_eq!(rgba.data, vec![1, 2, 3, 4, 1, 2, 3, 4]);
    }
}
