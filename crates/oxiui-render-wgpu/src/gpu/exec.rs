//! GPU pass execution helpers and per-frame statistics for [`WgpuBackend`].
//!
//! This module owns:
//!
//! - [`FrameStats`] — per-frame draw-call and render-pass counters.
//! - `run_solid_pass` — executes the solid vertex-buffer pass using a
//!   persistent, reused, growable vertex buffer.
//! - `run_gradient_pass_batched` — executes ALL gradient draws in a single
//!   render pass using dynamic-offset uniforms.
//! - `run_textured_pass` — executes one textured render pass.
//!
//! [`WgpuBackend`]: super::renderer::WgpuBackend

use oxiui_core::UiError;
use wgpu::util::DeviceExt;

use crate::gpu::buffer::{GradientUniforms, GradientVertex, Vertex};
use crate::gpu::geometry::{DrawSegment, GradientDraw};
use crate::gpu::pipeline::{GradientPipeline, SolidPipeline, TexturedPipeline};
use crate::gpu::texture::{upload_image, TexturedDraw};

// ── FrameStats ────────────────────────────────────────────────────────────────

/// Per-frame draw-call and render-pass counters.
///
/// Populated during `WgpuBackend::execute` and accessible via
/// [`WgpuBackend::frame_stats`].
///
/// [`WgpuBackend::frame_stats`]: super::renderer::WgpuBackend::frame_stats
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Number of `pass.draw(...)` calls issued during the last `execute()` call.
    ///
    /// Counts only real GPU draws; skipped/dead segments are excluded.
    /// Shadow passes are included.
    pub draw_calls: u32,
    /// Number of `begin_render_pass` calls issued during the last `execute()` call.
    ///
    /// Includes the clear pass, shadow passes, solid pass, gradient pass(es),
    /// and textured pass(es).
    pub render_passes: u32,
}

// ── align_up ──────────────────────────────────────────────────────────────────

/// Round `n` up to the next multiple of `align` (which must be a power of two).
#[inline]
pub(crate) fn align_up(n: u32, align: u32) -> u32 {
    (n + align - 1) & !(align - 1)
}

// ── Solid pass ────────────────────────────────────────────────────────────────

/// Parameters for the solid-geometry render pass.
pub(crate) struct SolidPassParams<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) encoder: &'a mut wgpu::CommandEncoder,
    pub(crate) screen_view: &'a wgpu::TextureView,
    pub(crate) screen_resolve: Option<&'a wgpu::TextureView>,
    pub(crate) pipeline: &'a SolidPipeline,
    pub(crate) globals_bind_group: &'a wgpu::BindGroup,
    pub(crate) verts: &'a [Vertex],
    pub(crate) segments: &'a [DrawSegment],
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    /// Persistent vertex buffer: mutated in-place if it needs to grow.
    pub(crate) solid_vertex_buf: &'a mut Option<wgpu::Buffer>,
    /// Byte capacity of the persistent vertex buffer.
    pub(crate) solid_vertex_buf_capacity: &'a mut usize,
}

/// Execute the solid geometry pass, returning the number of draw calls issued.
///
/// Uses a persistent, reusable vertex buffer that is grown (next power-of-two)
/// whenever the current frame requires more capacity, then updated via
/// `queue.write_buffer` instead of creating a new buffer.
pub(crate) fn run_solid_pass(p: SolidPassParams<'_>) -> u32 {
    let mut draw_calls = 0u32;

    // ── Persistent buffer management ─────────────────────────────────────────
    // If we have vertices to draw, ensure the persistent buffer is large enough
    // and upload the current frame's geometry.
    if !p.verts.is_empty() {
        let verts_bytes: &[u8] = bytemuck::cast_slice(p.verts);
        let needed = verts_bytes.len();

        let needs_grow = p.solid_vertex_buf.is_none() || *p.solid_vertex_buf_capacity < needed;

        if needs_grow {
            // Grow to the next power of two, with a minimum of 64 vertices.
            let min_bytes = core::mem::size_of::<Vertex>() * 64;
            let new_cap = needed.next_power_of_two().max(min_bytes);
            *p.solid_vertex_buf = Some(p.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxiui-render-wgpu solid-verts-persistent"),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            *p.solid_vertex_buf_capacity = new_cap;
        }

        // Upload current frame's geometry into the persistent buffer.
        if let Some(buf) = p.solid_vertex_buf.as_ref() {
            p.queue.write_buffer(buf, 0, verts_bytes);
        }
    }

    let mut pass = p.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("oxiui-render-wgpu solid pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: p.screen_view,
            depth_slice: None,
            resolve_target: p.screen_resolve,
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

    if let Some(ref vb) = p.solid_vertex_buf {
        if !p.verts.is_empty() {
            // ── Pipeline state caching ────────────────────────────────────
            // Track raw pointers to the last-set pipeline and globals bind
            // group so that redundant `set_pipeline` / `set_bind_group`
            // calls can be skipped.  These are frame-local variables;
            // each RenderPass starts from a clean GPU state so there is
            // nothing to carry across pass boundaries.
            let mut last_pipeline_ptr: Option<*const wgpu::RenderPipeline> = None;
            let mut last_globals_bg_ptr: Option<*const wgpu::BindGroup> = None;

            let cur_pipeline_ptr = &p.pipeline.pipeline as *const wgpu::RenderPipeline;
            let cur_globals_bg_ptr = p.globals_bind_group as *const wgpu::BindGroup;

            if last_pipeline_ptr != Some(cur_pipeline_ptr) {
                pass.set_pipeline(&p.pipeline.pipeline);
                last_pipeline_ptr = Some(cur_pipeline_ptr);
            }
            if last_globals_bg_ptr != Some(cur_globals_bg_ptr) {
                pass.set_bind_group(0, p.globals_bind_group, &[]);
                last_globals_bg_ptr = Some(cur_globals_bg_ptr);
            }
            // Anchor the caching locals so the compiler does not warn
            // about dead assignments when there is only one pipeline/BG.
            let _ = last_pipeline_ptr;
            let _ = last_globals_bg_ptr;

            // Draw only the vertices for this frame (0..verts.len()), not
            // the whole buffer capacity.
            pass.set_vertex_buffer(
                0,
                vb.slice(..p.verts.len() as u64 * core::mem::size_of::<Vertex>() as u64),
            );

            for seg in p.segments {
                match seg.scissor {
                    Some([_, _, 0, _]) | Some([_, _, _, 0]) => continue,
                    Some([x, y, w, h]) => pass.set_scissor_rect(x, y, w, h),
                    None => pass.set_scissor_rect(0, 0, p.viewport_w, p.viewport_h),
                }
                pass.draw(seg.start..seg.end, 0..1);
                draw_calls += 1;
            }
        }
    }

    draw_calls
}

// ── Gradient pass (batched) ───────────────────────────────────────────────────

/// Parameters for the batched gradient render pass.
///
/// All gradient draws are coalesced into a single render pass using a
/// dynamic-offset uniform buffer.  The pipeline bind group layout must have
/// binding 1 with `has_dynamic_offset: true`.
pub(crate) struct GradientPassParams<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) encoder: &'a mut wgpu::CommandEncoder,
    pub(crate) screen_view: &'a wgpu::TextureView,
    pub(crate) screen_resolve: Option<&'a wgpu::TextureView>,
    pub(crate) pipeline: &'a GradientPipeline,
    pub(crate) globals_buffer: &'a wgpu::Buffer,
    pub(crate) gradient_draws: &'a [GradientDraw],
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
}

/// Execute ALL gradient draws in a single render pass via dynamic-offset uniforms.
///
/// Returns `(render_passes_added, draw_calls_added)`.
/// Returns `(0, 0)` when `gradient_draws` is empty.
///
/// # Dynamic-offset batching
///
/// All per-gradient [`GradientUniforms`] are packed into a single combined
/// buffer with a stride of `align_up(sizeof(GradientUniforms),
/// min_uniform_buffer_offset_alignment)`.  Inside the one render pass each
/// draw call issues `set_bind_group` with a different byte offset — this is
/// valid because binding 1 in the gradient bind group layout has
/// `has_dynamic_offset: true`.
pub(crate) fn run_gradient_pass_batched(p: GradientPassParams<'_>) -> (u32, u32) {
    if p.gradient_draws.is_empty() {
        return (0, 0);
    }

    // ── Compute per-element stride (device-aligned) ───────────────────────
    let struct_size = core::mem::size_of::<GradientUniforms>() as u32;
    let min_align = p.device.limits().min_uniform_buffer_offset_alignment;
    let grad_stride = align_up(struct_size, min_align) as u64;

    let n_grads = p.gradient_draws.len() as u64;

    // ── Build combined uniforms buffer ────────────────────────────────────
    let grad_uniform_buf = p.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("oxiui-render-wgpu grad-uniforms-combined"),
        size: n_grads * grad_stride,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for (i, gd) in p.gradient_draws.iter().enumerate() {
        p.queue.write_buffer(
            &grad_uniform_buf,
            i as u64 * grad_stride,
            bytemuck::bytes_of(&gd.uniforms),
        );
    }

    // ── Build combined vertex buffer (all quads concatenated) ─────────────
    let all_verts: Vec<GradientVertex> = p
        .gradient_draws
        .iter()
        .flat_map(|gd| gd.verts.iter().copied())
        .collect();

    if all_verts.is_empty() {
        return (0, 0);
    }

    let grad_vert_buf = p
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu grad-verts-combined"),
            contents: bytemuck::cast_slice(&all_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

    // ── Create ONE bind group using the combined uniforms buffer ──────────
    // Binding 1 (dynamic) needs a sized binding so the driver knows which
    // chunk of the buffer each draw uses.  We provide a BufferBinding with
    // offset=0 and size=sizeof(GradientUniforms); the actual per-draw offset
    // is supplied via the dynamic offset argument to set_bind_group.
    let grad_uniform_size =
        core::num::NonZeroU64::new(core::mem::size_of::<GradientUniforms>() as u64);
    let grad_bg = p.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oxiui-render-wgpu gradient-batched bg"),
        layout: &p.pipeline.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: p.globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &grad_uniform_buf,
                    offset: 0,
                    size: grad_uniform_size,
                }),
            },
        ],
    });

    // ── ONE render pass for all gradient draws ────────────────────────────
    let mut pass = p.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("oxiui-render-wgpu gradient-batched pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: p.screen_view,
            depth_slice: None,
            resolve_target: p.screen_resolve,
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

    pass.set_pipeline(&p.pipeline.pipeline);
    pass.set_vertex_buffer(0, grad_vert_buf.slice(..));

    let mut draw_calls = 0u32;
    let mut vertex_offset: u32 = 0;

    for (i, gd) in p.gradient_draws.iter().enumerate() {
        if gd.verts.is_empty() {
            continue;
        }

        // Dynamic offset selects this gradient's uniform slice.
        let dyn_offset = (i as u64 * grad_stride) as u32;
        pass.set_bind_group(0, &grad_bg, &[dyn_offset]);

        // Apply scissor for this gradient draw.
        match gd.scissor {
            Some([_, _, 0, _]) | Some([_, _, _, 0]) => {
                vertex_offset += gd.verts.len() as u32;
                continue;
            }
            Some([x, y, w, h]) => pass.set_scissor_rect(x, y, w, h),
            None => pass.set_scissor_rect(0, 0, p.viewport_w, p.viewport_h),
        }

        let n_verts = gd.verts.len() as u32;
        pass.draw(vertex_offset..vertex_offset + n_verts, 0..1);
        draw_calls += 1;
        vertex_offset += n_verts;
    }

    (1, draw_calls)
}

// ── Textured pass ─────────────────────────────────────────────────────────────

/// Parameters for a single textured render pass.
pub(crate) struct TexturedPassParams<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) encoder: &'a mut wgpu::CommandEncoder,
    pub(crate) screen_view: &'a wgpu::TextureView,
    pub(crate) screen_resolve: Option<&'a wgpu::TextureView>,
    pub(crate) pipeline: &'a TexturedPipeline,
    pub(crate) globals_bind_group: &'a wgpu::BindGroup,
    pub(crate) td: &'a TexturedDraw,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
}

/// Execute one textured pass. Returns `(render_passes_added, draw_calls_added)`.
///
/// Returns `(0, 0)` if the textured draw has no vertices.
///
/// # Errors
///
/// Returns [`UiError::Render`] if texture upload fails.
pub(crate) fn run_textured_pass(p: TexturedPassParams<'_>) -> Result<(u32, u32), UiError> {
    if p.td.verts.is_empty() {
        return Ok((0, 0));
    }

    let (tex_view, tex_sampler) = upload_image(p.device, p.queue, &p.td.image, p.td.filter)
        .map_err(|e| UiError::Render(format!("texture upload failed: {e}")))?;

    let tex_vb = p
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu tex verts"),
            contents: bytemuck::cast_slice(&p.td.verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

    let tex_bg = p.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oxiui-render-wgpu tex bind group"),
        layout: &p.pipeline.texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&tex_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&tex_sampler),
            },
        ],
    });

    let mut pass = p.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("oxiui-render-wgpu textured pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: p.screen_view,
            depth_slice: None,
            resolve_target: p.screen_resolve,
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

    pass.set_pipeline(&p.pipeline.pipeline);
    pass.set_bind_group(0, p.globals_bind_group, &[]);
    pass.set_bind_group(1, &tex_bg, &[]);
    pass.set_vertex_buffer(0, tex_vb.slice(..));

    match p.td.scissor {
        Some([_, _, 0, _]) | Some([_, _, _, 0]) => return Ok((1, 0)),
        Some([x, y, w, h]) => pass.set_scissor_rect(x, y, w, h),
        None => pass.set_scissor_rect(0, 0, p.viewport_w, p.viewport_h),
    }

    pass.draw(0..p.td.verts.len() as u32, 0..1);
    Ok((1, 1))
}
