//! Shadow rendering: mask → 2-pass Gaussian blur → composite onto main target.
//!
//! For each [`ShadowDesc`]:
//! 1. Clear ping texture to transparent.
//! 2. Render white rect mask of the (offset-shifted) rect into ping using the
//!    solid pipeline.
//! 3. Horizontal blur ping → pong.
//! 4. Vertical blur pong → ping.
//! 5. Composite ping (tinted by shadow colour) onto the main `target_view`
//!    using `LoadOp::Load` (alpha blending — shadow renders *under* content
//!    because it is composited before the solid/gradient/textured passes).
//!
//! When `blur_radius <= 0.5` the blur passes are skipped and the white mask is
//! composited directly, yielding a crisp (hard-edge) shadow.
//!
//! Ping-pong textures are allocated once per `render_shadows` call and shared
//! across all shadows in the draw list.

use crate::gpu::buffer::{push_fullscreen_quad, push_rect_quad, BlurUniforms, CompUniforms};
use crate::gpu::buffer::{GradientVertex, Vertex};
use crate::gpu::device::TARGET_FORMAT;
use crate::gpu::pipeline::{BlurPipeline, CompositePipeline, SolidPipeline};
use oxiui_core::paint::{DrawCommand, DrawList};
use oxiui_core::{Color, UiError};
use wgpu::util::DeviceExt;

/// Maximum Gaussian blur radius in pixels.  Clamped to bound the tap-loop
/// iteration count (`2 * MAX_BLUR_RADIUS + 1 = 129` iterations worst-case).
pub const MAX_BLUR_RADIUS: u32 = 64;

// ── ShadowRenderStats ─────────────────────────────────────────────────────────

/// Render pass and draw call counts accumulated across all shadow descriptions
/// submitted by a single [`render_shadows`] call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShadowRenderStats {
    /// Total render passes opened across all shadow rendering.
    pub render_passes: u32,
    /// Total `pass.draw(...)` calls issued across all shadow rendering.
    pub draw_calls: u32,
}

// ── ShadowDesc ────────────────────────────────────────────────────────────────

/// A parsed, ready-to-render shadow.
pub struct ShadowDesc {
    /// The shadow rectangle in pixel space (original rect shifted by offset).
    pub shadow_rect: oxiui_core::geometry::Rect,
    /// Shadow colour (used as the composite tint).
    pub color: Color,
    /// Blur radius in pixels (0 = sharp, >0 = soft halo).
    pub blur_radius: f32,
}

// ── ShadowPipelines ───────────────────────────────────────────────────────────

/// The three GPU pipelines needed for shadow rendering.
pub struct ShadowPipelines<'a> {
    /// Solid pipeline (used to draw the white rect mask).
    pub solid: &'a SolidPipeline,
    /// Blur pipeline (separable Gaussian, horizontal and vertical).
    pub blur: &'a BlurPipeline,
    /// Composite pipeline (tint + alpha-blend mask onto target).
    pub composite: &'a CompositePipeline,
}

// ── ShadowGpuState ────────────────────────────────────────────────────────────

/// GPU device/queue state needed for shadow rendering.
pub struct ShadowGpuState<'a> {
    /// The logical GPU device.
    pub device: &'a wgpu::Device,
    /// The command queue.
    pub queue: &'a wgpu::Queue,
    /// The main frame colour view (shadow composites here).
    /// Under MSAA this is the MSAA multisample view (the render target).
    pub target_view: &'a wgpu::TextureView,
    /// The MSAA resolve target for the composite pass, or `None` when no MSAA.
    /// Under MSAA this is the single-sample `color_view`; under no MSAA it is
    /// `None` and `target_view` is the direct render target.
    pub resolve_target: Option<&'a wgpu::TextureView>,
    /// The per-frame viewport globals buffer.
    pub globals_buffer: &'a wgpu::Buffer,
    /// The bind group for the viewport globals (group 0, solid pipeline).
    pub globals_bind_group: &'a wgpu::BindGroup,
    /// Viewport width in pixels.
    pub viewport_w: u32,
    /// Viewport height in pixels.
    pub viewport_h: u32,
}

// ── ShadowContext ─────────────────────────────────────────────────────────────

/// Internal shared state threaded through the per-shadow render passes.
struct ShadowContext<'a> {
    gpu: &'a ShadowGpuState<'a>,
    pipelines: &'a ShadowPipelines<'a>,
    linear_sampler: &'a wgpu::Sampler,
    fs_vb: &'a wgpu::Buffer,
    fs_count: u32,
    texel_w: f32,
    texel_h: f32,
}

// ── PingPong ──────────────────────────────────────────────────────────────────

/// Ping-pong texture pair and their views.
struct PingPong<'a> {
    ping_tex: &'a wgpu::Texture,
    ping_view: &'a wgpu::TextureView,
    pong_tex: &'a wgpu::Texture,
    pong_view: &'a wgpu::TextureView,
}

// ── collect_shadows ───────────────────────────────────────────────────────────

/// Collect all [`DrawCommand::BoxShadow`] entries from `list` into
/// [`ShadowDesc`] values, translating each rect by its offset.
pub fn collect_shadows(list: &DrawList) -> Vec<ShadowDesc> {
    list.iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::BoxShadow {
                rect,
                offset,
                blur_radius,
                color,
            } => {
                let sr = oxiui_core::geometry::Rect::new(
                    rect.left() + offset.x,
                    rect.top() + offset.y,
                    rect.width(),
                    rect.height(),
                );
                Some(ShadowDesc {
                    shadow_rect: sr,
                    color: *color,
                    blur_radius: *blur_radius,
                })
            }
            _ => None,
        })
        .collect()
}

// ── render_shadows ────────────────────────────────────────────────────────────

/// Render all `descs` shadows, compositing each onto `gpu.target_view`.
///
/// Ping-pong textures are allocated once for the entire batch.  Each shadow
/// issues separate command-encoder submissions so wgpu's internal ordering
/// guarantees sequential execution.
///
/// The caller must ensure the main frame encoder has NOT yet been submitted —
/// shadows are composited onto the target *before* the solid/gradient/textured
/// passes so content renders on top.
///
/// # Errors
///
/// Propagates any [`UiError`] from sub-passes.
pub fn render_shadows(
    gpu: &ShadowGpuState<'_>,
    pipelines: &ShadowPipelines<'_>,
    descs: &[ShadowDesc],
) -> Result<ShadowRenderStats, UiError> {
    if descs.is_empty() {
        return Ok(ShadowRenderStats::default());
    }

    let vp_w = gpu.viewport_w;
    let vp_h = gpu.viewport_h;

    // ── Allocate ping-pong offscreen targets ──────────────────────────────────
    let ping_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("oxiui-render-wgpu shadow ping"),
        size: wgpu::Extent3d {
            width: vp_w,
            height: vp_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let pong_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("oxiui-render-wgpu shadow pong"),
        size: wgpu::Extent3d {
            width: vp_w,
            height: vp_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let ping_view = ping_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let pong_view = pong_tex.create_view(&wgpu::TextureViewDescriptor::default());

    // Linear sampler shared across all blur and composite passes.
    let linear_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("oxiui-render-wgpu shadow sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 32.0,
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    });

    // Fullscreen quad vertex buffer — shared across all passes.
    let mut fs_verts: Vec<GradientVertex> = Vec::new();
    push_fullscreen_quad(&mut fs_verts, vp_w as f32, vp_h as f32);
    let fs_vb = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu shadow fullscreen quad"),
            contents: bytemuck::cast_slice(&fs_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

    let ctx = ShadowContext {
        gpu,
        pipelines,
        linear_sampler: &linear_sampler,
        fs_vb: &fs_vb,
        fs_count: fs_verts.len() as u32,
        texel_w: 1.0 / vp_w as f32,
        texel_h: 1.0 / vp_h as f32,
    };

    let pp = PingPong {
        ping_tex: &ping_tex,
        ping_view: &ping_view,
        pong_tex: &pong_tex,
        pong_view: &pong_view,
    };

    let mut stats = ShadowRenderStats::default();
    for shadow in descs {
        let s = render_shadow_desc(&ctx, &pp, shadow)?;
        stats.render_passes += s.render_passes;
        stats.draw_calls += s.draw_calls;
    }

    Ok(stats)
}

// ── render_shadow_desc ────────────────────────────────────────────────────────

/// Render a single [`ShadowDesc`]: mask → optional blur → composite.
///
/// Returns the number of render passes opened and draw calls issued.
fn render_shadow_desc(
    ctx: &ShadowContext<'_>,
    pp: &PingPong<'_>,
    shadow: &ShadowDesc,
) -> Result<ShadowRenderStats, UiError> {
    let sigma = shadow.blur_radius.max(1.0) / 2.0;
    let radius = ((3.0 * sigma).ceil() as u32).min(MAX_BLUR_RADIUS) as f32;

    let mut stats = ShadowRenderStats::default();

    // Step 2: white rect mask into ping (clears ping first).
    // 1 render pass, 1 draw.
    render_mask(ctx, pp.ping_view, shadow)?;
    stats.render_passes += 1;
    stats.draw_calls += 1;

    if shadow.blur_radius > 0.5 {
        // Step 3: horizontal blur ping → pong. 1 render pass, 1 draw.
        execute_blur_pass(
            ctx,
            pp.ping_tex,
            pp.pong_view,
            &BlurPassParams {
                direction: [1.0, 0.0],
                texel_size: [ctx.texel_w, ctx.texel_h],
                radius,
                sigma,
            },
        )?;
        stats.render_passes += 1;
        stats.draw_calls += 1;
        // Step 4: vertical blur pong → ping. 1 render pass, 1 draw.
        execute_blur_pass(
            ctx,
            pp.pong_tex,
            pp.ping_view,
            &BlurPassParams {
                direction: [0.0, 1.0],
                texel_size: [ctx.texel_w, ctx.texel_h],
                radius,
                sigma,
            },
        )?;
        stats.render_passes += 1;
        stats.draw_calls += 1;
    }

    // Step 5: composite ping onto main target. 1 render pass, 1 draw.
    execute_composite_pass(ctx, pp.ping_tex, shadow.color)?;
    stats.render_passes += 1;
    stats.draw_calls += 1;

    Ok(stats)
}

// ── render_mask ───────────────────────────────────────────────────────────────

/// Clear ping to transparent and draw the white rect mask for `shadow`.
fn render_mask(
    ctx: &ShadowContext<'_>,
    ping_view: &wgpu::TextureView,
    shadow: &ShadowDesc,
) -> Result<(), UiError> {
    let sr = &shadow.shadow_rect;
    let mut rect_verts: Vec<Vertex> = Vec::new();
    push_rect_quad(
        &mut rect_verts,
        sr.left(),
        sr.top(),
        sr.width(),
        sr.height(),
        Color(255, 255, 255, 255),
    );

    let rect_vb = ctx
        .gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu shadow mask rect"),
            contents: bytemuck::cast_slice(&rect_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

    let mut encoder = ctx
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxiui-render-wgpu shadow mask encoder"),
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxiui-render-wgpu shadow mask pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ping_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&ctx.pipelines.solid.pipeline);
        pass.set_bind_group(0, ctx.gpu.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, rect_vb.slice(..));
        pass.set_scissor_rect(0, 0, ctx.gpu.viewport_w, ctx.gpu.viewport_h);
        pass.draw(0..rect_verts.len() as u32, 0..1);
    }

    ctx.gpu.queue.submit(Some(encoder.finish()));
    Ok(())
}

// ── BlurPassParams ────────────────────────────────────────────────────────────

/// Parameters for a single separable Gaussian blur pass.
struct BlurPassParams {
    direction: [f32; 2],
    texel_size: [f32; 2],
    radius: f32,
    sigma: f32,
}

// ── execute_blur_pass ─────────────────────────────────────────────────────────

fn execute_blur_pass(
    ctx: &ShadowContext<'_>,
    src_tex: &wgpu::Texture,
    dst_view: &wgpu::TextureView,
    params: &BlurPassParams,
) -> Result<(), UiError> {
    let src_view = src_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let blur_uni = BlurUniforms {
        direction: params.direction,
        texel_size: params.texel_size,
        radius: params.radius,
        sigma: params.sigma,
        _pad: [0.0; 2],
    };

    let blur_ub = ctx
        .gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu blur uniform"),
            contents: bytemuck::bytes_of(&blur_uni),
            usage: wgpu::BufferUsages::UNIFORM,
        });

    let globals_bg = ctx
        .gpu
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui-render-wgpu blur globals bg"),
            layout: &ctx.pipelines.blur.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ctx.gpu.globals_buffer.as_entire_binding(),
            }],
        });

    let src_bg = ctx
        .gpu
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui-render-wgpu blur source bg"),
            layout: &ctx.pipelines.blur.source_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: blur_ub.as_entire_binding(),
                },
            ],
        });

    let mut encoder = ctx
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxiui-render-wgpu blur encoder"),
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxiui-render-wgpu blur pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 0.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&ctx.pipelines.blur.pipeline);
        pass.set_bind_group(0, &globals_bg, &[]);
        pass.set_bind_group(1, &src_bg, &[]);
        pass.set_vertex_buffer(0, ctx.fs_vb.slice(..));
        pass.set_scissor_rect(0, 0, ctx.gpu.viewport_w, ctx.gpu.viewport_h);
        pass.draw(0..ctx.fs_count, 0..1);
    }

    ctx.gpu.queue.submit(Some(encoder.finish()));
    Ok(())
}

// ── execute_composite_pass ────────────────────────────────────────────────────

/// Composite `mask_tex` tinted by `shadow_color` onto the main target.
fn execute_composite_pass(
    ctx: &ShadowContext<'_>,
    mask_tex: &wgpu::Texture,
    shadow_color: Color,
) -> Result<(), UiError> {
    let mask_view = mask_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let tint = [
        shadow_color.0 as f32 / 255.0,
        shadow_color.1 as f32 / 255.0,
        shadow_color.2 as f32 / 255.0,
        shadow_color.3 as f32 / 255.0,
    ];

    let comp_uni = CompUniforms {
        tint,
        texel_size: [ctx.texel_w, ctx.texel_h],
        _pad: [0.0; 2],
    };

    let comp_ub = ctx
        .gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu composite uniform"),
            contents: bytemuck::bytes_of(&comp_uni),
            usage: wgpu::BufferUsages::UNIFORM,
        });

    let globals_bg = ctx
        .gpu
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui-render-wgpu composite globals bg"),
            layout: &ctx.pipelines.composite.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ctx.gpu.globals_buffer.as_entire_binding(),
            }],
        });

    let src_bg = ctx
        .gpu
        .device
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui-render-wgpu composite source bg"),
            layout: &ctx.pipelines.composite.source_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&mask_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: comp_ub.as_entire_binding(),
                },
            ],
        });

    let mut encoder = ctx
        .gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxiui-render-wgpu composite encoder"),
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxiui-render-wgpu composite pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.gpu.target_view,
                depth_slice: None,
                // When MSAA is active, resolve_target holds the single-sample
                // colour view so the resolve happens during this pass.
                resolve_target: ctx.gpu.resolve_target,
                ops: wgpu::Operations {
                    // Load existing content — shadow composites on top of whatever
                    // was already in the target (the prior clear pass).
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&ctx.pipelines.composite.pipeline);
        pass.set_bind_group(0, &globals_bg, &[]);
        pass.set_bind_group(1, &src_bg, &[]);
        pass.set_vertex_buffer(0, ctx.fs_vb.slice(..));
        pass.set_scissor_rect(0, 0, ctx.gpu.viewport_w, ctx.gpu.viewport_h);
        pass.draw(0..ctx.fs_count, 0..1);
    }

    ctx.gpu.queue.submit(Some(encoder.finish()));
    Ok(())
}
