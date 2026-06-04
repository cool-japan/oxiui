//! CPU-side geometry building for [`WgpuBackend`].
//!
//! This module owns:
//!
//! - `DrawSegment` — a scissor-bounded range in the solid vertex buffer.
//! - `GradientDraw` — per-draw gradient vertex + uniform data.
//! - `build_geometry` — the main CPU geometry builder that walks a
//!   [`DrawList`] and emits `(Vertex[], DrawSegment[], GradientDraw[],
//!   TexturedDraw[])`.
//! - Visibility culling helpers: `cmd_bounds`, `rect_from_points`,
//!   `rects_intersect`, `scissor_to_rect`.
//! - Stroke/dashed-line emitters and gradient uniform builders.
//!
//! [`WgpuBackend`]: super::renderer::WgpuBackend

use oxiui_core::geometry::Rect;
use oxiui_core::paint::{DrawCommand, DrawList, GradientStop, ImageFilter};
use oxiui_core::Color;

use crate::clip::{ClipRect, ClipStack};
use crate::gpu::buffer::{
    push_circle_quad, push_ellipse_quad, push_gradient_quad, push_line_quad, push_nine_slice_quads,
    push_rect_quad, push_rounded_rect_per_corner_quad, push_rounded_rect_quad, push_textured_quad,
    GradientUniforms, GradientVertex, LineQuadParams, TexQuadParams, Vertex, MAX_GRADIENT_STOPS,
};
use crate::gpu::tessellator::{tessellate_fill, tessellate_stroke};
use crate::gpu::texture::TexturedDraw;

// ── DrawSegment ───────────────────────────────────────────────────────────────

/// A scissor-bounded range of vertices in the solid vertex buffer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DrawSegment {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) scissor: Option<[u32; 4]>,
}

// ── GradientDraw ──────────────────────────────────────────────────────────────

/// Per-draw gradient vertex + uniform data.
pub(crate) struct GradientDraw {
    pub(crate) verts: Vec<GradientVertex>,
    pub(crate) uniforms: GradientUniforms,
    pub(crate) scissor: Option<[u32; 4]>,
}

// ── BackdropBlurDraw ──────────────────────────────────────────────────────────

/// Data for a single `BackdropBlur` draw command.
///
/// These are collected by [`build_geometry`] and will be executed by the
/// renderer as a copy-from-colour + Gaussian blur + write-back pass.
#[allow(dead_code)] // fields consumed in the renderer's backdrop-blur pass
pub(crate) struct BackdropBlurDraw {
    /// The region to blur, in pixel coordinates `[x, y, w, h]`.
    pub(crate) rect: [f32; 4],
    /// Blur radius in pixels.
    pub(crate) blur_radius: f32,
}

// ── Scissor helpers ───────────────────────────────────────────────────────────

/// Compute scissor from clip stack, clamped to the viewport.
pub(crate) fn scissor_from_stack(
    stack: &ClipStack,
    viewport_w: u32,
    viewport_h: u32,
) -> Option<[u32; 4]> {
    let raw = stack.as_scissor()?;
    Some(clamp_scissor(raw, viewport_w, viewport_h))
}

/// Clamp a scissor rect to the viewport bounds.
pub(crate) fn clamp_scissor([x, y, w, h]: [u32; 4], viewport_w: u32, viewport_h: u32) -> [u32; 4] {
    let x = x.min(viewport_w);
    let y = y.min(viewport_h);
    let w = w.min(viewport_w - x);
    let h = h.min(viewport_h - y);
    [x, y, w, h]
}

// ── Main geometry builder ─────────────────────────────────────────────────────

/// Output of [`build_geometry`]: a tuple of the four draw-data collections.
pub(crate) type GeometryOutput = (
    Vec<Vertex>,
    Vec<DrawSegment>,
    Vec<GradientDraw>,
    Vec<TexturedDraw>,
    Vec<BackdropBlurDraw>,
);

/// Walk `list` and emit all CPU geometry for the solid, gradient and textured passes.
///
/// Returns a [`GeometryOutput`] tuple:
/// `(solid_vertices, draw_segments, gradient_draws, textured_draws, backdrop_blur_draws)`.
pub(crate) fn build_geometry(list: &DrawList, viewport_w: u32, viewport_h: u32) -> GeometryOutput {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut segments: Vec<DrawSegment> = Vec::new();
    let mut gradient_draws: Vec<GradientDraw> = Vec::new();
    let mut textured_draws: Vec<TexturedDraw> = Vec::new();
    let mut backdrop_blur_draws: Vec<BackdropBlurDraw> = Vec::new();
    let mut stack = ClipStack::new();

    let mut current_scissor = scissor_from_stack(&stack, viewport_w, viewport_h);
    let mut segment_start: u32 = 0;

    let flush = |segs: &mut Vec<DrawSegment>, start: u32, end: u32, sc: Option<[u32; 4]>| {
        if end > start {
            segs.push(DrawSegment {
                start,
                end,
                scissor: sc,
            });
        }
    };

    for cmd in list.iter() {
        // ── Clip-stack management (always processed, never culled) ─────────
        match cmd {
            DrawCommand::PushClip { rect } => {
                flush(
                    &mut segments,
                    segment_start,
                    verts.len() as u32,
                    current_scissor,
                );
                stack.push(ClipRect::new(
                    rect.left(),
                    rect.top(),
                    rect.width(),
                    rect.height(),
                ));
                current_scissor = scissor_from_stack(&stack, viewport_w, viewport_h);
                segment_start = verts.len() as u32;
                continue;
            }
            DrawCommand::PopClip => {
                flush(
                    &mut segments,
                    segment_start,
                    verts.len() as u32,
                    current_scissor,
                );
                stack.pop();
                current_scissor = scissor_from_stack(&stack, viewport_w, viewport_h);
                segment_start = verts.len() as u32;
                continue;
            }
            _ => {}
        }

        // ── Visibility culling ────────────────────────────────────────────
        // Skip commands whose bounding rect lies entirely outside the
        // current scissor rect.  When there is no active scissor (full
        // viewport), nothing is culled.  When the bounding rect is
        // unknown (None), we render conservatively (no cull).
        if let Some(scissor) = current_scissor {
            // Degenerate scissors (zero area) cull everything.
            if scissor[2] == 0 || scissor[3] == 0 {
                continue;
            }
            if let Some(bounds) = cmd_bounds(cmd) {
                let scissor_rect = scissor_to_rect(scissor);
                if !rects_intersect(&bounds, &scissor_rect) {
                    continue;
                }
            }
        }

        // ── Geometry emission ─────────────────────────────────────────────
        match cmd {
            // Clip ops already handled above via `continue`.
            DrawCommand::PushClip { .. } | DrawCommand::PopClip => {}

            DrawCommand::FillRect { rect, color } => {
                push_rect_quad(
                    &mut verts,
                    rect.left(),
                    rect.top(),
                    rect.width(),
                    rect.height(),
                    *color,
                );
            }
            DrawCommand::StrokeRect {
                rect,
                thickness,
                color,
            } => {
                emit_stroke_rect(
                    &mut verts,
                    rect.left(),
                    rect.top(),
                    rect.width(),
                    rect.height(),
                    *thickness,
                    *color,
                );
            }
            DrawCommand::FillRoundedRect {
                rect,
                radius,
                color,
            } => {
                push_rounded_rect_quad(
                    &mut verts,
                    rect.left(),
                    rect.top(),
                    rect.width(),
                    rect.height(),
                    *radius,
                    *color,
                );
            }
            DrawCommand::FillRoundedRectPerCorner { rect, radii, color } => {
                push_rounded_rect_per_corner_quad(
                    &mut verts,
                    rect.left(),
                    rect.top(),
                    rect.width(),
                    rect.height(),
                    *radii,
                    *color,
                );
            }
            DrawCommand::FillCircle {
                center,
                radius,
                color,
            } => {
                push_circle_quad(&mut verts, center.x, center.y, *radius, *color);
            }
            DrawCommand::FillEllipse {
                center,
                rx,
                ry,
                color,
            } => {
                push_ellipse_quad(&mut verts, center.x, center.y, *rx, *ry, *color);
            }
            DrawCommand::Line { from, to, color } => {
                push_line_quad(
                    &mut verts,
                    LineQuadParams {
                        from_x: from.x,
                        from_y: from.y,
                        to_x: to.x,
                        to_y: to.y,
                        half_width: 0.5,
                        color: *color,
                        aa_smooth: false,
                    },
                );
            }
            DrawCommand::LineAa { from, to, color } => {
                push_line_quad(
                    &mut verts,
                    LineQuadParams {
                        from_x: from.x,
                        from_y: from.y,
                        to_x: to.x,
                        to_y: to.y,
                        half_width: 0.5,
                        color: *color,
                        aa_smooth: true,
                    },
                );
            }
            DrawCommand::LineThick {
                from,
                to,
                width,
                color,
            } => {
                push_line_quad(
                    &mut verts,
                    LineQuadParams {
                        from_x: from.x,
                        from_y: from.y,
                        to_x: to.x,
                        to_y: to.y,
                        half_width: width * 0.5,
                        color: *color,
                        aa_smooth: true,
                    },
                );
            }
            DrawCommand::LineDashed {
                from,
                to,
                dash_len,
                gap_len,
                color,
            } => {
                emit_dashed_line(
                    &mut verts,
                    DashedLineParams {
                        x0: from.x,
                        y0: from.y,
                        x1: to.x,
                        y1: to.y,
                        dash_len: *dash_len,
                        gap_len: *gap_len,
                        color: *color,
                    },
                );
            }
            DrawCommand::FillPath { path, color } => {
                tessellate_fill(&mut verts, path, *color);
            }
            DrawCommand::StrokePath { path, style, color } => {
                tessellate_stroke(&mut verts, path, style, *color);
            }
            DrawCommand::LinearGradient {
                rect,
                start,
                end,
                stops,
            } => {
                if let Some(gd) = build_gradient_draw_linear(LinearGradientParams {
                    x: rect.left(),
                    y: rect.top(),
                    w: rect.width(),
                    h: rect.height(),
                    sx: start.x,
                    sy: start.y,
                    ex: end.x,
                    ey: end.y,
                    stops,
                    scissor: current_scissor,
                }) {
                    gradient_draws.push(gd);
                }
            }
            DrawCommand::RadialGradient {
                rect,
                center,
                radius,
                stops,
            } => {
                if let Some(gd) = build_gradient_draw_radial(RadialGradientParams {
                    x: rect.left(),
                    y: rect.top(),
                    w: rect.width(),
                    h: rect.height(),
                    cx: center.x,
                    cy: center.y,
                    radius: *radius,
                    stops,
                    scissor: current_scissor,
                }) {
                    gradient_draws.push(gd);
                }
            }
            DrawCommand::Image {
                image,
                dest,
                filter,
            } => {
                let mut tex_verts = Vec::new();
                push_textured_quad(
                    &mut tex_verts,
                    TexQuadParams {
                        x: dest.left(),
                        y: dest.top(),
                        w: dest.width(),
                        h: dest.height(),
                        u0: 0.0,
                        v0: 0.0,
                        u1: 1.0,
                        v1: 1.0,
                        tint: [1.0, 1.0, 1.0, 1.0],
                    },
                );
                textured_draws.push(TexturedDraw {
                    verts: tex_verts,
                    image: image.clone(),
                    filter: *filter,
                    scissor: current_scissor,
                });
            }
            DrawCommand::NineSlice {
                image,
                dest,
                insets,
            } => {
                let mut tex_verts = Vec::new();
                push_nine_slice_quads(
                    &mut tex_verts,
                    [dest.left(), dest.top(), dest.width(), dest.height()],
                    image.width,
                    image.height,
                    *insets,
                    [1.0, 1.0, 1.0, 1.0],
                );
                textured_draws.push(TexturedDraw {
                    verts: tex_verts,
                    image: image.clone(),
                    filter: ImageFilter::Nearest,
                    scissor: current_scissor,
                });
            }
            DrawCommand::BackdropBlur { rect, blur_radius } => {
                // Record a backdrop blur request.  The renderer will execute
                // this as a copy-from-colour-texture + blur + write-back pass
                // between the solid and the next pass.
                backdrop_blur_draws.push(BackdropBlurDraw {
                    rect: [rect.left(), rect.top(), rect.width(), rect.height()],
                    blur_radius: *blur_radius,
                });
            }
            // SetBlendMode, BoxShadow: BoxShadow is handled by shadow::collect_shadows;
            // SetBlendMode is informational for now (blend mode plumbing is
            // handled by BlendPipelineSet in the renderer).
            // DrawText: deferred (requires glyph atlas / oxiui-text).
            _ => {}
        }
    }

    flush(
        &mut segments,
        segment_start,
        verts.len() as u32,
        current_scissor,
    );
    (
        verts,
        segments,
        gradient_draws,
        textured_draws,
        backdrop_blur_draws,
    )
}

// ── Visibility culling helpers ────────────────────────────────────────────────

/// Compute a conservative bounding [`Rect`] for a draw command, or `None` for
/// commands that have no geometric extent (clip stack ops) or for which a
/// conservative bound cannot be computed cheaply (wildcard future variants).
///
/// The returned rect is used for scissor-intersection culling only; it need
/// not be tight, but it must be a *superset* of the actual drawn pixels.
pub(crate) fn cmd_bounds(cmd: &DrawCommand) -> Option<Rect> {
    match cmd {
        DrawCommand::FillRect { rect, .. }
        | DrawCommand::StrokeRect { rect, .. }
        | DrawCommand::FillRoundedRect { rect, .. }
        | DrawCommand::FillRoundedRectPerCorner { rect, .. }
        | DrawCommand::LinearGradient { rect, .. }
        | DrawCommand::RadialGradient { rect, .. }
        | DrawCommand::Image { dest: rect, .. }
        | DrawCommand::NineSlice { dest: rect, .. }
        | DrawCommand::DrawText { rect, .. } => Some(*rect),

        DrawCommand::FillCircle { center, radius, .. } => Some(Rect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        )),

        DrawCommand::FillEllipse { center, rx, ry, .. } => {
            Some(Rect::new(center.x - rx, center.y - ry, rx * 2.0, ry * 2.0))
        }

        DrawCommand::Line { from, to, .. } | DrawCommand::LineAa { from, to, .. } => {
            Some(rect_from_points(*from, *to))
        }

        DrawCommand::LineThick {
            from, to, width, ..
        } => {
            let r = rect_from_points(*from, *to);
            Some(Rect::new(
                r.left() - width,
                r.top() - width,
                r.width() + width * 2.0,
                r.height() + width * 2.0,
            ))
        }

        DrawCommand::LineDashed { from, to, .. } => Some(rect_from_points(*from, *to)),

        DrawCommand::FillPath { path, .. } => path.bounds(),

        DrawCommand::StrokePath { path, style, .. } => path.bounds().map(|b| {
            let pad = style.width / 2.0;
            Rect::new(
                b.left() - pad,
                b.top() - pad,
                b.width() + style.width,
                b.height() + style.width,
            )
        }),

        // Clip ops have no draw geometry.
        DrawCommand::PushClip { .. } | DrawCommand::PopClip => None,

        // BoxShadow is handled by collect_shadows/render_shadows before
        // build_geometry runs; it does not emit solid vertices here, so we
        // return None so it falls through to the `_ => {}` wildcard below.
        DrawCommand::BoxShadow { .. } => None,

        // Forward-compatibility: unknown variants are rendered conservatively.
        _ => None,
    }
}

/// Build a bounding rect that covers two points (at least 1×1 in each
/// dimension so degenerate lines don't collapse to zero area).
pub(crate) fn rect_from_points(
    a: oxiui_core::geometry::Point,
    b: oxiui_core::geometry::Point,
) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let w = (a.x - b.x).abs().max(1.0);
    let h = (a.y - b.y).abs().max(1.0);
    Rect::new(x, y, w, h)
}

/// Test whether two axis-aligned rectangles overlap (at least one shared
/// pixel column and row).  A zero-area overlap (tangent border) is NOT
/// considered an intersection — it returns `false`.
pub(crate) fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.left() < b.left() + b.width()
        && a.left() + a.width() > b.left()
        && a.top() < b.top() + b.height()
        && a.top() + a.height() > b.top()
}

/// Convert a hardware scissor `[x, y, w, h]` back to a floating-point
/// [`Rect`] for intersection tests.
pub(crate) fn scissor_to_rect(s: [u32; 4]) -> Rect {
    Rect::new(s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32)
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

pub(crate) fn emit_stroke_rect(
    out: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    t: f32,
    color: Color,
) {
    push_rect_quad(out, x, y, w, t, color);
    push_rect_quad(out, x, y + h - t, w, t, color);
    push_rect_quad(out, x, y + t, t, h - 2.0 * t, color);
    push_rect_quad(out, x + w - t, y + t, t, h - 2.0 * t, color);
}

pub(crate) struct DashedLineParams {
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) dash_len: f32,
    pub(crate) gap_len: f32,
    pub(crate) color: Color,
}

pub(crate) fn emit_dashed_line(out: &mut Vec<Vertex>, p: DashedLineParams) {
    let DashedLineParams {
        x0,
        y0,
        x1,
        y1,
        dash_len,
        gap_len,
        color,
    } = p;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let total = (dx * dx + dy * dy).sqrt();
    if total < 1e-6 || dash_len <= 0.0 {
        return;
    }
    let ux = dx / total;
    let uy = dy / total;
    let period = dash_len + gap_len.max(0.0);
    if period < 1e-6 {
        return;
    }
    let mut t = 0.0_f32;
    while t < total {
        let end = (t + dash_len).min(total);
        push_line_quad(
            out,
            LineQuadParams {
                from_x: x0 + ux * t,
                from_y: y0 + uy * t,
                to_x: x0 + ux * end,
                to_y: y0 + uy * end,
                half_width: 0.5,
                color,
                aa_smooth: false,
            },
        );
        t += period;
    }
}

pub(crate) struct LinearGradientParams<'a> {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) sx: f32,
    pub(crate) sy: f32,
    pub(crate) ex: f32,
    pub(crate) ey: f32,
    pub(crate) stops: &'a [GradientStop],
    pub(crate) scissor: Option<[u32; 4]>,
}

pub(crate) fn build_gradient_draw_linear(p: LinearGradientParams<'_>) -> Option<GradientDraw> {
    let LinearGradientParams {
        x,
        y,
        w,
        h,
        sx,
        sy,
        ex,
        ey,
        stops,
        scissor,
    } = p;
    let uniforms = build_gradient_uniforms(0, [sx, sy], [ex, ey], 0.0, stops)?;
    let mut verts = Vec::new();
    push_gradient_quad(&mut verts, x, y, w, h);
    Some(GradientDraw {
        verts,
        uniforms,
        scissor,
    })
}

pub(crate) struct RadialGradientParams<'a> {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) radius: f32,
    pub(crate) stops: &'a [GradientStop],
    pub(crate) scissor: Option<[u32; 4]>,
}

pub(crate) fn build_gradient_draw_radial(p: RadialGradientParams<'_>) -> Option<GradientDraw> {
    let RadialGradientParams {
        x,
        y,
        w,
        h,
        cx,
        cy,
        radius,
        stops,
        scissor,
    } = p;
    let uniforms = build_gradient_uniforms(1, [cx, cy], [0.0, 0.0], radius, stops)?;
    let mut verts = Vec::new();
    push_gradient_quad(&mut verts, x, y, w, h);
    Some(GradientDraw {
        verts,
        uniforms,
        scissor,
    })
}

pub(crate) fn build_gradient_uniforms(
    gradient_type: u32,
    p0: [f32; 2],
    p1: [f32; 2],
    radius: f32,
    stops: &[GradientStop],
) -> Option<GradientUniforms> {
    if stops.is_empty() {
        return None;
    }
    let count = stops.len().min(MAX_GRADIENT_STOPS);
    let mut stop_offsets = [[0.0f32; 4]; MAX_GRADIENT_STOPS];
    let mut stop_colors = [[0.0f32; 4]; MAX_GRADIENT_STOPS];
    for (i, s) in stops.iter().take(count).enumerate() {
        stop_offsets[i] = [s.offset, 0.0, 0.0, 0.0];
        stop_colors[i] = [
            s.color.0 as f32 / 255.0,
            s.color.1 as f32 / 255.0,
            s.color.2 as f32 / 255.0,
            s.color.3 as f32 / 255.0,
        ];
    }
    Some(GradientUniforms {
        p0,
        p1,
        radius,
        gradient_type,
        stop_count: count as u32,
        _pad: 0,
        stop_offsets,
        stop_colors,
    })
}
