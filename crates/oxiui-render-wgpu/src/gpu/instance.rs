//! Instanced rendering for repeated UI primitives.
//!
//! [`InstancedRectPipeline`] renders many axis-aligned rectangles (with
//! optional uniform corner radius) using a single indexed quad mesh and a
//! per-instance vertex buffer.  This is much more efficient than emitting
//! individual `Vertex` quads when rendering large numbers of identical
//! primitives (buttons, table cells, list items).
//!
//! # Pipeline design
//!
//! - **Mesh vertex buffer** (step_mode=Vertex): 4 corners of a unit quad as
//!   `[f32; 2]` UV coordinates in `[0,1]²`.  Reused across all instances.
//! - **Index buffer**: 6 indices forming 2 triangles for the unit quad.
//! - **Instance vertex buffer** (step_mode=Instance): one [`InstanceRect`] per
//!   rectangle, carrying position, size, colour, and corner radius.
//!
//! The vertex shader computes the pixel-space position from `inst_pos +
//! uv * inst_size` and applies the 2-D orthographic projection.  The fragment
//! shader runs a rounded-rect SDF when `corner_radius > 0`.
//!
//! # Usage
//!
//! ```rust,ignore
//! let pipeline = InstancedRectPipeline::new(&device, sample_count);
//! let mut renderer = InstancedRectRenderer::new(&device, 256);
//! renderer.push(InstanceRect { pos: [10.0, 10.0], size: [80.0, 30.0],
//!                               color: [1.0, 0.0, 0.0, 1.0], corner_radius: 4.0 });
//! renderer.flush(&device, &queue, &mut encoder, &pipeline, &globals_bind_group, ...);
//! ```

use bytemuck::{Pod, Zeroable};
use oxiui_core::UiError;
use wgpu::util::DeviceExt;

use crate::gpu::device::TARGET_FORMAT;

// ── InstanceRect ──────────────────────────────────────────────────────────────

/// Per-instance data for a single instanced rectangle.
///
/// 36 bytes, `#[repr(C)]`, `Pod` + `Zeroable` so it can be uploaded directly.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct InstanceRect {
    /// Top-left corner in pixel space.
    pub pos: [f32; 2],
    /// Width × height in pixels.
    pub size: [f32; 2],
    /// Straight-alpha RGBA colour in `[0, 1]`.
    pub color: [f32; 4],
    /// Uniform corner radius in pixels (0 = sharp).
    pub corner_radius: f32,
    /// Padding to align to 4 bytes.
    pub _pad: [f32; 3],
}

// Compile-time size assert: InstanceRect must be 48 bytes (4+4+4+4+4+4+4+4 × 4
// = 12 × 4 = 48).
const _: () = assert!(core::mem::size_of::<InstanceRect>() == 48);

impl InstanceRect {
    /// Construct an instance with no corner radius.
    pub fn rect(pos: [f32; 2], size: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            pos,
            size,
            color,
            corner_radius: 0.0,
            _pad: [0.0; 3],
        }
    }

    /// Construct an instance with a uniform corner radius.
    pub fn rounded(pos: [f32; 2], size: [f32; 2], color: [f32; 4], corner_radius: f32) -> Self {
        Self {
            pos,
            size,
            color,
            corner_radius,
            _pad: [0.0; 3],
        }
    }
}

// ── UvVertex ─────────────────────────────────────────────────────────────────

/// A single mesh vertex: just a 2-D UV coordinate in `[0,1]²`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct UvVertex {
    uv: [f32; 2],
}

/// The four corners of the unit quad, counter-clockwise.
const QUAD_VERTICES: [UvVertex; 4] = [
    UvVertex { uv: [0.0, 0.0] }, // top-left
    UvVertex { uv: [1.0, 0.0] }, // top-right
    UvVertex { uv: [1.0, 1.0] }, // bottom-right
    UvVertex { uv: [0.0, 1.0] }, // bottom-left
];

/// Two triangles from the quad vertices: [0,1,2] and [0,2,3].
const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

// ── InstancedRectPipeline ─────────────────────────────────────────────────────

/// The compiled instanced-rect render pipeline.
pub struct InstancedRectPipeline {
    /// The render pipeline.
    pub pipeline: wgpu::RenderPipeline,
    /// Bind group layout for bind group 0 (the viewport `Globals` uniform).
    pub globals_layout: wgpu::BindGroupLayout,
    /// The shared unit-quad index buffer.
    pub index_buffer: wgpu::Buffer,
    /// The shared unit-quad vertex buffer.
    pub vertex_buffer: wgpu::Buffer,
}

impl InstancedRectPipeline {
    /// Build the instanced-rect pipeline for a colour target in [`TARGET_FORMAT`].
    ///
    /// `sample_count` controls MSAA (1 = no MSAA, 4 or 8 = MSAA).
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu instanced.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/instanced.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu instanced globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxiui-render-wgpu instanced pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });

        // Per-instance attributes (step_mode = Instance):
        //   pos           vec2  @0  offset  0
        //   size          vec2  @1  offset  8
        //   color         vec4  @2  offset 16
        //   corner_radius f32   @3  offset 32
        //   (3 × f32 pad)           offset 36
        let instance_attrs = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 32,
                shader_location: 3,
            },
        ];
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<InstanceRect>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attrs,
        };

        // Per-vertex attribute (step_mode = Vertex):
        //   uv  vec2  @4  offset 0
        let vertex_attrs = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 4,
        }];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<UvVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("oxiui-render-wgpu instanced pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                // Instance buffer first (per-instance), then quad vertex buffer.
                buffers: &[instance_layout, vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // Build the shared unit-quad buffers.
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu instanced quad vertices"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui-render-wgpu instanced quad indices"),
            contents: bytemuck::cast_slice(&QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            globals_layout,
            index_buffer,
            vertex_buffer,
        }
    }
}

// ── InstancedRectRenderer ─────────────────────────────────────────────────────

/// A frame-scoped collector of [`InstanceRect`] data and a flusher that
/// issues a single instanced draw call.
///
/// # Usage
///
/// 1. Call `push` once per rectangle to append to the batch.
/// 2. Call `flush` to upload the instance buffer and issue one
///    `draw_indexed(0..6, 0, 0..n)` call.
/// 3. Call `clear` at the start of the next frame (or after flush).
pub struct InstancedRectRenderer {
    instances: Vec<InstanceRect>,
    /// Persistent instance buffer, grown on demand (next power of two).
    instance_buf: Option<wgpu::Buffer>,
    /// Byte capacity of `instance_buf`.
    instance_buf_capacity: usize,
}

impl InstancedRectRenderer {
    /// Create a renderer pre-allocated for `initial_capacity` instances.
    pub fn new(initial_capacity: usize) -> Self {
        Self {
            instances: Vec::with_capacity(initial_capacity.max(4)),
            instance_buf: None,
            instance_buf_capacity: 0,
        }
    }

    /// Append one rectangle instance to the pending batch.
    pub fn push(&mut self, inst: InstanceRect) {
        self.instances.push(inst);
    }

    /// Return the number of pending instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Return `true` if there are no pending instances.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Clear all pending instances (call at the start of each frame).
    pub fn clear(&mut self) {
        self.instances.clear();
    }

    /// Upload the instance buffer and issue a single instanced draw call.
    ///
    /// Opens a render pass on `encoder`, uses `LoadOp::Load` so existing frame
    /// content is preserved.  The scissor is set to the full viewport.
    ///
    /// Returns the number of draw calls issued (0 if empty, 1 otherwise).
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Render`] if buffer creation fails.
    #[allow(clippy::too_many_arguments)]
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &InstancedRectPipeline,
        globals_bind_group: &wgpu::BindGroup,
        screen_view: &wgpu::TextureView,
        screen_resolve: Option<&wgpu::TextureView>,
        viewport_w: u32,
        viewport_h: u32,
    ) -> Result<u32, UiError> {
        if self.instances.is_empty() {
            return Ok(0);
        }

        // Upload instance data to a persistent, reused buffer.
        let inst_bytes: &[u8] = bytemuck::cast_slice(&self.instances);
        let needed = inst_bytes.len();

        let needs_grow = self.instance_buf.is_none() || self.instance_buf_capacity < needed;
        if needs_grow {
            let min_bytes = core::mem::size_of::<InstanceRect>() * 64;
            let new_cap = needed.next_power_of_two().max(min_bytes);
            self.instance_buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxiui-render-wgpu instanced-rects persistent"),
                size: new_cap as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.instance_buf_capacity = new_cap;
        }

        if let Some(ref buf) = self.instance_buf {
            queue.write_buffer(buf, 0, inst_bytes);
        }

        let n_instances = self.instances.len() as u32;

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxiui-render-wgpu instanced-rect pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: screen_view,
                depth_slice: None,
                resolve_target: screen_resolve,
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

        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, globals_bind_group, &[]);
        pass.set_scissor_rect(0, 0, viewport_w, viewport_h);

        if let Some(ref inst_buf) = self.instance_buf {
            pass.set_vertex_buffer(
                0,
                inst_buf.slice(..n_instances as u64 * core::mem::size_of::<InstanceRect>() as u64),
            );
        }
        pass.set_vertex_buffer(1, pipeline.vertex_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..n_instances);

        Ok(1)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_rect_size_is_48() {
        assert_eq!(core::mem::size_of::<InstanceRect>(), 48);
    }

    #[test]
    fn instance_rect_is_pod() {
        // Verify Pod/Zeroable derivation compiles and zeroed is valid.
        let _zero: InstanceRect = bytemuck::Zeroable::zeroed();
    }

    #[test]
    fn instance_rect_constructors() {
        let r = InstanceRect::rect([10.0, 20.0], [80.0, 30.0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(r.corner_radius, 0.0);
        assert_eq!(r.pos, [10.0, 20.0]);

        let rnd = InstanceRect::rounded([0.0, 0.0], [100.0, 100.0], [0.0, 1.0, 0.0, 1.0], 8.0);
        assert_eq!(rnd.corner_radius, 8.0);
    }

    #[test]
    fn instanced_renderer_push_and_clear() {
        let mut r = InstancedRectRenderer::new(4);
        assert!(r.is_empty());
        r.push(InstanceRect::rect([0.0, 0.0], [10.0, 10.0], [1.0; 4]));
        assert_eq!(r.len(), 1);
        r.clear();
        assert!(r.is_empty());
    }

    fn try_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("instanced test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    #[test]
    fn instanced_pipeline_compiles() {
        // Verify that the WGSL shader compiles without error on a real device.
        let Some((device, _queue)) = try_device() else {
            return;
        };
        let _pipeline = InstancedRectPipeline::new(&device, 1);
        // Reaching here means no WGSL compile error.
    }

    #[test]
    fn instanced_renderer_renders_rects() {
        use crate::gpu::buffer::Globals;
        use wgpu::util::DeviceExt;

        let Some((device, queue)) = try_device() else {
            return;
        };

        // Create a small offscreen render target.
        let w = 64u32;
        let h = 64u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("instanced test target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::gpu::device::TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let pipeline = InstancedRectPipeline::new(&device, 1);

        let globals = Globals::new(w, h);
        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instanced test globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("instanced test globals bg"),
            layout: &pipeline.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });

        // Render a red rect filling the entire canvas.
        let mut renderer = InstancedRectRenderer::new(4);
        renderer.push(InstanceRect::rect(
            [0.0, 0.0],
            [w as f32, h as f32],
            [1.0, 0.0, 0.0, 1.0],
        ));

        // Clear pass.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("instanced test encoder"),
        });
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
        }

        let draws = renderer
            .flush(
                &device,
                &queue,
                &mut encoder,
                &pipeline,
                &globals_bg,
                &view,
                None,
                w,
                h,
            )
            .expect("flush");
        assert_eq!(draws, 1, "should have issued 1 draw call");

        queue.submit(Some(encoder.finish()));

        // Readback a pixel from the centre.
        let unpadded = w * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc2 =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc2.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(enc2.finish()));
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let data = slice.get_mapped_range();
        let row = 32u32;
        let col = 32u32;
        let idx = (row * padded + col * 4) as usize;
        let r = data[idx];
        let a = data[idx + 3];
        drop(data);
        readback.unmap();
        assert!(r > 200, "centre pixel should be reddish (r={r})");
        assert!(a > 200, "centre pixel should be opaque (a={a})");
    }
}
