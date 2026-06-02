//! Drawing primitives operating on a [`Framebuffer`] through a clip stack.
//!
//! [`Canvas`] borrows a framebuffer and owns a [`ClipStack`]; all drawing is
//! clipped to the current effective clip. Shapes use straight-alpha source-over
//! compositing; circles and rounded rectangles use analytical coverage-based
//! anti-aliasing along their edges.

use crate::clip::{ClipRect, ClipStack};
use crate::framebuffer::Framebuffer;
use oxiui_core::Color;

/// Dash pattern for [`Canvas::draw_line_dashed`].
#[derive(Clone, Copy, Debug)]
pub struct DashPattern {
    /// Length of each dash in pixels (minimum 1.0).
    pub dash_len: f32,
    /// Length of each gap in pixels.
    pub gap_len: f32,
}

impl DashPattern {
    /// Construct a dash pattern.
    pub fn new(dash_len: f32, gap_len: f32) -> Self {
        Self {
            dash_len: dash_len.max(1.0),
            gap_len: gap_len.max(0.0),
        }
    }
}

/// A borrowed RGBA source image for [`Canvas::blit_rgba`].
///
/// `data` holds `width * height * 4` bytes in R, G, B, A order, row-major.
#[derive(Clone, Copy, Debug)]
pub struct SrcImage<'a> {
    /// Pixel bytes (`width * height * 4`, R,G,B,A per pixel, row-major).
    pub data: &'a [u8],
    /// Source width in pixels.
    pub width: u32,
    /// Source height in pixels.
    pub height: u32,
}

impl<'a> SrcImage<'a> {
    /// Construct a source image view from raw RGBA bytes.
    pub fn new(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data,
            width,
            height,
        }
    }

    /// Returns `true` if `data` is large enough for `width * height` RGBA pixels.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.data.len() >= (self.width * self.height * 4) as usize
    }
}

/// A clipped drawing surface over a [`Framebuffer`].
pub struct Canvas<'fb> {
    fb: &'fb mut Framebuffer,
    clip: ClipStack,
    /// When `true`, polygon / path fill operations use sub-pixel AA (vertical
    /// supersample coverage). Set via [`Canvas::set_aa`].
    pub aa: bool,
}

impl<'fb> Canvas<'fb> {
    /// Create a canvas over `fb` with a base clip covering the whole buffer.
    /// Anti-aliasing is **enabled** by default.
    pub fn new(fb: &'fb mut Framebuffer) -> Self {
        let clip = ClipStack::new(fb.width(), fb.height());
        Self { fb, clip, aa: true }
    }

    /// Set whether polygon / path fill operations use sub-pixel AA.
    ///
    /// When `false`, the `aa` flag passed to `fill_polygon_clipped` is `false`,
    /// producing aliased edges (faster).
    pub fn set_aa(&mut self, enabled: bool) {
        self.aa = enabled;
    }

    /// Borrow the underlying framebuffer.
    pub fn framebuffer(&self) -> &Framebuffer {
        self.fb
    }

    /// Push a rectangular clip (intersected with the current clip).
    pub fn push_clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let rect = ClipRect::from_rect(
            x.floor() as i64,
            y.floor() as i64,
            w.ceil() as i64,
            h.ceil() as i64,
        );
        self.clip.push(rect);
    }

    /// Pop the most recently pushed clip.
    pub fn pop_clip(&mut self) {
        self.clip.pop();
    }

    #[inline]
    fn put(&mut self, x: i64, y: i64, color: &Color, coverage: f32) {
        if !self.clip.current().contains(x, y) {
            return;
        }
        if x < 0 || y < 0 {
            return;
        }
        self.fb.blend_coverage(x as u32, y as u32, color, coverage);
    }

    /// Fill an axis-aligned rectangle with a solid colour (no AA; pixel-snapped).
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let x1 = (x + w).ceil() as i64;
        let y1 = (y + h).ceil() as i64;
        let clip = self.clip.current();
        let cx0 = x0.max(clip.x0);
        let cy0 = y0.max(clip.y0);
        let cx1 = x1.min(clip.x1);
        let cy1 = y1.min(clip.y1);
        for py in cy0..cy1 {
            for px in cx0..cx1 {
                if px >= 0 && py >= 0 {
                    self.fb
                        .blend(px as u32, py as u32, crate::framebuffer::pack(&color));
                }
            }
        }
    }

    /// Stroke a rectangle outline of the given `thickness` (drawn inward).
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, thickness: f32, color: Color) {
        let t = thickness.max(1.0);
        // Top, bottom, left, right bars.
        self.fill_rect(x, y, w, t, color);
        self.fill_rect(x, y + h - t, w, t, color);
        self.fill_rect(x, y, t, h, color);
        self.fill_rect(x + w - t, y, t, h, color);
    }

    /// Draw a 1-pixel line from `(x0, y0)` to `(x1, y1)` using Bresenham's
    /// algorithm (no anti-aliasing).
    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color) {
        let mut x0 = x0.round() as i64;
        let mut y0 = y0.round() as i64;
        let x1 = x1.round() as i64;
        let y1 = y1.round() as i64;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.put(x0, y0, &color, 1.0);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Fill an anti-aliased circle centred at `(cx, cy)` with `radius`.
    ///
    /// Coverage is estimated from the signed distance to the circle edge,
    /// giving a smooth one-pixel-wide anti-aliased boundary.
    pub fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, color: Color) {
        if radius <= 0.0 {
            return;
        }
        let x0 = (cx - radius - 1.0).floor() as i64;
        let y0 = (cy - radius - 1.0).floor() as i64;
        let x1 = (cx + radius + 1.0).ceil() as i64;
        let y1 = (cy + radius + 1.0).ceil() as i64;
        for py in y0..y1 {
            for px in x0..x1 {
                // Sample at pixel centre.
                let sx = px as f32 + 0.5;
                let sy = py as f32 + 0.5;
                let dist = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                let coverage = (radius - dist + 0.5).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.put(px, py, &color, coverage);
                }
            }
        }
    }

    /// Fill a rounded rectangle with a uniform corner `radius` and AA edges.
    pub fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = radius.clamp(0.0, (w.min(h)) * 0.5);
        if r <= 0.0 {
            self.fill_rect(x, y, w, h, color);
            return;
        }
        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let x1 = (x + w).ceil() as i64;
        let y1 = (y + h).ceil() as i64;
        // Corner circle centres.
        let left = x + r;
        let right = x + w - r;
        let top = y + r;
        let bottom = y + h - r;
        for py in y0..y1 {
            for px in x0..x1 {
                let sx = px as f32 + 0.5;
                let sy = py as f32 + 0.5;
                // Distance into the nearest corner region, if any.
                let dx = if sx < left {
                    left - sx
                } else if sx > right {
                    sx - right
                } else {
                    0.0
                };
                let dy = if sy < top {
                    top - sy
                } else if sy > bottom {
                    sy - bottom
                } else {
                    0.0
                };
                let coverage = if dx == 0.0 || dy == 0.0 {
                    // Straight edge or interior: inside rect bounds fully covered.
                    if sx >= x && sx <= x + w && sy >= y && sy <= y + h {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    let dist = (dx * dx + dy * dy).sqrt();
                    (r - dist + 0.5).clamp(0.0, 1.0)
                };
                if coverage > 0.0 {
                    self.put(px, py, &color, coverage);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Wu's anti-aliased line
    // -----------------------------------------------------------------------

    /// Draw an anti-aliased line from `(x0, y0)` to `(x1, y1)` using Wu's
    /// algorithm (float pixel coverage).
    pub fn draw_line_wu(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color) {
        wu_line(self.fb, &self.clip, x0, y0, x1, y1, &color);
    }

    /// Draw a thick anti-aliased line of the given `width` using a parallel-offset
    /// rectangle polygon fill.
    pub fn draw_line_thick(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        width: f32,
        color: Color,
    ) {
        if width <= 1.0 {
            self.draw_line_wu(x0, y0, x1, y1, color);
            return;
        }
        let half = width * 0.5;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        let nx = -dy / len * half;
        let ny = dx / len * half;
        let pts = [
            (x0 + nx, y0 + ny),
            (x1 + nx, y1 + ny),
            (x1 - nx, y1 - ny),
            (x0 - nx, y0 - ny),
        ];
        // Clip-aware fill using scanline.
        let clip = self.clip.current();
        crate::scanline::fill_polygon_clipped(
            self.fb,
            &pts,
            color,
            crate::scanline::FillRule::NonZero,
            true,
            clip,
        );
    }

    /// Draw a dashed line from `(x0, y0)` to `(x1, y1)` using `pattern`.
    ///
    /// Dashes and gaps alternate until the endpoint is reached.
    pub fn draw_line_dashed(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        color: Color,
        pattern: DashPattern,
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let total_len = (dx * dx + dy * dy).sqrt();
        if total_len < f32::EPSILON {
            return;
        }
        let ux = dx / total_len;
        let uy = dy / total_len;
        let mut t = 0.0f32;
        let mut drawing = true;
        while t < total_len {
            let seg_len = if drawing {
                pattern.dash_len
            } else {
                pattern.gap_len
            };
            let end_t = (t + seg_len).min(total_len);
            if drawing && end_t > t {
                let sx = x0 + ux * t;
                let sy = y0 + uy * t;
                let ex = x0 + ux * end_t;
                let ey = y0 + uy * end_t;
                wu_line(self.fb, &self.clip, sx, sy, ex, ey, &color);
            }
            t += seg_len;
            if t >= total_len {
                break;
            }
            drawing = !drawing;
        }
    }

    // -----------------------------------------------------------------------
    // Ellipse
    // -----------------------------------------------------------------------

    /// Fill an axis-aligned anti-aliased ellipse centred at `(cx, cy)` with
    /// semi-axes `rx` (horizontal) and `ry` (vertical).
    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: Color) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let x0 = (cx - rx - 1.0).floor() as i64;
        let y0 = (cy - ry - 1.0).floor() as i64;
        let x1 = (cx + rx + 1.0).ceil() as i64;
        let y1 = (cy + ry + 1.0).ceil() as i64;
        let clip = self.clip.current();
        for py in y0..y1 {
            for px in x0..x1 {
                if !clip.contains(px, py) {
                    continue;
                }
                let sx = px as f32 + 0.5;
                let sy = py as f32 + 0.5;
                let ex = (sx - cx) / rx;
                let ey = (sy - cy) / ry;
                let d = (ex * ex + ey * ey).sqrt();
                // Signed-distance approximation: distance to ellipse boundary ≈
                // (d - 1.0) * effective_radius, where effective_radius is the
                // minor/major axis harmonic mean in that direction.
                let eff_r = 1.0 / (ex.abs() / rx + ey.abs() / ry + f32::EPSILON);
                let coverage = (1.0 - d + 0.5 / eff_r.max(1.0)).clamp(0.0, 1.0);
                if coverage > 0.0 && px >= 0 && py >= 0 {
                    self.fb
                        .blend_coverage(px as u32, py as u32, &color, coverage);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Per-corner rounded rect
    // -----------------------------------------------------------------------

    /// Fill a rounded rectangle where each corner may have a different radius.
    ///
    /// `radii` = `[top_left, top_right, bottom_right, bottom_left]`.
    pub fn fill_rounded_rect_per_corner(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radii: [f32; 4],
        color: Color,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        // Clamp radii so they don't exceed half the corresponding dimension.
        let half_w = w * 0.5;
        let half_h = h * 0.5;
        let r_tl = radii[0].clamp(0.0, half_w.min(half_h));
        let r_tr = radii[1].clamp(0.0, half_w.min(half_h));
        let r_br = radii[2].clamp(0.0, half_w.min(half_h));
        let r_bl = radii[3].clamp(0.0, half_w.min(half_h));

        let x0 = x.floor() as i64;
        let y0 = y.floor() as i64;
        let x1 = (x + w).ceil() as i64;
        let y1 = (y + h).ceil() as i64;
        let clip = self.clip.current();

        // Corner circle centres.
        let c_tl = (x + r_tl, y + r_tl);
        let c_tr = (x + w - r_tr, y + r_tr);
        let c_br = (x + w - r_br, y + h - r_br);
        let c_bl = (x + r_bl, y + h - r_bl);

        let corner_params = CornerCoverageParams {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
            c_tl,
            c_tr,
            c_br,
            c_bl,
            r_tl,
            r_tr,
            r_br,
            r_bl,
        };
        for py in y0..y1 {
            for px in x0..x1 {
                if !clip.contains(px, py) {
                    continue;
                }
                let sx = px as f32 + 0.5;
                let sy = py as f32 + 0.5;
                let coverage = per_corner_coverage(sx, sy, &corner_params);
                if coverage > 0.0 && px >= 0 && py >= 0 {
                    self.fb
                        .blend_coverage(px as u32, py as u32, &color, coverage);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Bilinear image scaling
    // -----------------------------------------------------------------------

    /// Blit an RGBA source image at destination position `(dx, dy)` with
    /// source-over blending and nearest-neighbour scaling to `dst_w × dst_h`.
    pub fn blit_rgba(&mut self, src: SrcImage<'_>, dx: f32, dy: f32, dst_w: u32, dst_h: u32) {
        if !src.is_valid() || dst_w == 0 || dst_h == 0 {
            return;
        }
        let src_w = src.width;
        let src_h = src.height;
        let ox = dx.round() as i64;
        let oy = dy.round() as i64;
        for j in 0..dst_h {
            for i in 0..dst_w {
                // Map destination pixel back to a source sample.
                let su = (i * src_w) / dst_w;
                let sv = (j * src_h) / dst_h;
                let si = ((sv * src_w + su) * 4) as usize;
                let r = src.data[si];
                let g = src.data[si + 1];
                let b = src.data[si + 2];
                let a = src.data[si + 3];
                if a == 0 {
                    continue;
                }
                let px = ox + i as i64;
                let py = oy + j as i64;
                if px < 0 || py < 0 || !self.clip.current().contains(px, py) {
                    continue;
                }
                self.fb.blend(
                    px as u32,
                    py as u32,
                    crate::framebuffer::pack_rgba(r, g, b, a),
                );
            }
        }
    }
    /// Blit an RGBA source image at destination position `(dx, dy)` with
    /// source-over blending and bilinear interpolation scaled to `dst_w × dst_h`.
    pub fn blit_bilinear(&mut self, src: SrcImage<'_>, dx: f32, dy: f32, dst_w: u32, dst_h: u32) {
        if !src.is_valid() || dst_w == 0 || dst_h == 0 {
            return;
        }
        let src_w = src.width;
        let src_h = src.height;
        let ox = dx.round() as i64;
        let oy = dy.round() as i64;
        let clip = self.clip.current();

        for j in 0..dst_h {
            for i in 0..dst_w {
                let px = ox + i as i64;
                let py = oy + j as i64;
                if !clip.contains(px, py) || px < 0 || py < 0 {
                    continue;
                }
                // Bilinear sample position in source space.
                let u = (i as f32 + 0.5) * src_w as f32 / dst_w as f32 - 0.5;
                let v = (j as f32 + 0.5) * src_h as f32 / dst_h as f32 - 0.5;
                let x0 = u.floor() as i32;
                let y0 = v.floor() as i32;
                let fx = u - x0 as f32;
                let fy = v - y0 as f32;

                let sample = |sx: i32, sy: i32| -> [f32; 4] {
                    let sx = sx.clamp(0, src_w as i32 - 1) as u32;
                    let sy = sy.clamp(0, src_h as i32 - 1) as u32;
                    let idx = ((sy * src_w + sx) * 4) as usize;
                    [
                        src.data[idx] as f32,
                        src.data[idx + 1] as f32,
                        src.data[idx + 2] as f32,
                        src.data[idx + 3] as f32,
                    ]
                };

                let s00 = sample(x0, y0);
                let s10 = sample(x0 + 1, y0);
                let s01 = sample(x0, y0 + 1);
                let s11 = sample(x0 + 1, y0 + 1);

                let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
                let r = lerp(lerp(s00[0], s10[0], fx), lerp(s01[0], s11[0], fx), fy);
                let g = lerp(lerp(s00[1], s10[1], fx), lerp(s01[1], s11[1], fx), fy);
                let b = lerp(lerp(s00[2], s10[2], fx), lerp(s01[2], s11[2], fx), fy);
                let a = lerp(lerp(s00[3], s10[3], fx), lerp(s01[3], s11[3], fx), fy);

                if a <= 0.0 {
                    continue;
                }
                self.fb.blend(
                    px as u32,
                    py as u32,
                    crate::framebuffer::pack_rgba(
                        r.round().clamp(0.0, 255.0) as u8,
                        g.round().clamp(0.0, 255.0) as u8,
                        b.round().clamp(0.0, 255.0) as u8,
                        a.round().clamp(0.0, 255.0) as u8,
                    ),
                );
            }
        }
    }

    /// Nine-slice blit: corners are copied unscaled; edges stretch in one axis;
    /// centre stretches in both axes.
    ///
    /// `insets` = `[top, right, bottom, left]` in source pixels.
    pub fn blit_nine_slice(
        &mut self,
        src: SrcImage<'_>,
        dx: f32,
        dy: f32,
        dst_w: u32,
        dst_h: u32,
        insets: [u32; 4],
    ) {
        if !src.is_valid() || dst_w == 0 || dst_h == 0 {
            return;
        }
        let [ins_t, ins_r, ins_b, ins_l] = insets;
        let sw = src.width;
        let sh = src.height;

        // Ensure insets don't exceed source dimensions.
        let ins_l = ins_l.min(sw / 2);
        let ins_r = ins_r.min(sw / 2);
        let ins_t = ins_t.min(sh / 2);
        let ins_b = ins_b.min(sh / 2);

        // Destination insets (same as source if dst is large enough).
        let dst_l = ins_l.min(dst_w / 2);
        let dst_r = ins_r.min(dst_w / 2);
        let dst_t = ins_t.min(dst_h / 2);
        let dst_b = ins_b.min(dst_h / 2);

        let ox = dx.round() as i64;
        let oy = dy.round() as i64;

        // Source slice boundaries.
        let sx_segs = [0, ins_l, sw - ins_r, sw];
        let sy_segs = [0, ins_t, sh - ins_b, sh];

        // Destination slice boundaries.
        let dx_segs = [0u32, dst_l, dst_w - dst_r, dst_w];
        let dy_segs = [0u32, dst_t, dst_h - dst_b, dst_h];

        for row in 0..3usize {
            for col in 0..3usize {
                let src_x0 = sx_segs[col];
                let src_x1 = sx_segs[col + 1];
                let src_y0 = sy_segs[row];
                let src_y1 = sy_segs[row + 1];
                let src_patch_w = src_x1.saturating_sub(src_x0);
                let src_patch_h = src_y1.saturating_sub(src_y0);

                let dst_x0 = dx_segs[col];
                let dst_x1 = dx_segs[col + 1];
                let dst_y0 = dy_segs[row];
                let dst_y1 = dy_segs[row + 1];
                let dst_patch_w = dst_x1.saturating_sub(dst_x0);
                let dst_patch_h = dst_y1.saturating_sub(dst_y0);

                if src_patch_w == 0 || src_patch_h == 0 || dst_patch_w == 0 || dst_patch_h == 0 {
                    continue;
                }

                // Build a sub-image for this patch and blit it bilinearly.
                let mut patch = Vec::with_capacity((src_patch_w * src_patch_h * 4) as usize);
                for sy in src_y0..src_y1 {
                    for sxi in src_x0..src_x1 {
                        let idx = ((sy * sw + sxi) * 4) as usize;
                        patch.extend_from_slice(&src.data[idx..idx + 4]);
                    }
                }
                let patch_img = SrcImage::new(&patch, src_patch_w, src_patch_h);
                let dst_abs_x = ox + dst_x0 as i64;
                let dst_abs_y = oy + dst_y0 as i64;
                self.blit_bilinear(
                    patch_img,
                    dst_abs_x as f32,
                    dst_abs_y as f32,
                    dst_patch_w,
                    dst_patch_h,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // DrawList command dispatch helpers (clip-aware)
    // -----------------------------------------------------------------------

    /// Fill a [`oxiui_core::paint::PathData`] using the current clip. Converts verbs to a soft [`crate::path::Path`].
    pub fn fill_path(&mut self, path: &oxiui_core::paint::PathData, color: Color) {
        use crate::path::Path;
        use crate::scanline::FillRule as ScanFillRule;
        let fill_rule = match path.fill_rule {
            oxiui_core::paint::FillRule::EvenOdd => ScanFillRule::EvenOdd,
            oxiui_core::paint::FillRule::NonZero => ScanFillRule::NonZero,
        };
        let mut p = Path::new().with_fill_rule(fill_rule);
        replay_path_verbs(&mut p, path);
        p.fill_clipped_aa(self.fb, color, self.clip.current(), self.aa);
    }

    /// Stroke a [`oxiui_core::paint::PathData`] using the current clip and stroke style.
    pub fn stroke_path(
        &mut self,
        path: &oxiui_core::paint::PathData,
        style: &oxiui_core::paint::StrokeStyle,
        color: Color,
    ) {
        use crate::path::{Cap, Join, Path, StrokeStyle as PathStroke};
        let mut p = Path::new();
        replay_path_verbs(&mut p, path);
        let ss = PathStroke {
            width: style.width,
            join: match style.join {
                oxiui_core::paint::LineJoin::Miter => Join::Miter,
                oxiui_core::paint::LineJoin::Bevel => Join::Bevel,
                oxiui_core::paint::LineJoin::Round => Join::Round,
            },
            cap: match style.cap {
                oxiui_core::paint::LineCap::Butt => Cap::Butt,
                oxiui_core::paint::LineCap::Round => Cap::Round,
                oxiui_core::paint::LineCap::Square => Cap::Square,
            },
            miter_limit: style.miter_limit,
        };
        p.stroke_clipped_aa(self.fb, &ss, color, self.clip.current(), self.aa);
    }

    /// Fill a rectangle with a linear gradient, respecting the current clip.
    pub fn fill_linear_gradient_cmd(
        &mut self,
        rect: oxiui_core::geometry::Rect,
        start: oxiui_core::geometry::Point,
        end: oxiui_core::geometry::Point,
        stops: &[oxiui_core::paint::GradientStop],
    ) {
        use crate::gradient::{GradientStop as SoftStop, LinearGradient};
        let soft_stops: Vec<SoftStop> = stops
            .iter()
            .map(|s| SoftStop {
                offset: s.offset,
                color: s.color,
            })
            .collect();
        let g = LinearGradient::new((start.x, start.y), (end.x, end.y), soft_stops);
        let clip = self.clip.current();
        g.fill_rect(
            self.fb,
            &clip,
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
        );
    }

    /// Fill a rectangle with a radial gradient, respecting the current clip.
    pub fn fill_radial_gradient_cmd(
        &mut self,
        rect: oxiui_core::geometry::Rect,
        center: oxiui_core::geometry::Point,
        radius: f32,
        stops: &[oxiui_core::paint::GradientStop],
    ) {
        use crate::gradient::{GradientStop as SoftStop, RadialGradient};
        let soft_stops: Vec<SoftStop> = stops
            .iter()
            .map(|s| SoftStop {
                offset: s.offset,
                color: s.color,
            })
            .collect();
        let g = RadialGradient::new((center.x, center.y), radius, soft_stops);
        let clip = self.clip.current();
        g.fill_rect(
            self.fb,
            &clip,
            rect.left(),
            rect.top(),
            rect.width(),
            rect.height(),
        );
    }

    /// Draw a box shadow. The shadow composites into the full framebuffer
    /// (no clip applied — consistent with the existing `box_shadow` function).
    pub fn box_shadow_cmd(
        &mut self,
        rect: oxiui_core::geometry::Rect,
        offset: oxiui_core::geometry::Point,
        blur_radius: f32,
        color: Color,
        cache: &mut crate::shadow::GaussianCache,
    ) {
        crate::shadow::box_shadow(
            self.fb,
            (rect.left(), rect.top(), rect.width(), rect.height()),
            offset.x,
            offset.y,
            blur_radius,
            color,
            cache,
        );
    }

    // -----------------------------------------------------------------------
    // Bézier convenience methods (delegate to Path)
    // -----------------------------------------------------------------------

    /// Draw a quadratic Bézier curve from `p0` via `ctrl` to `p2`.
    pub fn draw_quad_bezier(
        &mut self,
        p0: (f32, f32),
        ctrl: (f32, f32),
        p2: (f32, f32),
        color: Color,
    ) {
        let pts = crate::path::flatten_quad_bezier(p0, ctrl, p2, 0.25);
        for i in 1..pts.len() {
            wu_line(
                self.fb,
                &self.clip,
                pts[i - 1].0,
                pts[i - 1].1,
                pts[i].0,
                pts[i].1,
                &color,
            );
        }
    }

    /// Draw a cubic Bézier curve from `p0` via `c1`, `c2` to `p3`.
    pub fn draw_cubic_bezier(
        &mut self,
        p0: (f32, f32),
        c1: (f32, f32),
        c2: (f32, f32),
        p3: (f32, f32),
        color: Color,
    ) {
        let pts = crate::path::flatten_cubic_bezier(p0, c1, c2, p3, 0.25);
        for i in 1..pts.len() {
            wu_line(
                self.fb,
                &self.clip,
                pts[i - 1].0,
                pts[i - 1].1,
                pts[i].0,
                pts[i].1,
                &color,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Path replay helper
// ---------------------------------------------------------------------------

/// Replay [`oxiui_core::paint::PathVerb`] commands into a soft-render [`crate::path::Path`].
fn replay_path_verbs(p: &mut crate::path::Path, path: &oxiui_core::paint::PathData) {
    use oxiui_core::paint::PathVerb;
    for verb in &path.verbs {
        match verb {
            PathVerb::MoveTo(pt) => {
                p.move_to((pt.x, pt.y));
            }
            PathVerb::LineTo(pt) => {
                p.line_to((pt.x, pt.y));
            }
            PathVerb::QuadTo { ctrl, end } => {
                p.quad_to((ctrl.x, ctrl.y), (end.x, end.y));
            }
            PathVerb::CubicTo { c1, c2, end } => {
                p.cubic_to((c1.x, c1.y), (c2.x, c2.y), (end.x, end.y));
            }
            PathVerb::Close => {
                p.close();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Module-level helpers (private)
// ---------------------------------------------------------------------------

/// Wu's anti-aliased line algorithm (float pixel coverage).
fn wu_line(
    fb: &mut crate::framebuffer::Framebuffer,
    clip: &crate::clip::ClipStack,
    mut x0: f32,
    mut y0: f32,
    mut x1: f32,
    mut y1: f32,
    color: &Color,
) {
    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    if steep {
        core::mem::swap(&mut x0, &mut y0);
        core::mem::swap(&mut x1, &mut y1);
    }
    if x0 > x1 {
        core::mem::swap(&mut x0, &mut x1);
        core::mem::swap(&mut y0, &mut y1);
    }
    let dx = x1 - x0;
    let dy = y1 - y0;
    let gradient = if dx.abs() < f32::EPSILON {
        1.0
    } else {
        dy / dx
    };

    let put_wu = |fb: &mut crate::framebuffer::Framebuffer,
                  clip: &crate::clip::ClipStack,
                  x: i64,
                  y: i64,
                  c: &Color,
                  alpha: f32| {
        if !clip.current().contains(x, y) || x < 0 || y < 0 {
            return;
        }
        fb.blend_coverage(x as u32, y as u32, c, alpha);
    };

    // First endpoint.
    let xend = x0.round();
    let yend = y0 + gradient * (xend - x0);
    let xgap = 1.0 - (x0 + 0.5).fract();
    let xpxl1 = xend as i64;
    let ypxl1 = yend.floor() as i64;
    if steep {
        put_wu(fb, clip, ypxl1, xpxl1, color, (1.0 - yend.fract()) * xgap);
        put_wu(fb, clip, ypxl1 + 1, xpxl1, color, yend.fract() * xgap);
    } else {
        put_wu(fb, clip, xpxl1, ypxl1, color, (1.0 - yend.fract()) * xgap);
        put_wu(fb, clip, xpxl1, ypxl1 + 1, color, yend.fract() * xgap);
    }
    let mut intery = yend + gradient;

    // Second endpoint.
    let xend = x1.round();
    let yend = y1 + gradient * (xend - x1);
    let xgap = (x1 + 0.5).fract();
    let xpxl2 = xend as i64;
    let ypxl2 = yend.floor() as i64;
    if steep {
        put_wu(fb, clip, ypxl2, xpxl2, color, (1.0 - yend.fract()) * xgap);
        put_wu(fb, clip, ypxl2 + 1, xpxl2, color, yend.fract() * xgap);
    } else {
        put_wu(fb, clip, xpxl2, ypxl2, color, (1.0 - yend.fract()) * xgap);
        put_wu(fb, clip, xpxl2, ypxl2 + 1, color, yend.fract() * xgap);
    }

    // Interior.
    for x in (xpxl1 + 1)..xpxl2 {
        let iy = intery.floor() as i64;
        let frac = intery.fract();
        if steep {
            put_wu(fb, clip, iy, x, color, 1.0 - frac);
            put_wu(fb, clip, iy + 1, x, color, frac);
        } else {
            put_wu(fb, clip, x, iy, color, 1.0 - frac);
            put_wu(fb, clip, x, iy + 1, color, frac);
        }
        intery += gradient;
    }
}

/// Parameters for per-corner rounded-rect coverage computation.
struct CornerCoverageParams {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    c_tl: (f32, f32),
    c_tr: (f32, f32),
    c_br: (f32, f32),
    c_bl: (f32, f32),
    r_tl: f32,
    r_tr: f32,
    r_br: f32,
    r_bl: f32,
}

/// Compute pixel coverage for per-corner rounded-rect at sample position `(sx, sy)`.
fn per_corner_coverage(sx: f32, sy: f32, p: &CornerCoverageParams) -> f32 {
    let CornerCoverageParams {
        left,
        top,
        right,
        bottom,
        c_tl,
        c_tr,
        c_br,
        c_bl,
        r_tl,
        r_tr,
        r_br,
        r_bl,
    } = p;
    // Determine which quadrant of the rect the sample is in.
    let in_left_half = sx < (left + right) * 0.5;
    let in_top_half = sy < (top + bottom) * 0.5;

    // Check corner regions.
    if in_left_half && in_top_half && *r_tl > 0.0 && sx < c_tl.0 && sy < c_tl.1 {
        // Top-left corner.
        let dx = c_tl.0 - sx;
        let dy = c_tl.1 - sy;
        let dist = (dx * dx + dy * dy).sqrt();
        return (r_tl - dist + 0.5).clamp(0.0, 1.0);
    }
    if !in_left_half && in_top_half && *r_tr > 0.0 && sx > c_tr.0 && sy < c_tr.1 {
        // Top-right corner.
        let dx = sx - c_tr.0;
        let dy = c_tr.1 - sy;
        let dist = (dx * dx + dy * dy).sqrt();
        return (r_tr - dist + 0.5).clamp(0.0, 1.0);
    }
    if !in_left_half && !in_top_half && *r_br > 0.0 && sx > c_br.0 && sy > c_br.1 {
        // Bottom-right corner.
        let dx = sx - c_br.0;
        let dy = sy - c_br.1;
        let dist = (dx * dx + dy * dy).sqrt();
        return (r_br - dist + 0.5).clamp(0.0, 1.0);
    }
    if in_left_half && !in_top_half && *r_bl > 0.0 && sx < c_bl.0 && sy > c_bl.1 {
        // Bottom-left corner.
        let dx = c_bl.0 - sx;
        let dy = sy - c_bl.1;
        let dist = (dx * dx + dy * dy).sqrt();
        return (r_bl - dist + 0.5).clamp(0.0, 1.0);
    }

    // Interior or edge — inside if within the bounding rect.
    if sx >= *left && sx <= *right && sy >= *top && sy <= *bottom {
        1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;

    fn fresh(w: u32, h: u32) -> Framebuffer {
        Framebuffer::with_fill(w, h, Color(0, 0, 0, 255))
    }

    #[test]
    fn fill_rect_paints_inside_only() {
        let mut fb = fresh(10, 10);
        {
            let mut c = Canvas::new(&mut fb);
            c.fill_rect(2.0, 2.0, 4.0, 4.0, Color(255, 0, 0, 255));
        }
        assert_eq!(fb.get_rgba(3, 3), Some((255, 0, 0, 255)));
        assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 255)));
        assert_eq!(fb.get_rgba(6, 6), Some((0, 0, 0, 255)));
    }

    #[test]
    fn clip_prevents_drawing() {
        let mut fb = fresh(10, 10);
        {
            let mut c = Canvas::new(&mut fb);
            c.push_clip(0.0, 0.0, 3.0, 3.0);
            // Try to paint the whole buffer; only the 3x3 clip region changes.
            c.fill_rect(0.0, 0.0, 10.0, 10.0, Color(0, 255, 0, 255));
            c.pop_clip();
        }
        assert_eq!(fb.get_rgba(1, 1), Some((0, 255, 0, 255)));
        assert_eq!(fb.get_rgba(5, 5), Some((0, 0, 0, 255)));
    }

    #[test]
    fn draw_line_horizontal_and_diagonal() {
        let mut fb = fresh(10, 10);
        {
            let mut c = Canvas::new(&mut fb);
            c.draw_line(0.0, 0.0, 9.0, 0.0, Color(255, 255, 255, 255));
            c.draw_line(0.0, 0.0, 9.0, 9.0, Color(255, 0, 0, 255));
        }
        assert_eq!(fb.get_rgba(5, 0), Some((255, 255, 255, 255)));
        assert_eq!(fb.get_rgba(5, 5), Some((255, 0, 0, 255)));
        assert_eq!(fb.get_rgba(9, 9), Some((255, 0, 0, 255)));
    }

    #[test]
    fn circle_center_filled_edge_antialiased() {
        let mut fb = fresh(20, 20);
        {
            let mut c = Canvas::new(&mut fb);
            c.fill_circle(10.0, 10.0, 6.0, Color(255, 255, 255, 255));
        }
        // Centre is fully covered (white).
        assert_eq!(fb.get_rgba(10, 10), Some((255, 255, 255, 255)));
        // Far corner untouched (black).
        assert_eq!(fb.get_rgba(0, 0), Some((0, 0, 0, 255)));
        // An edge pixel should be a partial blend (grey, not pure black/white).
        let (r, _, _, _) = fb.get_rgba(10, 4).expect("edge pixel near top of circle");
        assert!(r > 0, "edge pixel should have received some coverage");
    }

    #[test]
    fn rounded_rect_corner_softer_than_center() {
        let mut fb = fresh(40, 40);
        {
            let mut c = Canvas::new(&mut fb);
            c.fill_rounded_rect(5.0, 5.0, 30.0, 30.0, 8.0, Color(255, 255, 255, 255));
        }
        // Centre filled.
        assert_eq!(fb.get_rgba(20, 20), Some((255, 255, 255, 255)));
        // The extreme corner of the bounding box is outside the rounded corner.
        assert_eq!(fb.get_rgba(5, 5), Some((0, 0, 0, 255)));
    }

    #[test]
    fn fill_rect_color_matches() {
        let mut fb = fresh(10, 10);
        {
            let mut c = Canvas::new(&mut fb);
            c.fill_rect(0.0, 0.0, 10.0, 10.0, Color(200, 100, 50, 255));
        }
        // Sample the centre pixel.
        let (r, g, b, a) = fb.get_rgba(5, 5).expect("center pixel");
        assert_eq!((r, g, b, a), (200, 100, 50, 255));
    }

    #[test]
    fn rounded_rect_corners_aa() {
        let mut fb = fresh(30, 30);
        {
            let mut c = Canvas::new(&mut fb);
            c.fill_rounded_rect(2.0, 2.0, 26.0, 26.0, 8.0, Color(255, 255, 255, 255));
        }
        // Corner pixel at (2, 2) should be partially blended (alpha < 255 from background black).
        let (r, _, _, _) = fb.get_rgba(2, 2).expect("corner pixel");
        // The corner (2, 2) is at the extreme top-left; it should have less than full coverage
        // since the circle centre is at (10, 10) and r=8.
        assert!(r < 255, "corner should be anti-aliased (r={r})");
    }

    #[test]
    fn wu_line_edge_alpha() {
        // A 45-degree line should produce pixels with intermediate alpha.
        let mut fb = fresh(20, 20);
        {
            let mut c = Canvas::new(&mut fb);
            c.draw_line_wu(0.0, 0.3, 10.0, 10.3, Color(255, 255, 255, 255));
        }
        // Check that at least one pixel has non-full, non-zero alpha.
        let mut found_partial = false;
        for y in 0..20 {
            for x in 0..20 {
                let (r, _, _, _) = fb.get_rgba(x, y).unwrap_or((0, 0, 0, 0));
                if r > 0 && r < 255 {
                    found_partial = true;
                }
            }
        }
        assert!(
            found_partial,
            "Wu line should produce partially-alpha pixels"
        );
    }

    #[test]
    fn thick_line_covers_area() {
        let mut fb = fresh(30, 30);
        {
            let mut c = Canvas::new(&mut fb);
            // Horizontal line at y=15, width=8 → half=4, covers y∈[11,19).
            c.draw_line_thick(0.0, 15.0, 29.0, 15.0, 8.0, Color(255, 0, 0, 255));
        }
        // Pixels well within the band should be red.
        let (r12, _, _, _) = fb.get_rgba(14, 12).unwrap_or((0, 0, 0, 0));
        let (r18, _, _, _) = fb.get_rgba(14, 18).unwrap_or((0, 0, 0, 0));
        assert!(
            r12 > 0,
            "pixel at y=12 should be inside thick line (r={r12})"
        );
        assert!(
            r18 > 0,
            "pixel at y=18 should be inside thick line (r={r18})"
        );
    }

    #[test]
    fn dashed_line_has_gaps() {
        let mut fb = fresh(40, 5);
        {
            let mut c = Canvas::new(&mut fb);
            c.draw_line_dashed(
                0.0,
                2.0,
                39.0,
                2.0,
                Color(255, 255, 255, 255),
                DashPattern::new(5.0, 5.0),
            );
        }
        // Count painted and unpainted pixels.
        let mut painted = 0u32;
        let mut unpainted = 0u32;
        for x in 0..40 {
            let (r, _, _, _) = fb.get_rgba(x, 2).unwrap_or((0, 0, 0, 0));
            if r > 0 {
                painted += 1;
            } else {
                unpainted += 1;
            }
        }
        assert!(painted > 0, "should have some painted pixels");
        assert!(unpainted > 0, "should have some gap pixels");
    }

    #[test]
    fn ellipse_fill() {
        let mut fb = fresh(30, 30);
        {
            let mut c = Canvas::new(&mut fb);
            c.fill_ellipse(15.0, 15.0, 10.0, 5.0, Color(0, 255, 0, 255));
        }
        // Centre should be painted.
        let (_, g, _, _) = fb.get_rgba(15, 15).expect("centre");
        assert!(g > 0, "ellipse centre should be green");
        // Far corner should not.
        let (_, g2, _, _) = fb.get_rgba(0, 0).expect("corner");
        assert_eq!(g2, 0, "corner should not be painted");
    }

    #[test]
    fn per_corner_rounded_rect() {
        let mut fb = fresh(30, 30);
        {
            let mut c = Canvas::new(&mut fb);
            // Large corner radii — corners should be anti-aliased.
            c.fill_rounded_rect_per_corner(
                2.0,
                2.0,
                26.0,
                26.0,
                [10.0, 10.0, 10.0, 10.0],
                Color(255, 255, 255, 255),
            );
        }
        // Centre should be fully painted.
        let (r, _, _, _) = fb.get_rgba(15, 15).expect("centre");
        assert_eq!(r, 255, "centre should be fully painted");
    }

    #[test]
    fn bilinear_2x2_to_4x4() {
        // A 2×2 source with distinct quadrant colours scaled to 4×4.
        // Top-left: red, top-right: green, bottom-left: blue, bottom-right: white.
        let src_data = [
            255u8, 0, 0, 255, 0, 255, 0, 255, // row 0: red, green
            0, 0, 255, 255, 255, 255, 255, 255, // row 1: blue, white
        ];
        let mut fb = fresh(4, 4);
        {
            let mut c = Canvas::new(&mut fb);
            c.blit_bilinear(SrcImage::new(&src_data, 2, 2), 0.0, 0.0, 4, 4);
        }
        // Top-left corner should be reddish.
        let (r, g, b, _) = fb.get_rgba(0, 0).expect("(0,0)");
        assert!(
            r > g && r > b,
            "top-left should be reddish (r={r}, g={g}, b={b})"
        );
        // Top-right corner should be greenish.
        let (r2, g2, b2, _) = fb.get_rgba(3, 0).expect("(3,0)");
        assert!(
            g2 > r2 && g2 > b2,
            "top-right should be greenish (r={r2}, g={g2}, b={b2})"
        );
    }

    #[test]
    fn draw_quad_bezier_produces_pixels() {
        let mut fb = fresh(30, 30);
        {
            let mut c = Canvas::new(&mut fb);
            // Quadratic bezier: p0=(0,15), ctrl=(15,0), p2=(30,15)
            c.draw_quad_bezier(
                (0.0, 15.0),
                (15.0, 0.0),
                (29.0, 15.0),
                Color(255, 255, 255, 255),
            );
        }
        // The curve should produce some painted pixels.
        let mut painted = 0u32;
        for y in 0..30 {
            for x in 0..30 {
                let (r, _, _, _) = fb.get_rgba(x, y).unwrap_or((0, 0, 0, 0));
                if r > 0 {
                    painted += 1;
                }
            }
        }
        assert!(
            painted >= 5,
            "quad bezier should produce >= 5 painted pixels, got {painted}"
        );
    }

    #[test]
    fn draw_cubic_bezier_produces_pixels() {
        let mut fb = fresh(40, 40);
        {
            let mut c = Canvas::new(&mut fb);
            // Cubic bezier: s=(0,20), c1=(10,0), c2=(30,40), e=(39,20)
            c.draw_cubic_bezier(
                (0.0, 20.0),
                (10.0, 0.0),
                (30.0, 40.0),
                (39.0, 20.0),
                Color(0, 255, 0, 255),
            );
        }
        let mut painted = 0u32;
        for y in 0..40 {
            for x in 0..40 {
                let (_, g, _, _) = fb.get_rgba(x, y).unwrap_or((0, 0, 0, 0));
                if g > 0 {
                    painted += 1;
                }
            }
        }
        assert!(
            painted >= 5,
            "cubic bezier should produce >= 5 painted pixels, got {painted}"
        );
    }

    #[test]
    fn blit_rgba_nearest_scale() {
        let mut fb = fresh(8, 8);
        // 2x2 source: red, green / blue, white.
        let src = [
            255, 0, 0, 255, 0, 255, 0, 255, // row 0
            0, 0, 255, 255, 255, 255, 255, 255, // row 1
        ];
        {
            let mut c = Canvas::new(&mut fb);
            c.blit_rgba(SrcImage::new(&src, 2, 2), 0.0, 0.0, 4, 4);
        }
        // Top-left quadrant red, scaled 2x.
        assert_eq!(fb.get_rgba(0, 0), Some((255, 0, 0, 255)));
        assert_eq!(fb.get_rgba(1, 1), Some((255, 0, 0, 255)));
        // Bottom-right quadrant white.
        assert_eq!(fb.get_rgba(3, 3), Some((255, 255, 255, 255)));
    }
}
