//! [`WgpuBackend`]: headless GPU [`RenderBackend`] implementing Tier 1
//! primitives and gradient fills.
//!
//! # Supported `DrawCommand` variants
//!
//! | Command                       | Pipeline | Notes                              |
//! |-------------------------------|----------|------------------------------------|
//! | `PushClip` / `PopClip`        | solid    | Hardware scissor via `ClipStack`    |
//! | `FillRect`                    | solid    | kind=0 solid quad                  |
//! | `FillCircle`                  | solid    | kind=1 SDF disc                    |
//! | `StrokeRect`                  | solid    | 4 thin edge quads                  |
//! | `FillRoundedRect`             | solid    | kind=2 SDF rounded rect            |
//! | `FillRoundedRectPerCorner`    | solid    | kind=3 SDF per-corner rounded rect |
//! | `FillEllipse`                 | solid    | kind=4 SDF ellipse                 |
//! | `Line`                        | solid    | kind=5 hard-clip line              |
//! | `LineAa`                      | solid    | kind=5 AA line                     |
//! | `LineThick`                   | solid    | kind=5 AA line with custom width   |
//! | `LineDashed`                  | solid    | CPU-split into solid segments      |
//! | `FillPath`                    | solid    | CPU fan-tessellation               |
//! | `StrokePath`                  | solid    | CPU stroke-expansion               |
//! | `LinearGradient`              | gradient | per-draw uniform + gradient quad   |
//! | `RadialGradient`              | gradient | per-draw uniform + gradient quad   |
//!
//! # Out-of-scope (deferred)
//!
//! `Image`, `NineSlice`, `BoxShadow`, `DrawText` — require texture atlas /
//! blur pipeline and are left in the wildcard arm with an honest comment.
//!
//! [`RenderBackend`]: oxiui_core::paint::RenderBackend

use oxiui_core::geometry::Size;
use oxiui_core::paint::{DrawCommand, DrawList, GradientStop, RenderBackend};
use oxiui_core::{Color, UiError};
use wgpu::util::DeviceExt;

use crate::clip::{ClipRect, ClipStack};
use crate::gpu::buffer::{
    push_circle_quad, push_ellipse_quad, push_gradient_quad, push_line_quad, push_rect_quad,
    push_rounded_rect_per_corner_quad, push_rounded_rect_quad, Globals, GradientUniforms,
    GradientVertex, LineQuadParams, Vertex, MAX_GRADIENT_STOPS,
};
use crate::gpu::device::GpuContext;
use crate::gpu::pipeline::{GradientPipeline, SolidPipeline};
use crate::gpu::tessellator::{tessellate_fill, tessellate_stroke};

// ── DrawSegment ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
struct DrawSegment {
    start: u32,
    end: u32,
    scissor: Option<[u32; 4]>,
}

// ── GradientDraw ──────────────────────────────────────────────────────────────

struct GradientDraw {
    verts: Vec<GradientVertex>,
    uniforms: GradientUniforms,
    scissor: Option<[u32; 4]>,
}

// ── WgpuBackend ───────────────────────────────────────────────────────────────

/// Headless GPU backend implementing [`RenderBackend`].
pub struct WgpuBackend {
    ctx: GpuContext,
    pipeline: SolidPipeline,
    gradient_pipeline: GradientPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    clear_color: Color,
}

impl WgpuBackend {
    /// Initialise a headless backend with an offscreen target of
    /// `width × height` physical pixels.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] when no GPU adapter is available (so
    /// the caller can skip on a machine without a usable GPU), or
    /// [`UiError::Backend`] when device creation fails.
    pub fn headless(width: u32, height: u32) -> Result<Self, UiError> {
        let ctx = GpuContext::headless(width, height)?;
        let pipeline = SolidPipeline::new(&ctx.device);
        let gradient_pipeline = GradientPipeline::new(&ctx.device);

        let globals = Globals::new(width, height);
        let globals_buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("oxiui-render-wgpu globals"),
                contents: bytemuck::bytes_of(&globals),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let globals_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui-render-wgpu globals bind group"),
            layout: &pipeline.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            ctx,
            pipeline,
            gradient_pipeline,
            globals_buffer,
            globals_bind_group,
            clear_color: Color(0, 0, 0, 0),
        })
    }

    /// Set the colour the offscreen target is cleared to before each frame.
    pub fn set_clear_color(&mut self, color: Color) {
        self.clear_color = color;
    }

    /// Return the current clear colour.
    pub fn clear_color(&self) -> Color {
        self.clear_color
    }

    /// Target width in physical pixels.
    pub fn width(&self) -> u32 {
        self.ctx.width
    }

    /// Target height in physical pixels.
    pub fn height(&self) -> u32 {
        self.ctx.height
    }

    fn scissor_from_stack(&self, stack: &ClipStack) -> Option<[u32; 4]> {
        let raw = stack.as_scissor()?;
        Some(self.clamp_scissor(raw))
    }

    fn clamp_scissor(&self, [x, y, w, h]: [u32; 4]) -> [u32; 4] {
        let x = x.min(self.ctx.width);
        let y = y.min(self.ctx.height);
        let w = w.min(self.ctx.width - x);
        let h = h.min(self.ctx.height - y);
        [x, y, w, h]
    }

    fn build_geometry(
        &self,
        list: &DrawList,
    ) -> (Vec<Vertex>, Vec<DrawSegment>, Vec<GradientDraw>) {
        let mut verts: Vec<Vertex> = Vec::new();
        let mut segments: Vec<DrawSegment> = Vec::new();
        let mut gradient_draws: Vec<GradientDraw> = Vec::new();
        let mut stack = ClipStack::new();

        let mut current_scissor = self.scissor_from_stack(&stack);
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
                    current_scissor = self.scissor_from_stack(&stack);
                    segment_start = verts.len() as u32;
                }
                DrawCommand::PopClip => {
                    flush(
                        &mut segments,
                        segment_start,
                        verts.len() as u32,
                        current_scissor,
                    );
                    stack.pop();
                    current_scissor = self.scissor_from_stack(&stack);
                    segment_start = verts.len() as u32;
                }
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
                // Image, NineSlice, BoxShadow, DrawText: deferred (require
                // texture-atlas / blur pipeline — out of scope for this slice).
                _ => {}
            }
        }

        flush(
            &mut segments,
            segment_start,
            verts.len() as u32,
            current_scissor,
        );
        (verts, segments, gradient_draws)
    }

    /// Read the offscreen colour target back into a tightly packed
    /// `width * height * 4` RGBA byte vector (row padding stripped).
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] if the GPU poll or buffer mapping fails.
    pub fn readback_rgba(&self) -> Result<Vec<u8>, UiError> {
        let width = self.ctx.width;
        let height = self.ctx.height;
        let unpadded_bytes_per_row = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

        let readback = self.ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxiui-render-wgpu readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxiui-render-wgpu readback encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.ctx.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.ctx.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.ctx
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| UiError::Render(format!("GPU poll failed during readback: {e:?}")))?;

        let data = slice.get_mapped_range();

        let mut out = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
        for row in 0..height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + unpadded_bytes_per_row as usize;
            out.extend_from_slice(&data[start..end]);
        }

        drop(data);
        readback.unmap();
        Ok(out)
    }

    /// Read back a single pixel as `(r, g, b, a)`, or `None` if out of bounds.
    pub fn read_pixel(&self, x: u32, y: u32) -> Result<Option<(u8, u8, u8, u8)>, UiError> {
        if x >= self.ctx.width || y >= self.ctx.height {
            return Ok(None);
        }
        let buf = self.readback_rgba()?;
        let idx = ((y * self.ctx.width + x) * 4) as usize;
        Ok(Some((buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3])))
    }
}

// ── RenderBackend impl ────────────────────────────────────────────────────────

impl RenderBackend for WgpuBackend {
    fn surface_size(&self) -> Size {
        Size::new(self.ctx.width as f32, self.ctx.height as f32)
    }

    fn supports_gradients(&self) -> bool {
        true
    }

    fn supports_paths(&self) -> bool {
        true
    }

    fn execute(&mut self, list: &DrawList) -> Result<(), UiError> {
        let globals = Globals::new(self.ctx.width, self.ctx.height);
        self.ctx
            .queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        let (verts, segments, gradient_draws) = self.build_geometry(list);

        let clear = self.clear_color;
        let clear_value = wgpu::Color {
            r: clear.0 as f64 / 255.0,
            g: clear.1 as f64 / 255.0,
            b: clear.2 as f64 / 255.0,
            a: clear.3 as f64 / 255.0,
        };

        let vertex_buffer = if verts.is_empty() {
            None
        } else {
            Some(
                self.ctx
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("oxiui-render-wgpu solid verts"),
                        contents: bytemuck::cast_slice(&verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };

        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxiui-render-wgpu frame encoder"),
            });

        // ── Solid pass ────────────────────────────────────────────────────────
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oxiui-render-wgpu solid pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.ctx.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_value),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if let Some(ref vb) = vertex_buffer {
                pass.set_pipeline(&self.pipeline.pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));

                for seg in &segments {
                    match seg.scissor {
                        Some([_, _, 0, _]) | Some([_, _, _, 0]) => continue,
                        Some([x, y, w, h]) => pass.set_scissor_rect(x, y, w, h),
                        None => pass.set_scissor_rect(0, 0, self.ctx.width, self.ctx.height),
                    }
                    pass.draw(seg.start..seg.end, 0..1);
                }
            }
        }

        // ── Gradient pass (one render pass per gradient draw) ─────────────────
        for gd in &gradient_draws {
            if gd.verts.is_empty() {
                continue;
            }

            let grad_vb = self
                .ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("oxiui-render-wgpu gradient verts"),
                    contents: bytemuck::cast_slice(&gd.verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });

            let grad_ub = self
                .ctx
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("oxiui-render-wgpu gradient uniforms"),
                    contents: bytemuck::bytes_of(&gd.uniforms),
                    usage: wgpu::BufferUsages::UNIFORM,
                });

            let grad_bg = self
                .ctx
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("oxiui-render-wgpu gradient bg"),
                    layout: &self.gradient_pipeline.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.globals_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: grad_ub.as_entire_binding(),
                        },
                    ],
                });

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oxiui-render-wgpu gradient pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.ctx.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.gradient_pipeline.pipeline);
            pass.set_bind_group(0, &grad_bg, &[]);
            pass.set_vertex_buffer(0, grad_vb.slice(..));

            match gd.scissor {
                Some([_, _, 0, _]) | Some([_, _, _, 0]) => continue,
                Some([x, y, w, h]) => pass.set_scissor_rect(x, y, w, h),
                None => pass.set_scissor_rect(0, 0, self.ctx.width, self.ctx.height),
            }

            pass.draw(0..gd.verts.len() as u32, 0..1);
        }

        self.ctx.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

fn emit_stroke_rect(out: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32, t: f32, color: Color) {
    push_rect_quad(out, x, y, w, t, color);
    push_rect_quad(out, x, y + h - t, w, t, color);
    push_rect_quad(out, x, y + t, t, h - 2.0 * t, color);
    push_rect_quad(out, x + w - t, y + t, t, h - 2.0 * t, color);
}

struct DashedLineParams {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    dash_len: f32,
    gap_len: f32,
    color: Color,
}

fn emit_dashed_line(out: &mut Vec<Vertex>, p: DashedLineParams) {
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

struct LinearGradientParams<'a> {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    sx: f32,
    sy: f32,
    ex: f32,
    ey: f32,
    stops: &'a [GradientStop],
    scissor: Option<[u32; 4]>,
}

fn build_gradient_draw_linear(p: LinearGradientParams<'_>) -> Option<GradientDraw> {
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

struct RadialGradientParams<'a> {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    cx: f32,
    cy: f32,
    radius: f32,
    stops: &'a [GradientStop],
    scissor: Option<[u32; 4]>,
}

fn build_gradient_draw_radial(p: RadialGradientParams<'_>) -> Option<GradientDraw> {
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

fn build_gradient_uniforms(
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::geometry::{Point, Rect};
    use oxiui_core::paint::{DrawList, GradientStop, PathData, StrokeStyle};
    use oxiui_core::Color;

    fn try_backend(w: u32, h: u32) -> Option<WgpuBackend> {
        WgpuBackend::headless(w, h).ok()
    }

    fn assert_visible(b: &WgpuBackend, x: u32, y: u32, label: &str) {
        let px = b
            .read_pixel(x, y)
            .expect("read_pixel ok")
            .expect("in bounds");
        assert!(px.3 > 0, "{label}: pixel ({x},{y}) alpha=0, got {px:?}");
    }

    fn assert_transparent(b: &WgpuBackend, x: u32, y: u32, label: &str) {
        let px = b
            .read_pixel(x, y)
            .expect("read_pixel ok")
            .expect("in bounds");
        assert!(
            px.3 == 0,
            "{label}: pixel ({x},{y}) expected transparent, got {px:?}"
        );
    }

    #[test]
    fn test_stroke_rect_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push(DrawCommand::StrokeRect {
            rect: Rect::new(10.0, 10.0, 80.0, 80.0),
            thickness: 4.0,
            color: Color(255, 0, 0, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 12, 10, "stroke_rect top border");
        assert_transparent(&b, 50, 50, "stroke_rect interior");
    }

    #[test]
    fn test_fill_rounded_rect_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push(DrawCommand::FillRoundedRect {
            rect: Rect::new(10.0, 10.0, 80.0, 80.0),
            radius: 10.0,
            color: Color(0, 200, 0, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 50, 50, "rrect centre");
        assert_transparent(&b, 10, 10, "rrect corner tl");
    }

    #[test]
    fn test_fill_rounded_rect_per_corner_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push(DrawCommand::FillRoundedRectPerCorner {
            rect: Rect::new(10.0, 10.0, 80.0, 80.0),
            radii: [15.0, 5.0, 15.0, 5.0],
            color: Color(0, 100, 200, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 50, 50, "rrect-pc centre");
    }

    #[test]
    fn test_fill_ellipse_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push(DrawCommand::FillEllipse {
            center: Point::new(50.0, 50.0),
            rx: 30.0,
            ry: 20.0,
            color: Color(200, 0, 200, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 50, 50, "ellipse centre");
        assert_transparent(&b, 2, 2, "ellipse exterior");
    }

    #[test]
    fn test_line_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push(DrawCommand::Line {
            from: Point::new(10.0, 50.0),
            to: Point::new(90.0, 50.0),
            color: Color(255, 255, 0, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 50, 50, "line mid");
    }

    #[test]
    fn test_fill_path_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut path = PathData::new();
        path.move_to(Point::new(20.0, 20.0));
        path.line_to(Point::new(80.0, 20.0));
        path.line_to(Point::new(50.0, 80.0));
        path.close();
        let mut dl = DrawList::new();
        dl.push(DrawCommand::FillPath {
            path,
            color: Color(255, 0, 128, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 50, 40, "fill_path interior");
        assert_transparent(&b, 2, 2, "fill_path exterior");
    }

    #[test]
    fn test_stroke_path_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let mut path = PathData::new();
        path.move_to(Point::new(20.0, 50.0));
        path.line_to(Point::new(80.0, 50.0));
        let style = StrokeStyle {
            width: 4.0,
            ..Default::default()
        };
        let mut dl = DrawList::new();
        dl.push(DrawCommand::StrokePath {
            path,
            style,
            color: Color(200, 200, 0, 255),
        });
        b.execute(&dl).expect("execute ok");
        assert_visible(&b, 50, 50, "stroke_path mid");
    }

    #[test]
    fn test_linear_gradient_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let stops = vec![
            GradientStop::new(0.0, Color(255, 0, 0, 255)),
            GradientStop::new(1.0, Color(0, 0, 255, 255)),
        ];
        let mut dl = DrawList::new();
        dl.push(DrawCommand::LinearGradient {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            start: Point::new(0.0, 50.0),
            end: Point::new(100.0, 50.0),
            stops,
        });
        b.execute(&dl).expect("execute ok");
        let left = b.read_pixel(5, 50).expect("ok").expect("bounds");
        assert!(left.0 > 128, "left reddish: {left:?}");
        let right = b.read_pixel(95, 50).expect("ok").expect("bounds");
        assert!(right.2 > 128, "right bluish: {right:?}");
        let mid = b.read_pixel(50, 50).expect("ok").expect("bounds");
        assert!(mid.3 > 0, "mid visible: {mid:?}");
    }

    #[test]
    fn test_radial_gradient_renders() {
        let Some(mut b) = try_backend(100, 100) else {
            return;
        };
        let stops = vec![
            GradientStop::new(0.0, Color(255, 255, 255, 255)),
            GradientStop::new(1.0, Color(0, 0, 0, 255)),
        ];
        let mut dl = DrawList::new();
        dl.push(DrawCommand::RadialGradient {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            center: Point::new(50.0, 50.0),
            radius: 40.0,
            stops,
        });
        b.execute(&dl).expect("execute ok");
        let centre = b.read_pixel(50, 50).expect("ok").expect("bounds");
        assert!(centre.0 > 200, "centre bright: {centre:?}");
        let edge = b.read_pixel(90, 50).expect("ok").expect("bounds");
        assert!(
            edge.0 < centre.0,
            "edge darker: edge={edge:?} centre={centre:?}"
        );
    }

    #[test]
    fn test_supports_probes() {
        let Some(b) = try_backend(64, 64) else {
            return;
        };
        assert!(b.supports_gradients());
        assert!(b.supports_paths());
    }
}
