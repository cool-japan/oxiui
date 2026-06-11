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
use oxiui_core::paint::{DrawList, RenderBackend};
use oxiui_core::{Color, UiError};
use wgpu::util::DeviceExt;

use crate::gpu::buffer::Globals;
use crate::gpu::device::GpuContext;
use crate::gpu::exec::{
    run_gradient_pass_batched, run_solid_pass, run_textured_pass, FrameStats, GradientPassParams,
    SolidPassParams, TexturedPassParams,
};
use crate::gpu::geometry::build_geometry;
use crate::gpu::pipeline::{
    BlurPipeline, CompositePipeline, GradientPipeline, SolidPipeline, TexturedPipeline,
};

#[cfg(feature = "text")]
use crate::text_bridge::TextBridge;
#[cfg(feature = "text")]
use oxiui_text::TextPipeline;

// ── WgpuBackend ───────────────────────────────────────────────────────────────

/// Headless GPU backend implementing [`RenderBackend`].
pub struct WgpuBackend {
    ctx: GpuContext,
    /// Screen solid pipeline — uses `ctx.sample_count` (may be MSAA).
    pipeline: SolidPipeline,
    gradient_pipeline: GradientPipeline,
    textured_pipeline: TexturedPipeline,
    /// Shadow offscreen blur pipeline — always count=1 (ping/pong are count=1).
    blur_pipeline: BlurPipeline,
    /// Shadow composite pipeline — uses `ctx.sample_count` (must match screen target).
    composite_pipeline: CompositePipeline,
    /// Shadow mask solid pipeline — always count=1 (ping target is count=1).
    solid_mask_pipeline: SolidPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    clear_color: Color,
    /// Per-frame statistics populated by the most recent `execute()` call.
    last_frame_stats: FrameStats,
    /// Persistent solid vertex buffer (reused across frames, grown on demand).
    solid_vertex_buf: Option<wgpu::Buffer>,
    /// Byte capacity of `solid_vertex_buf`.
    solid_vertex_buf_capacity: usize,
    /// Lazily-initialised CPU text bridge for pre-expanding `DrawText` commands
    /// into per-glyph `Image` blits.  `None` until the first `DrawText` command
    /// is encountered or until a bridge is successfully initialised.  Requires
    /// the `text` Cargo feature.
    #[cfg(feature = "text")]
    text_bridge: Option<TextBridge>,
}

impl WgpuBackend {
    /// Initialise a headless backend with an offscreen target of
    /// `width × height` physical pixels, using the provided
    /// [`crate::quality::RenderQuality`] to determine the MSAA sample count.
    ///
    /// Screen pipelines (solid, gradient, textured, composite) are created with
    /// the effective sample count from `quality`.  Shadow offscreen pipelines
    /// (blur and solid_mask) are always count=1 because ping-pong textures are
    /// always single-sample.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] when no GPU adapter is available (so
    /// the caller can skip on a machine without a usable GPU), or
    /// [`UiError::Backend`] when device creation fails.
    pub fn headless_with_quality(
        width: u32,
        height: u32,
        quality: &crate::RenderQuality,
    ) -> Result<Self, UiError> {
        let sc = quality.sample_count();
        let ctx = GpuContext::headless_with_sample_count(width, height, sc)?;
        // Screen pipelines use the effective sample count.
        let pipeline = SolidPipeline::new(&ctx.device, ctx.sample_count);
        let gradient_pipeline = GradientPipeline::new(&ctx.device, ctx.sample_count);
        let textured_pipeline = TexturedPipeline::new(&ctx.device, ctx.sample_count);
        let composite_pipeline = CompositePipeline::new(&ctx.device, ctx.sample_count);
        // Shadow offscreen pipelines MUST be count=1 (ping/pong are count=1 textures).
        let blur_pipeline = BlurPipeline::new(&ctx.device, 1);
        let solid_mask_pipeline = SolidPipeline::new(&ctx.device, 1);

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
            textured_pipeline,
            blur_pipeline,
            composite_pipeline,
            solid_mask_pipeline,
            globals_buffer,
            globals_bind_group,
            clear_color: Color(0, 0, 0, 0),
            last_frame_stats: FrameStats::default(),
            solid_vertex_buf: None,
            solid_vertex_buf_capacity: 0,
            #[cfg(feature = "text")]
            text_bridge: None,
        })
    }

    /// Initialise a headless backend with an offscreen target of
    /// `width × height` physical pixels, using [`crate::quality::RenderQuality::low`] (no
    /// MSAA, sample_count=1).
    ///
    /// This is the backward-compatible entry point.  It delegates to
    /// [`headless_with_quality`] with `RenderQuality::low()`, so existing
    /// callers receive the exact same code path as before MSAA support was
    /// added.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] when no GPU adapter is available (so
    /// the caller can skip on a machine without a usable GPU), or
    /// [`UiError::Backend`] when device creation fails.
    ///
    /// [`headless_with_quality`]: WgpuBackend::headless_with_quality
    pub fn headless(width: u32, height: u32) -> Result<Self, UiError> {
        Self::headless_with_quality(width, height, &crate::RenderQuality::low())
    }

    /// Returns a reference to the underlying [`GpuContext`].
    pub fn ctx(&self) -> &GpuContext {
        &self.ctx
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

    /// Return the per-frame statistics populated by the most recent
    /// [`execute`] call.
    ///
    /// Statistics are reset to zero at the start of each `execute()` and
    /// incrementally updated as GPU passes are issued.
    ///
    /// [`execute`]: WgpuBackend::execute
    pub fn frame_stats(&self) -> FrameStats {
        self.last_frame_stats
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

    /// Resize the headless offscreen target to `new_width × new_height` pixels.
    ///
    /// Recreates only the offscreen colour texture (and the MSAA texture if
    /// active).  The `wgpu::Device`, `Queue`, and compiled pipelines are
    /// preserved — only size-dependent GPU resources are rebuilt.
    ///
    /// All texture views obtained from this backend before the resize become
    /// invalid and must not be used afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] if either dimension is zero.
    pub fn resize(&mut self, new_width: u32, new_height: u32) -> Result<(), UiError> {
        // Resize the colour textures in-place (device/queue/pipelines unchanged).
        self.ctx.resize(new_width, new_height)?;

        // Update the globals uniform buffer with the new viewport size.
        let globals = Globals::new(new_width, new_height);
        self.ctx
            .queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        // Invalidate the persistent solid vertex buffer so it is reallocated on
        // the next frame (the old buffer remains valid; we just stop using it).
        self.solid_vertex_buf = None;
        self.solid_vertex_buf_capacity = 0;

        Ok(())
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

    fn supports_images(&self) -> bool {
        true
    }

    fn supports_blur(&self) -> bool {
        true
    }

    fn supports_blend_modes(&self) -> bool {
        true
    }

    fn supports_backdrop_blur(&self) -> bool {
        true
    }

    fn supports_text(&self) -> bool {
        // Text is supported when the `text` feature is enabled; the bridge is
        // initialised lazily on first use.
        cfg!(feature = "text")
    }

    fn execute(&mut self, list: &DrawList) -> Result<(), UiError> {
        // Reset per-frame stats at the start of each execute().
        self.last_frame_stats = FrameStats::default();

        // Update the viewport globals uniform.
        let globals = Globals::new(self.ctx.width, self.ctx.height);
        self.ctx
            .queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        // ── Text pre-expansion ────────────────────────────────────────────────
        // When the `text` feature is enabled, expand any `DrawText` commands
        // into per-glyph `Image` blits *before* geometry building so that
        // `build_geometry` sees only `Image`/`NineSlice` commands for text.
        //
        // The bridge is lazily initialised on the first frame that contains
        // a `DrawText` command, using the first available system font family.
        // If no system font is found the bridge stays `None` and `DrawText`
        // commands are silently skipped (same behaviour as before).
        #[cfg(feature = "text")]
        let expanded_list: DrawList;
        #[cfg(feature = "text")]
        let list: &DrawList = {
            use oxiui_core::paint::DrawCommand;
            let has_text = list
                .iter()
                .any(|c| matches!(c, DrawCommand::DrawText { .. }));
            if has_text {
                // Lazily initialise the bridge from a system font.
                if self.text_bridge.is_none() {
                    // Try common system font families in order.
                    let candidates = &[
                        "Helvetica",
                        "Arial",
                        "DejaVu Sans",
                        "Liberation Sans",
                        "sans-serif",
                    ];
                    for &family in candidates {
                        if let Ok(pipeline) = TextPipeline::from_system_font(family) {
                            self.text_bridge = Some(TextBridge::new(pipeline, 1024));
                            break;
                        }
                    }
                }
                if let Some(bridge) = &mut self.text_bridge {
                    expanded_list = bridge.expand_draw_text_commands(list);
                    &expanded_list
                } else {
                    list
                }
            } else {
                list
            }
        };

        let (verts, segments, gradient_draws, textured_draws, _backdrop_blur_draws) =
            build_geometry(list, self.ctx.width, self.ctx.height);

        let clear = self.clear_color;
        let clear_value = wgpu::Color {
            r: clear.0 as f64 / 255.0,
            g: clear.1 as f64 / 255.0,
            b: clear.2 as f64 / 255.0,
            a: clear.3 as f64 / 255.0,
        };

        // Obtain the screen colour attachment.  Under MSAA this is
        // (msaa_view, Some(color_view)); under no MSAA it is (color_view, None).
        let (screen_view, screen_resolve) = self.ctx.color_attachment();

        // ── Pass 0: Dedicated clear ───────────────────────────────────────────
        // We separate the clear from the solid draw pass so that shadow passes
        // (which use LoadOp::Load) can composite onto the cleared target *before*
        // the solid/gradient/textured content is drawn on top.
        //
        // Under MSAA we clear the MSAA surface directly so the resolve target
        // also ends up cleared after any subsequent resolve.
        {
            let mut encoder =
                self.ctx
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("oxiui-render-wgpu clear encoder"),
                    });
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oxiui-render-wgpu clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: screen_view,
                    depth_slice: None,
                    resolve_target: screen_resolve,
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
            // No draws — clear only.
            drop(_pass);
            self.ctx.queue.submit(Some(encoder.finish()));
        }
        // Count the clear pass.
        self.last_frame_stats.render_passes += 1;

        // ── Passes 1-N: Shadow composites ─────────────────────────────────────
        // Each shadow submits its own command encoders internally.  These are
        // submitted before the main-frame encoder so shadows appear under content.
        //
        // The shadow composite pass writes to the screen target (which may be
        // the MSAA surface when MSAA is active), so we pass both `screen_view`
        // and `screen_resolve` through `ShadowGpuState`.
        let shadows = crate::gpu::shadow::collect_shadows(list);
        let shadow_gpu = crate::gpu::shadow::ShadowGpuState {
            device: &self.ctx.device,
            queue: &self.ctx.queue,
            target_view: screen_view,
            resolve_target: screen_resolve,
            globals_buffer: &self.globals_buffer,
            globals_bind_group: &self.globals_bind_group,
            viewport_w: self.ctx.width,
            viewport_h: self.ctx.height,
        };
        let shadow_pipelines = crate::gpu::shadow::ShadowPipelines {
            // Mask pass uses solid_mask_pipeline (count=1, ping is count=1).
            solid: &self.solid_mask_pipeline,
            blur: &self.blur_pipeline,
            // Composite pass uses composite_pipeline (count=ctx.sample_count, writes to screen).
            composite: &self.composite_pipeline,
        };
        let shadow_stats =
            crate::gpu::shadow::render_shadows(&shadow_gpu, &shadow_pipelines, &shadows)?;
        self.last_frame_stats.render_passes += shadow_stats.render_passes;
        self.last_frame_stats.draw_calls += shadow_stats.draw_calls;

        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("oxiui-render-wgpu frame encoder"),
            });

        // Re-borrow after submitting the clear encoder above.
        let (screen_view2, screen_resolve2) = self.ctx.color_attachment();

        // ── Solid pass (LoadOp::Load — clear already done above) ──────────────
        let solid_draws = run_solid_pass(SolidPassParams {
            device: &self.ctx.device,
            queue: &self.ctx.queue,
            encoder: &mut encoder,
            screen_view: screen_view2,
            screen_resolve: screen_resolve2,
            pipeline: &self.pipeline,
            globals_bind_group: &self.globals_bind_group,
            verts: &verts,
            segments: &segments,
            viewport_w: self.ctx.width,
            viewport_h: self.ctx.height,
            solid_vertex_buf: &mut self.solid_vertex_buf,
            solid_vertex_buf_capacity: &mut self.solid_vertex_buf_capacity,
        });
        // Count the solid pass itself (it is always opened, even when empty).
        self.last_frame_stats.render_passes += 1;
        self.last_frame_stats.draw_calls += solid_draws;

        // ── Gradient pass (all gradient draws coalesced into one pass) ────────
        {
            let (sv, sr) = self.ctx.color_attachment();
            let (rp, dc) = run_gradient_pass_batched(GradientPassParams {
                device: &self.ctx.device,
                queue: &self.ctx.queue,
                encoder: &mut encoder,
                screen_view: sv,
                screen_resolve: sr,
                pipeline: &self.gradient_pipeline,
                globals_buffer: &self.globals_buffer,
                gradient_draws: &gradient_draws,
                viewport_w: self.ctx.width,
                viewport_h: self.ctx.height,
            });
            self.last_frame_stats.render_passes += rp;
            self.last_frame_stats.draw_calls += dc;
        }

        // ── Textured pass (one render pass per textured draw) ─────────────────
        for td in &textured_draws {
            let (sv, sr) = self.ctx.color_attachment();
            let (rp, dc) = run_textured_pass(TexturedPassParams {
                device: &self.ctx.device,
                queue: &self.ctx.queue,
                encoder: &mut encoder,
                screen_view: sv,
                screen_resolve: sr,
                pipeline: &self.textured_pipeline,
                globals_bind_group: &self.globals_bind_group,
                td,
                viewport_w: self.ctx.width,
                viewport_h: self.ctx.height,
            })?;
            self.last_frame_stats.render_passes += rp;
            self.last_frame_stats.draw_calls += dc;
        }

        self.ctx.queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::geometry::{Point, Rect};
    use oxiui_core::paint::{DrawCommand, DrawList, FillRule, GradientStop, PathData, StrokeStyle};
    use oxiui_core::Color;

    // ── MSAA tests ────────────────────────────────────────────────────────────

    #[test]
    fn msaa_smooths_diagonal_edge() {
        // Build a 4x MSAA backend. If MSAA is not supported by the adapter, we
        // may get sample_count=1 (fallback), in which case we skip the
        // intermediate-alpha assertion but still pass.
        let Some(mut b) =
            WgpuBackend::headless_with_quality(64, 64, &crate::RenderQuality::balanced()).ok()
        else {
            return;
        };
        let mut list = DrawList::new();
        // Draw a filled right-triangle with a 45° diagonal edge.
        let red = Color(255, 0, 0, 255);
        let mut path = PathData::new();
        path.move_to(Point::new(0.0, 0.0));
        path.line_to(Point::new(63.0, 0.0));
        path.line_to(Point::new(0.0, 63.0));
        path.close();
        list.push_path(path, red);
        b.execute(&list).expect("execute");
        let buf = b.readback_rgba().expect("readback");
        let w = b.width();
        let pixel = |x: u32, y: u32| -> (u8, u8, u8, u8) {
            let i = ((y * w + x) * 4) as usize;
            (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
        };
        if b.ctx().sample_count() > 1 {
            // With MSAA: at least one diagonal-edge pixel should have
            // intermediate alpha (0 < a < 255).
            let mut found_intermediate = false;
            for d in 5u32..58u32 {
                let p = pixel(d, d);
                if p.3 > 0 && p.3 < 255 {
                    found_intermediate = true;
                    break;
                }
            }
            assert!(
                found_intermediate,
                "MSAA should produce intermediate-alpha pixels on diagonal edge"
            );
        }
        // Fully-inside pixel should be full red.
        let inside = pixel(5, 5);
        assert_eq!(inside.3, 255, "inside pixel must be fully opaque");
    }

    #[test]
    fn non_msaa_edge_is_hard() {
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        let mut list = DrawList::new();
        let red = Color(255, 0, 0, 255);
        let mut path = PathData::new();
        path.move_to(Point::new(0.0, 0.0));
        path.line_to(Point::new(63.0, 0.0));
        path.line_to(Point::new(0.0, 63.0));
        path.close();
        list.push_path(path, red);
        b.execute(&list).expect("execute");
        let buf = b.readback_rgba().expect("readback");
        let w = b.width();
        // No MSAA: all pixels on the diagonal must be either fully opaque or
        // fully transparent.
        for d in 0u32..64u32 {
            let i = ((d * w + d) * 4) as usize;
            let a = buf[i + 3];
            assert!(
                a == 0 || a == 255,
                "non-MSAA edge pixel at ({d},{d}) must be 0 or 255, got {a}"
            );
        }
    }

    #[test]
    fn msaa_default_path_unchanged() {
        // headless() = RenderQuality::low() = msaa=1 → sample_count=1,
        // byte-identical path.
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        assert_eq!(
            b.ctx().sample_count(),
            1,
            "headless() must use sample_count=1"
        );
        let mut list = DrawList::new();
        list.push_rect(Rect::new(10.0, 10.0, 20.0, 20.0), Color(255, 0, 0, 255));
        b.execute(&list).expect("execute");
        let px = b.read_pixel(20, 20).expect("read").expect("pixel");
        assert_eq!(
            (px.0, px.1, px.2, px.3),
            (255, 0, 0, 255),
            "basic rect fill must still work"
        );
    }

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

    #[test]
    fn image_solid_fill_readback() {
        use oxiui_core::paint::{DrawList, ImageData, ImageFilter};
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        // 2x2 solid red image
        let image = ImageData::new(
            vec![
                255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
            ],
            2,
            2,
        );
        let mut dl = DrawList::new();
        dl.push_image(
            image,
            Rect::new(12.0, 12.0, 40.0, 40.0),
            ImageFilter::Nearest,
        );
        b.execute(&dl).expect("execute ok");
        let px = b.read_pixel(32, 32).expect("ok").expect("bounds");
        assert!(px.0 > 200 && px.3 > 200, "centre should be red: {px:?}");
        assert_transparent(&b, 2, 2, "outside image");
    }

    #[test]
    fn nine_slice_renders() {
        use oxiui_core::paint::{DrawList, ImageData};
        let Some(mut b) = try_backend(128, 128) else {
            return;
        };
        // 12x12 image: red corners (4px), blue centre
        let mut rgba = vec![0u8; 12 * 12 * 4];
        for y in 0..12u32 {
            for x in 0..12u32 {
                let i = ((y * 12 + x) * 4) as usize;
                let corner = !(4..8).contains(&x) || !(4..8).contains(&y);
                if corner {
                    rgba[i] = 255;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 0;
                    rgba[i + 3] = 255; // red
                } else {
                    rgba[i] = 0;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 255;
                    rgba[i + 3] = 255; // blue
                }
            }
        }
        let image = ImageData::new(rgba, 12, 12);
        let mut dl = DrawList::new();
        dl.push_nine_slice(image, Rect::new(0.0, 0.0, 128.0, 128.0), [4, 4, 4, 4]);
        b.execute(&dl).expect("execute ok");
        // Corner region should be reddish
        let corner = b.read_pixel(2, 2).expect("ok").expect("bounds");
        assert!(corner.0 > 100, "corner should be reddish: {corner:?}");
        // Centre region should be bluish
        let centre = b.read_pixel(64, 64).expect("ok").expect("bounds");
        assert!(centre.2 > 100, "centre should be bluish: {centre:?}");
    }

    #[test]
    fn tex_vertex_size_is_32() {
        use crate::gpu::buffer::TexVertex;
        assert_eq!(core::mem::size_of::<TexVertex>(), 32);
    }

    // ── BoxShadow tests ───────────────────────────────────────────────────────

    #[test]
    fn box_shadow_zero_blur_is_sharp() {
        let Some(mut b) = try_backend(128, 128) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push_shadow(
            Rect::new(20.0, 20.0, 80.0, 80.0),
            Point::new(0.0, 0.0),
            0.0,
            Color(0, 0, 0, 200),
        );
        b.execute(&dl).expect("execute ok");
        // Interior: shadow visible
        let interior = b.read_pixel(60, 60).expect("ok").expect("bounds");
        assert!(interior.3 > 100, "interior should be visible: {interior:?}");
        // Far outside: transparent
        let outside = b.read_pixel(5, 5).expect("ok").expect("bounds");
        assert!(outside.3 == 0, "outside should be transparent: {outside:?}");
    }

    #[test]
    fn box_shadow_blur_halo_falloff() {
        let Some(mut b) = try_backend(200, 200) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push_shadow(
            Rect::new(50.0, 50.0, 100.0, 100.0),
            Point::new(0.0, 0.0),
            12.0,
            Color(0, 0, 0, 255),
        );
        b.execute(&dl).expect("execute ok");
        // Near-interior: high alpha
        let interior = b.read_pixel(100, 100).expect("ok").expect("bounds");
        assert!(interior.3 > 100, "interior should be visible: {interior:?}");
        // Just outside: some alpha (blur halo)
        let edge = b.read_pixel(45, 100).expect("ok").expect("bounds");
        // Far outside: low/zero alpha
        let far = b.read_pixel(5, 5).expect("ok").expect("bounds");
        assert!(far.3 < edge.3, "falloff: far={far:?} edge={edge:?}");
    }

    #[test]
    fn box_shadow_offset_translates() {
        let Some(mut b) = try_backend(200, 200) else {
            return;
        };
        let mut dl = DrawList::new();
        dl.push_shadow(
            Rect::new(50.0, 50.0, 80.0, 80.0),
            Point::new(20.0, 20.0),
            0.0,
            Color(0, 0, 0, 255),
        );
        b.execute(&dl).expect("execute ok");
        // Original rect position (before offset) should be transparent
        let orig_pos = b.read_pixel(55, 55).expect("ok").expect("bounds");
        assert!(
            orig_pos.3 == 0,
            "original rect pos should be transparent: {orig_pos:?}"
        );
        // Offset position should be visible
        let offset_pos = b.read_pixel(80, 80).expect("ok").expect("bounds");
        assert!(
            offset_pos.3 > 100,
            "offset pos should be visible: {offset_pos:?}"
        );
    }

    #[test]
    fn shadows_render_under_solids() {
        let Some(mut b) = try_backend(200, 200) else {
            return;
        };
        let mut dl = DrawList::new();
        // Shadow covering most of the viewport
        dl.push_shadow(
            Rect::new(10.0, 10.0, 180.0, 180.0),
            Point::new(0.0, 0.0),
            0.0,
            Color(255, 0, 0, 255), // red shadow
        );
        // Blue rect covering the shadow area
        dl.push(DrawCommand::FillRect {
            rect: Rect::new(10.0, 10.0, 180.0, 180.0),
            color: Color(0, 0, 255, 255), // solid blue
        });
        b.execute(&dl).expect("execute ok");
        // The blue rect should be on top — pixel should show blue, not red
        let px = b.read_pixel(100, 100).expect("ok").expect("bounds");
        assert!(
            px.2 > 200 && px.0 < 100,
            "blue rect should be on top: {px:?}"
        );
    }

    #[test]
    fn fill_path_concave_notch_empty() {
        // Arrow/chevron shape: concave polygon with a notch at the bottom.
        // The notch interior pixel should be transparent.
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        let mut list = DrawList::new();
        let red = Color(255, 0, 0, 255);
        // Concave polygon (CCW): (5,5) (59,5) (59,59) (32,40) (5,59) — concave at (32,40).
        let mut path = PathData::new();
        path.move_to(Point::new(5.0, 5.0));
        path.line_to(Point::new(59.0, 5.0));
        path.line_to(Point::new(59.0, 59.0));
        path.line_to(Point::new(32.0, 40.0)); // concave vertex (notch tip)
        path.line_to(Point::new(5.0, 59.0));
        path.close();
        list.push_path(path, red);
        b.execute(&list).expect("execute");
        // Top body pixel should be red.
        let body = b.read_pixel(32, 10).expect("read").expect("pixel");
        assert_eq!(body.3, 255, "body should be opaque");
        // Pixel deep in the notch should be transparent.
        let notch = b.read_pixel(32, 55).expect("read").expect("pixel");
        assert_eq!(
            notch.3, 0,
            "notch must be transparent (concave fill correct)"
        );
    }

    #[test]
    fn fill_path_donut_hole_empty() {
        // Outer CCW ring (big square) + inner CW ring (small square) = donut.
        // Centre pixel must be transparent under NonZero (CW inner = opposite winding → hole).
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        let mut list = DrawList::new();
        let blue = Color(0, 0, 255, 255);
        let mut path = PathData::new();
        // Outer CCW
        path.move_to(Point::new(4.0, 4.0));
        path.line_to(Point::new(60.0, 4.0));
        path.line_to(Point::new(60.0, 60.0));
        path.line_to(Point::new(4.0, 60.0));
        path.close();
        // Inner CW (hole): reversed winding
        path.move_to(Point::new(20.0, 20.0));
        path.line_to(Point::new(20.0, 44.0));
        path.line_to(Point::new(44.0, 44.0));
        path.line_to(Point::new(44.0, 20.0));
        path.close();
        list.push_path(path, blue);
        b.execute(&list).expect("execute");
        // Outer ring should be blue.
        let ring = b.read_pixel(10, 10).expect("read").expect("pixel");
        assert_eq!(
            (ring.0, ring.1, ring.2, ring.3),
            (0, 0, 255, 255),
            "ring must be blue"
        );
        // Hole centre must be transparent.
        let hole = b.read_pixel(32, 32).expect("read").expect("pixel");
        assert_eq!(hole.3, 0, "donut hole must be transparent");
    }

    #[test]
    fn fill_rule_evenodd_vs_nonzero() {
        // Same-winding nested rings: outer CCW + inner CCW.
        // EvenOdd: inner is at depth 1 → hole → inner pixel transparent.
        // NonZero: inner winding sum = +2 → filled → inner pixel opaque.
        let Some(mut b_eo) = try_backend(64, 64) else {
            return;
        };
        let Some(mut b_nz) = try_backend(64, 64) else {
            return;
        };
        let make_path = |fill_rule: FillRule| {
            let mut path = PathData::new().with_fill_rule(fill_rule);
            // Outer CCW
            path.move_to(Point::new(4.0, 4.0));
            path.line_to(Point::new(60.0, 4.0));
            path.line_to(Point::new(60.0, 60.0));
            path.line_to(Point::new(4.0, 60.0));
            path.close();
            // Inner CCW (same winding as outer)
            path.move_to(Point::new(20.0, 20.0));
            path.line_to(Point::new(44.0, 20.0));
            path.line_to(Point::new(44.0, 44.0));
            path.line_to(Point::new(20.0, 44.0));
            path.close();
            path
        };
        let green = Color(0, 255, 0, 255);
        let mut list_eo = DrawList::new();
        list_eo.push_path(make_path(FillRule::EvenOdd), green);
        let mut list_nz = DrawList::new();
        list_nz.push_path(make_path(FillRule::NonZero), green);
        b_eo.execute(&list_eo).expect("execute");
        b_nz.execute(&list_nz).expect("execute");
        // Inner centre pixel
        let inner_eo = b_eo.read_pixel(32, 32).expect("read").expect("pixel");
        let inner_nz = b_nz.read_pixel(32, 32).expect("read").expect("pixel");
        // EvenOdd: inner CCW ring is at depth 1 → hole → transparent.
        assert_eq!(
            inner_eo.3, 0,
            "EvenOdd: same-winding inner ring must be transparent (depth=1 = hole)"
        );
        // NonZero: inner CCW ring winding sum = +1 (outer) + +1 (inner) = +2 ≠ 0 → filled.
        assert_eq!(
            inner_nz.3, 255,
            "NonZero: same-winding inner ring must be opaque (winding=2 ≠ 0)"
        );
    }

    // ── Visibility culling tests ───────────────────────────────────────────────

    #[test]
    fn culled_offscreen_rect_is_transparent() {
        // A FillRect placed entirely outside the active clip region should
        // produce no visible pixels — the visibility culling optimisation
        // must discard it before vertices are emitted.
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        let mut list = DrawList::new();
        // Active clip: top-left 32×32 quadrant.
        list.push_clip(Rect::new(0.0, 0.0, 32.0, 32.0));
        // Draw a rect entirely in the bottom-right quadrant → outside the clip.
        list.push_rect(Rect::new(40.0, 40.0, 20.0, 20.0), Color(255, 0, 0, 255));
        list.pop_clip();
        b.execute(&list).expect("execute");
        // Pixel inside the culled rect should remain transparent.
        let px = b.read_pixel(45, 45).expect("read").expect("pixel");
        assert_eq!(px.3, 0, "rect outside clip must be culled (transparent)");
        // Pixel in the clipped-but-undrawn top-left region also stays transparent.
        let px2 = b.read_pixel(10, 10).expect("read").expect("pixel");
        assert_eq!(px2.3, 0, "undrawn area must remain transparent");
    }

    #[test]
    fn culling_does_not_affect_visible_rect() {
        // Verify that visibility culling does not accidentally discard a rect
        // that lies within the viewport (no active scissor → no culling).
        let Some(mut b) = try_backend(64, 64) else {
            return;
        };
        let mut list = DrawList::new();
        list.push_rect(Rect::new(10.0, 10.0, 40.0, 40.0), Color(0, 255, 0, 255));
        b.execute(&list).expect("execute");
        let px = b.read_pixel(30, 30).expect("read").expect("pixel");
        assert_eq!(
            (px.0, px.1, px.2, px.3),
            (0, 255, 0, 255),
            "visible rect must not be culled"
        );
    }

    // ── FrameStats tests ──────────────────────────────────────────────────────

    #[test]
    fn frame_stats_counts_solid_draws() {
        let Some(mut backend) = try_backend(64, 64) else {
            return;
        };
        // One solid rect with no clip changes = one DrawSegment = one draw
        let mut list = DrawList::new();
        list.push(DrawCommand::FillRect {
            rect: Rect::new(10.0, 10.0, 44.0, 44.0),
            color: Color(255, 0, 0, 255),
        });
        backend.execute(&list).expect("execute failed");
        let stats = backend.frame_stats();
        assert!(stats.draw_calls >= 1, "should have at least 1 draw call");
        assert!(
            stats.render_passes >= 1,
            "should have at least 1 render pass"
        );
    }

    // ── R2: Draw-call batching tests ─────────────────────────────────────────

    #[test]
    fn two_gradients_one_pass() {
        let Some(mut backend) = try_backend(128, 64) else {
            return;
        };
        let mut list = DrawList::new();
        // Left half: linear gradient red→blue
        list.push(DrawCommand::LinearGradient {
            rect: Rect::new(0.0, 0.0, 64.0, 64.0),
            start: Point::new(0.0, 0.0),
            end: Point::new(64.0, 0.0),
            stops: vec![
                GradientStop::new(0.0, Color(255, 0, 0, 255)),
                GradientStop::new(1.0, Color(0, 0, 255, 255)),
            ],
        });
        // Right half: linear gradient green→yellow
        list.push(DrawCommand::LinearGradient {
            rect: Rect::new(64.0, 0.0, 64.0, 64.0),
            start: Point::new(64.0, 0.0),
            end: Point::new(128.0, 0.0),
            stops: vec![
                GradientStop::new(0.0, Color(0, 255, 0, 255)),
                GradientStop::new(1.0, Color(255, 255, 0, 255)),
            ],
        });
        backend.execute(&list).expect("execute");
        let stats = backend.frame_stats();
        // Both gradients should produce at least 2 draw calls
        assert!(
            stats.draw_calls >= 2,
            "should have at least 2 draw calls for 2 gradients, got {}",
            stats.draw_calls
        );

        // Left side near x=2: gradient starts as red
        let left_px = backend
            .read_pixel(2, 32)
            .expect("read left")
            .expect("bounds");
        assert!(left_px.0 > 200, "left should be reddish, got {:?}", left_px);
        assert!(
            left_px.2 < 100,
            "left should not be blue, got {:?}",
            left_px
        );

        // Right side near x=66: gradient starts as green
        let right_px = backend
            .read_pixel(66, 32)
            .expect("read right")
            .expect("bounds");
        assert!(
            right_px.1 > 200,
            "right should be greenish, got {:?}",
            right_px
        );
        assert!(
            right_px.0 < 100,
            "right should not be red, got {:?}",
            right_px
        );
    }

    #[test]
    fn gradient_byte_exact_single() {
        // A single gradient must produce correct pixel values through the
        // batched path (offset=0 dynamic offset invariant).
        let Some(mut backend) = try_backend(64, 64) else {
            return;
        };
        let mut list = DrawList::new();
        list.push(DrawCommand::LinearGradient {
            rect: Rect::new(0.0, 0.0, 64.0, 64.0),
            start: Point::new(0.0, 0.0),
            end: Point::new(64.0, 0.0),
            stops: vec![
                GradientStop::new(0.0, Color(255, 0, 0, 255)),
                GradientStop::new(1.0, Color(0, 0, 255, 255)),
            ],
        });
        backend.execute(&list).expect("execute");
        // Left edge ~red
        let left = backend
            .read_pixel(1, 32)
            .expect("read left")
            .expect("bounds");
        assert!(
            left.0 > 200 && left.2 < 100,
            "left should be reddish: {:?}",
            left
        );
        // Right edge ~blue
        let right = backend
            .read_pixel(62, 32)
            .expect("read right")
            .expect("bounds");
        assert!(
            right.2 > 200 && right.0 < 100,
            "right should be bluish: {:?}",
            right
        );
        // Mid should have intermediate values (not pure red or pure blue)
        let mid = backend
            .read_pixel(32, 32)
            .expect("read mid")
            .expect("bounds");
        assert!(mid.3 > 0, "mid should be visible: {:?}", mid);
    }

    #[test]
    fn persistent_buffer_reuse_stable() {
        // Render two consecutive frames with different primitive counts;
        // both must be correct (stale tail from frame 1 must not bleed into frame 2).
        let Some(mut backend) = try_backend(64, 64) else {
            return;
        };

        // Frame 1: 10 rects filling left strips
        let mut list1 = DrawList::new();
        for i in 0..10u32 {
            list1.push(DrawCommand::FillRect {
                rect: Rect::new(i as f32 * 4.0, 0.0, 4.0, 64.0),
                color: Color(255, 0, 0, 255),
            });
        }
        backend.execute(&list1).expect("frame 1");
        let px1 = backend
            .read_pixel(2, 32)
            .expect("frame 1 pixel")
            .expect("bounds");
        assert_eq!(px1.0, 255, "frame 1 should be red: {:?}", px1);
        assert_eq!(px1.3, 255, "frame 1 should be opaque: {:?}", px1);

        // Frame 2: single blue rect covering the whole canvas
        let mut list2 = DrawList::new();
        list2.push(DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 64.0, 64.0),
            color: Color(0, 0, 255, 255),
        });
        backend.execute(&list2).expect("frame 2");
        let px2 = backend
            .read_pixel(32, 32)
            .expect("frame 2 pixel")
            .expect("bounds");
        assert_eq!(px2.2, 255, "frame 2 should be blue: {:?}", px2);
        // Stale-tail check: no red from the previous frame's extra vertices
        assert!(
            px2.0 < 10,
            "frame 2 stale-tail check: should not see red, got {:?}",
            px2
        );
    }
}
