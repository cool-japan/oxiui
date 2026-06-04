//! Vertex / uniform data layouts for the headless solid-fill + gradient pipelines.
//!
//! All structs are `#[repr(C)]` and implement [`bytemuck::Pod`] /
//! [`bytemuck::Zeroable`] so they can be uploaded to the GPU as raw bytes with
//! a guaranteed, stable memory layout.
//!
//! # Vertex layout (56 bytes = 14 × f32)
//!
//! The struct was extended from the original 48-byte form to add an `extra`
//! field (`[f32; 2]`) that carries per-kind auxiliary parameters:
//!
//! | `kind` | `local`          | `shape_xy`      | `shape_r`           | `extra`              |
//! |--------|------------------|-----------------|---------------------|----------------------|
//! | 0 rect | pixel pos        | –               | –                   | –                    |
//! | 1 circle | pixel pos      | centre (cx,cy)  | radius              | –                    |
//! | 2 rrect-uniform | pixel pos | centre (cx,cy) | radius             | half-size (hw,hh)    |
//! | 3 rrect-per-corner | pixel pos | centre (cx,cy) | pack16(tl,tr)  | pack16(br,bl), pack16(hw,hh) |
//! | 4 ellipse | pixel pos     | centre (cx,cy)  | –                   | (rx, ry)             |
//! | 5 line-seg | pixel pos    | from (ax,ay)    | half_width (+0.5=aa)| to (bx,by)           |
//!
//! For kind=3 the four corner radii and the half-extents are packed as u16
//! pairs into f32 bit patterns (see `pack_u16_pair`).

use oxiui_core::Color;

// ── Vertex ─────────────────────────────────────────────────────────────────

/// A single vertex fed to `solid.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Pixel-space quad-corner position (`@location(0)`).
    pub position: [f32; 2],
    /// Straight-alpha RGBA colour in `[0, 1]` (`@location(1)`).
    pub color: [f32; 4],
    /// Pixel-space position used for SDF evaluation (`@location(2)`).
    pub local: [f32; 2],
    /// Shape parameter XY: centre for circle/ellipse/rounded-rect;
    /// line-segment start point for lines (`@location(3)`).
    pub shape_xy: [f32; 2],
    /// Shape parameter R: radius for circle/uniform-rrect; packed u16 pair
    /// (tl, tr) for per-corner rrect; (half_width + 0.5 if AA) for lines
    /// (`@location(4)`).
    pub shape_r: f32,
    /// Primitive discriminator (`@location(5)`):
    /// `0` = rect, `1` = circle, `2` = rrect-uniform, `3` = rrect-per-corner,
    /// `4` = ellipse, `5` = line-segment.
    pub kind: f32,
    /// Auxiliary parameters (`@location(6)`):
    /// - kind 2 (rrect-uniform): `[hw, hh]` half-extents.
    /// - kind 3 (rrect-per-corner): `[pack16(br,bl), pack16(hw,hh)]`.
    /// - kind 4 (ellipse): `[rx, ry]`.
    /// - kind 5 (line-seg): `[to_x, to_y]` endpoint B.
    /// - all others: `[0, 0]`.
    pub extra: [f32; 2],
}

// ── Kind discriminator constants ─────────────────────────────────────────────

/// Primitive discriminator value for a solid rectangle.
pub const KIND_RECT: f32 = 0.0;
/// Primitive discriminator value for an SDF circle.
pub const KIND_CIRCLE: f32 = 1.0;
/// Primitive discriminator value for a uniformly-rounded rectangle (SDF).
pub const KIND_ROUNDED_RECT: f32 = 2.0;
/// Primitive discriminator value for a per-corner rounded rectangle (SDF).
pub const KIND_ROUNDED_RECT_PC: f32 = 3.0;
/// Primitive discriminator value for an SDF ellipse.
pub const KIND_ELLIPSE: f32 = 4.0;
/// Primitive discriminator value for a line-segment SDF.
pub const KIND_LINE_SEG: f32 = 5.0;

// Compile-time layout guard: (2+4+2+2+1+1+2) f32 = 14 f32 = 56 bytes.
const _: () = assert!(core::mem::size_of::<Vertex>() == 56);
const _: () = assert!(core::mem::align_of::<Vertex>() == 4);

impl Vertex {
    /// Convert an 8-bit [`Color`] into straight-alpha `[f32; 4]` in `[0, 1]`.
    #[inline]
    pub fn color_to_f32(color: Color) -> [f32; 4] {
        [
            color.0 as f32 / 255.0,
            color.1 as f32 / 255.0,
            color.2 as f32 / 255.0,
            color.3 as f32 / 255.0,
        ]
    }
}

// ── Globals (uniform) ────────────────────────────────────────────────────────

/// Per-frame uniform block matching the WGSL `Globals` struct.
///
/// Padded to 16 bytes to satisfy the uniform-buffer alignment rules.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    /// Viewport `[width, height]` in physical pixels.
    pub viewport: [f32; 2],
    /// Padding to round the struct up to a 16-byte boundary.
    pub _pad: [f32; 2],
}

const _: () = assert!(core::mem::size_of::<Globals>() == 16);

impl Globals {
    /// Construct a [`Globals`] from a viewport size in pixels.
    #[inline]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            viewport: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        }
    }
}

// ── Gradient vertex / uniform ─────────────────────────────────────────────────

/// A single vertex fed to `gradient.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientVertex {
    /// Pixel-space quad-corner position (`@location(0)`).
    pub position: [f32; 2],
    /// Pixel-space position passed to the fragment stage for gradient sampling
    /// (`@location(1)`).
    pub local: [f32; 2],
}

const _: () = assert!(core::mem::size_of::<GradientVertex>() == 16);

/// Maximum number of gradient colour stops supported in a single gradient draw.
pub const MAX_GRADIENT_STOPS: usize = 8;

/// Per-gradient uniform block sent to `gradient.wgsl`.
///
/// Follows std140 layout: each field is aligned to its natural boundary,
/// the total size is a multiple of 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientUniforms {
    /// Linear: gradient start point (pixel space).
    /// Radial: gradient centre point (pixel space).
    pub p0: [f32; 2],
    /// Linear: gradient end point (pixel space).
    /// Radial: unused (zeroed).
    pub p1: [f32; 2],
    /// Radial: outer radius in pixels.  0 for linear.
    pub radius: f32,
    /// Gradient type: 0 = linear, 1 = radial.
    pub gradient_type: u32,
    /// Number of active colour stops (1–8).
    pub stop_count: u32,
    /// Padding to align the arrays on 16-byte boundaries.
    pub _pad: u32,
    /// Per-stop offset packed into `.x` (y/z/w = 0).
    pub stop_offsets: [[f32; 4]; MAX_GRADIENT_STOPS],
    /// Per-stop RGBA colour in `[0, 1]`.
    pub stop_colors: [[f32; 4]; MAX_GRADIENT_STOPS],
}

// Size: p0(8) + p1(8) + radius(4) + gradient_type(4) + stop_count(4) + _pad(4)
//       + stop_offsets(8*16=128) + stop_colors(8*16=128) = 32 + 256 = 288 bytes.
const _: () = assert!(core::mem::size_of::<GradientUniforms>() == 288);

// ── Packing helpers ───────────────────────────────────────────────────────────

/// Pack two `u16` values into the bit pattern of a `f32`.
///
/// The WGSL shader unpacks with `bitcast<u32>(v) >> 16u` and `& 0xffffu`.
/// Values must be in `[0, 65535]`.
#[inline]
pub fn pack_u16_pair(hi: u16, lo: u16) -> f32 {
    f32::from_bits(((hi as u32) << 16) | (lo as u32))
}

// ── Quad emitters ────────────────────────────────────────────────────────────

/// Append six vertices (two triangles) covering the axis-aligned rectangle
/// `(x, y, w, h)` with a uniform `color`, tagged as a solid rectangle.
pub fn push_rect_quad(out: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32, color: Color) {
    let rgba = Vertex::color_to_f32(color);
    let x1 = x + w;
    let y1 = y + h;
    let corners = [[x, y], [x, y1], [x1, y1], [x, y], [x1, y1], [x1, y]];
    for c in corners {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [0.0, 0.0],
            shape_r: 0.0,
            kind: KIND_RECT,
            extra: [0.0, 0.0],
        });
    }
}

/// Append six vertices covering the bounding quad of the circle centred at
/// `(cx, cy)` with `radius`, tagged as an SDF circle.
pub fn push_circle_quad(out: &mut Vec<Vertex>, cx: f32, cy: f32, radius: f32, color: Color) {
    let rgba = Vertex::color_to_f32(color);
    let r = radius + 1.0;
    let x0 = cx - r;
    let y0 = cy - r;
    let x1 = cx + r;
    let y1 = cy + r;
    let corners = [[x0, y0], [x0, y1], [x1, y1], [x0, y0], [x1, y1], [x1, y0]];
    for c in corners {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [cx, cy],
            shape_r: radius,
            kind: KIND_CIRCLE,
            extra: [0.0, 0.0],
        });
    }
}

/// Append six vertices for a uniformly-rounded rectangle (SDF).
///
/// The quad is inflated by 1 px on all sides to avoid clipping the AA rim.
pub fn push_rounded_rect_quad(
    out: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: Color,
) {
    let rgba = Vertex::color_to_f32(color);
    let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let hw = w * 0.5;
    let hh = h * 0.5;
    let pad = 1.0_f32;
    let x0 = x - pad;
    let y0 = y - pad;
    let x1 = x + w + pad;
    let y1 = y + h + pad;
    let corners = [[x0, y0], [x0, y1], [x1, y1], [x0, y0], [x1, y1], [x1, y0]];
    for c in corners {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [cx, cy],
            shape_r: r,
            kind: KIND_ROUNDED_RECT,
            extra: [hw, hh],
        });
    }
}

/// Append six vertices for a per-corner rounded rectangle (SDF).
///
/// `radii` is `[top-left, top-right, bottom-right, bottom-left]`.
///
/// The four radii and the half-extents are packed into the vertex using
/// integer-arithmetic encoding that avoids GPU subnormal/denormal issues:
///
/// * `shape_r`  = `tl_r * 256.0 + tr_r`  (radii clamped to `[0, 255]`)
/// * `extra[0]` = `br_r * 256.0 + bl_r`
/// * `extra[1]` = `hw_i * 4096.0 + hh_i` (half-extents clamped to `[0, 4095]`)
///
/// The WGSL shader unpacks with `floor(v / base)` / `mod(v, base)`.
/// All packed values stay below `2^24`, so f32 represents them exactly.
pub fn push_rounded_rect_per_corner_quad(
    out: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radii: [f32; 4],
    color: Color,
) {
    let rgba = Vertex::color_to_f32(color);
    let [tl, tr, br, bl] = radii;
    let hw = w * 0.5;
    let hh = h * 0.5;
    let cx = x + hw;
    let cy = y + hh;
    let clamp_r = |r: f32| r.clamp(0.0, hw.min(hh).min(255.0));
    let tl = clamp_r(tl);
    let tr = clamp_r(tr);
    let br = clamp_r(br);
    let bl = clamp_r(bl);
    let hw_c = hw.clamp(0.0, 4095.0);
    let hh_c = hh.clamp(0.0, 4095.0);
    // Encode using integer arithmetic within exact f32 range (< 2^24).
    let r_packed = tl.floor() * 256.0 + tr.floor();
    let brbl_packed = br.floor() * 256.0 + bl.floor();
    let hwhh_packed = hw_c.floor() * 4096.0 + hh_c.floor();
    let pad = 1.0_f32;
    let x0 = x - pad;
    let y0 = y - pad;
    let x1 = x + w + pad;
    let y1 = y + h + pad;
    let corners = [[x0, y0], [x0, y1], [x1, y1], [x0, y0], [x1, y1], [x1, y0]];
    for c in corners {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [cx, cy],
            shape_r: r_packed,
            kind: KIND_ROUNDED_RECT_PC,
            extra: [brbl_packed, hwhh_packed],
        });
    }
}

/// Append six vertices for an SDF ellipse centred at `(cx, cy)` with
/// horizontal radius `rx` and vertical radius `ry`.
pub fn push_ellipse_quad(out: &mut Vec<Vertex>, cx: f32, cy: f32, rx: f32, ry: f32, color: Color) {
    let rgba = Vertex::color_to_f32(color);
    let pad = 1.0_f32;
    let x0 = cx - rx - pad;
    let y0 = cy - ry - pad;
    let x1 = cx + rx + pad;
    let y1 = cy + ry + pad;
    let corners = [[x0, y0], [x0, y1], [x1, y1], [x0, y0], [x1, y1], [x1, y0]];
    for c in corners {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [cx, cy],
            shape_r: 0.0,
            kind: KIND_ELLIPSE,
            extra: [rx, ry],
        });
    }
}

/// Parameters for a line-segment SDF quad.
pub struct LineQuadParams {
    /// Start x coordinate.
    pub from_x: f32,
    /// Start y coordinate.
    pub from_y: f32,
    /// End x coordinate.
    pub to_x: f32,
    /// End y coordinate.
    pub to_y: f32,
    /// Half-width of the line stroke.
    pub half_width: f32,
    /// Line colour.
    pub color: Color,
    /// `true` = anti-aliased edges; `false` = hard clip.
    pub aa_smooth: bool,
}

/// Append six vertices for a line-segment SDF quad.
///
/// The quad is expanded perpendicular to the line by `half_width + 1.0` pixels
/// to ensure the anti-aliased edge is not clipped.
///
/// When `aa_smooth` is `true`, the shader uses `smoothstep` for soft edges.
/// When `false`, the edge is hard-clipped.
pub fn push_line_quad(out: &mut Vec<Vertex>, params: LineQuadParams) {
    let LineQuadParams {
        from_x,
        from_y,
        to_x,
        to_y,
        half_width,
        color,
        aa_smooth,
    } = params;
    let rgba = Vertex::color_to_f32(color);
    let dx = to_x - from_x;
    let dy = to_y - from_y;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    // Perpendicular unit vector (rotated 90° CCW).
    let nx = -dy / len;
    let ny = dx / len;
    // Also expand along the line direction (caps) by half_width.
    let lx = dx / len;
    let ly = dy / len;
    let expand = half_width + 1.0;
    let cap = half_width + 1.0;
    // Quad corners: a = from-side start, b = from-side end,
    //               c = to-side end,     d = to-side start.
    let ax = from_x - lx * cap + nx * expand;
    let ay = from_y - ly * cap + ny * expand;
    let bx = from_x - lx * cap - nx * expand;
    let by = from_y - ly * cap - ny * expand;
    let cx = to_x + lx * cap - nx * expand;
    let cy_v = to_y + ly * cap - ny * expand;
    let dxp = to_x + lx * cap + nx * expand;
    let dyp = to_y + ly * cap + ny * expand;
    // Two CCW triangles: (a, b, c) and (a, c, d).
    let corners = [
        [ax, ay],
        [bx, by],
        [cx, cy_v],
        [ax, ay],
        [cx, cy_v],
        [dxp, dyp],
    ];
    // Encode aa_smooth in the fractional part of shape_r:
    // shape_r = half_width + 0.5 → aa, half_width + 0.0 → hard.
    let shape_r_val = if aa_smooth {
        half_width + 0.5
    } else {
        half_width
    };
    for c in corners {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [from_x, from_y],
            shape_r: shape_r_val,
            kind: KIND_LINE_SEG,
            extra: [to_x, to_y],
        });
    }
}

/// Append three vertices (one triangle) for a path fill triangle.
pub fn push_triangle(
    out: &mut Vec<Vertex>,
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    color: Color,
) {
    let rgba = Vertex::color_to_f32(color);
    for c in [p0, p1, p2] {
        out.push(Vertex {
            position: c,
            color: rgba,
            local: c,
            shape_xy: [0.0, 0.0],
            shape_r: 0.0,
            kind: KIND_RECT,
            extra: [0.0, 0.0],
        });
    }
}

// ── TexVertex ─────────────────────────────────────────────────────────────────

/// A single vertex fed to `textured.wgsl`.
///
/// 32 bytes = 2 (position) + 2 (uv) + 4 (tint) f32 values.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexVertex {
    /// Pixel-space quad-corner position (`@location(0)`).
    pub position: [f32; 2],
    /// Texture UV in `[0, 1]` (`@location(1)`).
    pub uv: [f32; 2],
    /// RGBA tint multiplier (`@location(2)`); normally `[1, 1, 1, 1]`.
    pub tint: [f32; 4],
}

const _: () = assert!(core::mem::size_of::<TexVertex>() == 32);

// ── Textured quad emitters ────────────────────────────────────────────────────

/// Parameters for a textured quad.
pub struct TexQuadParams {
    /// Quad x position (pixel space).
    pub x: f32,
    /// Quad y position (pixel space).
    pub y: f32,
    /// Quad width (pixel space).
    pub w: f32,
    /// Quad height (pixel space).
    pub h: f32,
    /// UV left edge.
    pub u0: f32,
    /// UV top edge.
    pub v0: f32,
    /// UV right edge.
    pub u1: f32,
    /// UV bottom edge.
    pub v1: f32,
    /// RGBA tint multiplier (component-wise; use `[1,1,1,1]` for no tint).
    pub tint: [f32; 4],
}

/// Append six textured vertices (two triangles) for the given [`TexQuadParams`].
///
/// The UV rectangle `(u0, v0) → (u1, v1)` is mapped over the destination quad.
/// `tint` is multiplied component-wise with the sampled texel in the shader.
pub fn push_textured_quad(out: &mut Vec<TexVertex>, p: TexQuadParams) {
    let TexQuadParams {
        x,
        y,
        w,
        h,
        u0,
        v0,
        u1,
        v1,
        tint,
    } = p;
    let x1 = x + w;
    let y1 = y + h;
    // CCW winding: TL, BL, BR, TL, BR, TR
    let corners = [
        ([x, y], [u0, v0]),
        ([x, y1], [u0, v1]),
        ([x1, y1], [u1, v1]),
        ([x, y], [u0, v0]),
        ([x1, y1], [u1, v1]),
        ([x1, y], [u1, v0]),
    ];
    for (pos, uv) in corners {
        out.push(TexVertex {
            position: pos,
            uv,
            tint,
        });
    }
}

/// Append the 9 quads for a nine-slice image.
///
/// `dest` is `[x, y, w, h]` in pixel space (the destination rectangle).
/// `img_w`/`img_h` are the source image dimensions in pixels.
/// `insets` is `[top, right, bottom, left]` in source pixels.
///
/// The nine regions are:
///   TL corner | T edge   | TR corner
///   L  edge   | Centre   | R  edge
///   BL corner | B edge   | BR corner
///
/// Insets are clamped so `left+right <= img_w` and `top+bottom <= img_h`.
/// Destination insets scale proportionally.  Any region with zero source or
/// destination dimension is skipped.
pub fn push_nine_slice_quads(
    out: &mut Vec<TexVertex>,
    dest: [f32; 4],
    img_w: u32,
    img_h: u32,
    insets: [u32; 4],
    tint: [f32; 4],
) {
    let [dst_x, dst_y, dst_w, dst_h] = dest;

    // ── Clamp source insets so they don't overlap ─────────────────────────────
    let iw = img_w as f32;
    let ih = img_h as f32;

    let raw_top = insets[0] as f32;
    let raw_right = insets[1] as f32;
    let raw_bottom = insets[2] as f32;
    let raw_left = insets[3] as f32;

    // Scale down if left+right > img_w or top+bottom > img_h
    let lr_sum = raw_left + raw_right;
    let tb_sum = raw_top + raw_bottom;
    let (src_left, src_right) = if lr_sum > iw && lr_sum > 0.0 {
        let scale = iw / lr_sum;
        (raw_left * scale, raw_right * scale)
    } else {
        (raw_left, raw_right)
    };
    let (src_top, src_bottom) = if tb_sum > ih && tb_sum > 0.0 {
        let scale = ih / tb_sum;
        (raw_top * scale, raw_bottom * scale)
    } else {
        (raw_top, raw_bottom)
    };

    // ── Destination insets (pixel-space sizes matching corner regions) ─────────
    // Scale destination corner sizes to match the source inset proportions,
    // but clamp so they don't exceed the destination rect dimensions.
    let dst_left = src_left.min(dst_w * 0.5);
    let dst_right = src_right.min(dst_w - dst_left);
    let dst_top = src_top.min(dst_h * 0.5);
    let dst_bottom = src_bottom.min(dst_h - dst_top);

    // ── Source UV breakpoints ─────────────────────────────────────────────────
    let u0 = 0.0_f32;
    let u1 = src_left / iw;
    let u2 = (iw - src_right) / iw;
    let u3 = 1.0_f32;

    let v0 = 0.0_f32;
    let v1 = src_top / ih;
    let v2 = (ih - src_bottom) / ih;
    let v3 = 1.0_f32;

    // ── Destination pixel breakpoints ─────────────────────────────────────────
    let dx0 = dst_x;
    let dx1 = dst_x + dst_left;
    let dx2 = dst_x + dst_w - dst_right;
    let dx3 = dst_x + dst_w;

    let dy0 = dst_y;
    let dy1 = dst_y + dst_top;
    let dy2 = dst_y + dst_h - dst_bottom;
    let dy3 = dst_y + dst_h;

    // ── Emit each of the 9 regions (row-major, skip degenerate) ──────────────
    // Row 0: TL corner, T edge, TR corner
    // Row 1: L edge,   Centre, R edge
    // Row 2: BL corner, B edge, BR corner

    let regions: [([f32; 4], [f32; 4]); 9] = [
        // (dest [x,y,w,h], src_uv [u0,v0,u1,v1])
        ([dx0, dy0, dx1 - dx0, dy1 - dy0], [u0, v0, u1, v1]), // TL
        ([dx1, dy0, dx2 - dx1, dy1 - dy0], [u1, v0, u2, v1]), // T
        ([dx2, dy0, dx3 - dx2, dy1 - dy0], [u2, v0, u3, v1]), // TR
        ([dx0, dy1, dx1 - dx0, dy2 - dy1], [u0, v1, u1, v2]), // L
        ([dx1, dy1, dx2 - dx1, dy2 - dy1], [u1, v1, u2, v2]), // Centre
        ([dx2, dy1, dx3 - dx2, dy2 - dy1], [u2, v1, u3, v2]), // R
        ([dx0, dy2, dx1 - dx0, dy3 - dy2], [u0, v2, u1, v3]), // BL
        ([dx1, dy2, dx2 - dx1, dy3 - dy2], [u1, v2, u2, v3]), // B
        ([dx2, dy2, dx3 - dx2, dy3 - dy2], [u2, v2, u3, v3]), // BR
    ];

    for ([rx, ry, rw, rh], [ru0, rv0, ru1, rv1]) in regions {
        // Skip degenerate regions (zero destination or zero source UV span)
        if rw <= 0.0 || rh <= 0.0 {
            continue;
        }
        if (ru1 - ru0).abs() <= 0.0 || (rv1 - rv0).abs() <= 0.0 {
            continue;
        }
        push_textured_quad(
            out,
            TexQuadParams {
                x: rx,
                y: ry,
                w: rw,
                h: rh,
                u0: ru0,
                v0: rv0,
                u1: ru1,
                v1: rv1,
                tint,
            },
        );
    }
}

/// Append six gradient vertices covering `(x, y, w, h)`.
pub fn push_gradient_quad(out: &mut Vec<GradientVertex>, x: f32, y: f32, w: f32, h: f32) {
    let x1 = x + w;
    let y1 = y + h;
    let corners = [[x, y], [x, y1], [x1, y1], [x, y], [x1, y1], [x1, y]];
    for c in corners {
        out.push(GradientVertex {
            position: c,
            local: c,
        });
    }
}

// ── BlurUniforms ──────────────────────────────────────────────────────────────

/// Uniform block for the separable Gaussian blur pass.
///
/// Matches the `BlurUniforms` struct in `blur.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurUniforms {
    /// Blur direction: `[1.0, 0.0]` for horizontal, `[0.0, 1.0]` for vertical.
    pub direction: [f32; 2],
    /// Per-texel step size: `[1.0/viewport_w, 1.0/viewport_h]`.
    pub texel_size: [f32; 2],
    /// Blur radius in pixels (0 = no blur, 1 tap each side).
    pub radius: f32,
    /// Gaussian sigma: `max(blur_radius, 1.0) / 2.0`.
    pub sigma: f32,
    /// Padding to 32 bytes.
    pub _pad: [f32; 2],
}

const _: () = assert!(core::mem::size_of::<BlurUniforms>() == 32);

// ── CompUniforms ──────────────────────────────────────────────────────────────

/// Uniform block for the shadow composite pass.
///
/// Matches the `CompUniforms` struct in `composite.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompUniforms {
    /// Shadow tint colour in `[0, 1]` premultiplied RGBA.
    pub tint: [f32; 4],
    /// Per-texel step size: `[1.0/viewport_w, 1.0/viewport_h]`.
    pub texel_size: [f32; 2],
    /// Padding to 32 bytes.
    pub _pad: [f32; 2],
}

const _: () = assert!(core::mem::size_of::<CompUniforms>() == 32);

// ── Fullscreen quad emitter ───────────────────────────────────────────────────

/// Push a fullscreen quad (covers `[0,0]` to `[w, h]`) as 6 [`GradientVertex`]
/// vertices.
///
/// Used by the blur and composite passes which draw over the entire viewport.
/// The `local` field carries pixel-space coordinates used as UV source in those
/// shaders.
pub fn push_fullscreen_quad(out: &mut Vec<GradientVertex>, w: f32, h: f32) {
    // Corner definitions: (position, local) — both in pixel space.
    let corners = [
        ([0.0_f32, 0.0_f32], [0.0_f32, 0.0_f32]), // TL
        ([w, 0.0], [w, 0.0]),                     // TR
        ([0.0, h], [0.0, h]),                     // BL
        ([w, h], [w, h]),                         // BR
    ];
    // Triangle 1: TL, TR, BL
    out.push(GradientVertex {
        position: corners[0].0,
        local: corners[0].1,
    });
    out.push(GradientVertex {
        position: corners[1].0,
        local: corners[1].1,
    });
    out.push(GradientVertex {
        position: corners[2].0,
        local: corners[2].1,
    });
    // Triangle 2: TR, BR, BL
    out.push(GradientVertex {
        position: corners[1].0,
        local: corners[1].1,
    });
    out.push(GradientVertex {
        position: corners[3].0,
        local: corners[3].1,
    });
    out.push(GradientVertex {
        position: corners[2].0,
        local: corners[2].1,
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_size_is_56_bytes() {
        assert_eq!(core::mem::size_of::<Vertex>(), 56);
    }

    #[test]
    fn globals_size_is_16_bytes() {
        assert_eq!(core::mem::size_of::<Globals>(), 16);
    }

    #[test]
    fn gradient_vertex_size_is_16_bytes() {
        assert_eq!(core::mem::size_of::<GradientVertex>(), 16);
    }

    #[test]
    fn gradient_uniforms_size_is_288_bytes() {
        assert_eq!(core::mem::size_of::<GradientUniforms>(), 288);
    }

    #[test]
    fn color_to_f32_maps_full_range() {
        let white = Vertex::color_to_f32(Color(255, 255, 255, 255));
        assert!((white[0] - 1.0).abs() < 1e-6);
        assert!((white[3] - 1.0).abs() < 1e-6);
        let black = Vertex::color_to_f32(Color(0, 0, 0, 0));
        assert_eq!(black, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn rect_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_rect_quad(&mut v, 1.0, 2.0, 3.0, 4.0, Color(255, 0, 0, 255));
        assert_eq!(v.len(), 6);
        for vert in &v {
            assert_eq!(vert.kind, KIND_RECT);
        }
        let xs: Vec<f32> = v.iter().map(|vt| vt.position[0]).collect();
        assert!(xs.contains(&1.0));
        assert!(xs.contains(&4.0));
    }

    #[test]
    fn circle_quad_emits_six_vertices_with_center() {
        let mut v = Vec::new();
        push_circle_quad(&mut v, 10.0, 10.0, 5.0, Color(0, 255, 0, 255));
        assert_eq!(v.len(), 6);
        for vert in &v {
            assert_eq!(vert.kind, KIND_CIRCLE);
            assert_eq!(vert.shape_xy, [10.0, 10.0]);
            assert!((vert.shape_r - 5.0).abs() < 1e-6);
        }
    }

    #[test]
    fn vertices_are_pod_castable() {
        let mut v = Vec::new();
        push_rect_quad(&mut v, 0.0, 0.0, 1.0, 1.0, Color(1, 2, 3, 4));
        let bytes: &[u8] = bytemuck::cast_slice(&v);
        assert_eq!(bytes.len(), 6 * 56);
    }

    #[test]
    fn rounded_rect_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_rounded_rect_quad(&mut v, 10.0, 10.0, 80.0, 40.0, 8.0, Color(0, 0, 255, 255));
        assert_eq!(v.len(), 6);
        for vert in &v {
            assert_eq!(vert.kind, KIND_ROUNDED_RECT);
        }
    }

    #[test]
    fn rounded_rect_pc_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_rounded_rect_per_corner_quad(
            &mut v,
            10.0,
            10.0,
            80.0,
            40.0,
            [4.0, 8.0, 4.0, 8.0],
            Color(0, 0, 255, 255),
        );
        assert_eq!(v.len(), 6);
        for vert in &v {
            assert_eq!(vert.kind, KIND_ROUNDED_RECT_PC);
        }
    }

    #[test]
    fn ellipse_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_ellipse_quad(&mut v, 50.0, 50.0, 30.0, 20.0, Color(255, 255, 0, 255));
        assert_eq!(v.len(), 6);
        for vert in &v {
            assert_eq!(vert.kind, KIND_ELLIPSE);
            assert!((vert.extra[0] - 30.0).abs() < 1e-4);
            assert!((vert.extra[1] - 20.0).abs() < 1e-4);
        }
    }

    #[test]
    fn line_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_line_quad(
            &mut v,
            LineQuadParams {
                from_x: 0.0,
                from_y: 0.0,
                to_x: 100.0,
                to_y: 0.0,
                half_width: 2.0,
                color: Color(255, 0, 0, 255),
                aa_smooth: true,
            },
        );
        assert_eq!(v.len(), 6);
        for vert in &v {
            assert_eq!(vert.kind, KIND_LINE_SEG);
        }
    }

    #[test]
    fn push_triangle_emits_three_vertices() {
        let mut v = Vec::new();
        push_triangle(
            &mut v,
            [0.0, 0.0],
            [10.0, 0.0],
            [5.0, 8.0],
            Color(255, 255, 255, 255),
        );
        assert_eq!(v.len(), 3);
        for vert in &v {
            assert_eq!(vert.kind, KIND_RECT);
        }
    }

    #[test]
    fn gradient_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_gradient_quad(&mut v, 0.0, 0.0, 100.0, 50.0);
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn pack_u16_pair_round_trips() {
        let packed = pack_u16_pair(42, 1000);
        let bits = packed.to_bits();
        let hi = (bits >> 16) as u16;
        let lo = (bits & 0xffff) as u16;
        assert_eq!(hi, 42);
        assert_eq!(lo, 1000);
    }

    #[test]
    fn tex_vertex_size_is_32_bytes() {
        assert_eq!(core::mem::size_of::<TexVertex>(), 32);
    }

    #[test]
    fn textured_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_textured_quad(
            &mut v,
            TexQuadParams {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
                tint: [1.0, 1.0, 1.0, 1.0],
            },
        );
        assert_eq!(v.len(), 6);
        // All vertices have the same tint
        for vert in &v {
            assert_eq!(vert.tint, [1.0, 1.0, 1.0, 1.0]);
        }
        // UV corners should span [0,0] to [1,1]
        let us: Vec<f32> = v.iter().map(|vt| vt.uv[0]).collect();
        assert!(us.contains(&0.0_f32));
        assert!(us.contains(&1.0_f32));
    }

    #[test]
    fn nine_slice_quads_emit_at_most_54_vertices() {
        let mut v = Vec::new();
        // 12x12 image, 4px insets on each side
        push_nine_slice_quads(
            &mut v,
            [0.0, 0.0, 64.0, 64.0],
            12,
            12,
            [4, 4, 4, 4],
            [1.0, 1.0, 1.0, 1.0],
        );
        // 9 regions × 6 vertices = 54
        assert_eq!(v.len(), 54);
    }

    #[test]
    fn blur_uniforms_size_is_32_bytes() {
        assert_eq!(core::mem::size_of::<BlurUniforms>(), 32);
    }

    #[test]
    fn comp_uniforms_size_is_32_bytes() {
        assert_eq!(core::mem::size_of::<CompUniforms>(), 32);
    }

    #[test]
    fn fullscreen_quad_emits_six_vertices() {
        let mut v = Vec::new();
        push_fullscreen_quad(&mut v, 100.0, 200.0);
        assert_eq!(v.len(), 6);
        // Check corners cover 0..100 and 0..200
        let xs: Vec<f32> = v.iter().map(|vt| vt.position[0]).collect();
        let ys: Vec<f32> = v.iter().map(|vt| vt.position[1]).collect();
        assert!(xs.contains(&0.0_f32));
        assert!(xs.contains(&100.0_f32));
        assert!(ys.contains(&0.0_f32));
        assert!(ys.contains(&200.0_f32));
    }

    #[test]
    fn nine_slice_degenerate_insets_skip_regions() {
        let mut v = Vec::new();
        // Zero insets → only centre region (6 verts), corner/edge regions are
        // degenerate because src UV span is 0.
        push_nine_slice_quads(
            &mut v,
            [0.0, 0.0, 64.0, 64.0],
            12,
            12,
            [0, 0, 0, 0],
            [1.0, 1.0, 1.0, 1.0],
        );
        // Only the centre should be non-degenerate: 6 vertices
        assert_eq!(v.len(), 6);
    }
}
