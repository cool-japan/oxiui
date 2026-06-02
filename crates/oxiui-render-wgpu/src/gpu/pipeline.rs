//! Render-pipeline construction for the solid-fill / SDF shader and the
//! gradient pipeline.
//!
//! [`SolidPipeline`] owns the compiled `solid.wgsl` module, the uniform bind
//! group layout (viewport [`Globals`]), and the [`wgpu::RenderPipeline`].
//!
//! [`GradientPipeline`] owns the compiled `gradient.wgsl` module, a bind group
//! layout for the viewport uniform plus a per-draw gradient uniform buffer, and
//! the gradient render pipeline.
//!
//! Vertex attribute layouts are derived by hand to match the field offsets of
//! [`Vertex`] / [`GradientVertex`] exactly — the compile-time size assertions
//! in [`crate::gpu::buffer`] guard against drift.
//!
//! Solid pipeline vertex layout (56 bytes = 14 × f32):
//!   pos      vec2  @0   offset  0
//!   color    vec4  @1   offset  8
//!   local    vec2  @2   offset 24
//!   shape_xy vec2  @3   offset 32
//!   shape_r  f32   @4   offset 40
//!   kind     f32   @5   offset 44
//!   extra    vec2  @6   offset 48
//!
//! [`Globals`]: crate::gpu::buffer::Globals
//! [`Vertex`]: crate::gpu::buffer::Vertex
//! [`GradientVertex`]: crate::gpu::buffer::GradientVertex

use crate::gpu::buffer::{GradientVertex, Vertex};
use crate::gpu::device::TARGET_FORMAT;

// ── SolidPipeline ────────────────────────────────────────────────────────────

/// The compiled solid-fill / SDF pipeline plus the bind-group layout its draws
/// need.
pub struct SolidPipeline {
    /// The render pipeline (vertex + fragment stages, alpha blending).
    pub pipeline: wgpu::RenderPipeline,
    /// Layout of bind group 0 (the viewport uniform).
    pub globals_layout: wgpu::BindGroupLayout,
}

impl SolidPipeline {
    /// Build the solid pipeline for a colour target in [`TARGET_FORMAT`].
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu solid.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/solid.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu globals layout"),
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
            label: Some("oxiui-render-wgpu solid pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });

        // Vertex attributes mirror `Vertex` byte offsets exactly (56 bytes):
        //   pos      vec2  @0   offset  0
        //   color    vec4  @1   offset  8
        //   local    vec2  @2   offset 24
        //   shape_xy vec2  @3   offset 32
        //   shape_r  f32   @4   offset 40
        //   kind     f32   @5   offset 44
        //   extra    vec2  @6   offset 48
        let attrs = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 40,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 44,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 48,
                shader_location: 6,
            },
        ];

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("oxiui-render-wgpu solid pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
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
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            globals_layout,
        }
    }
}

// ── GradientPipeline ─────────────────────────────────────────────────────────

/// The compiled gradient pipeline: gradient quads rendered via a per-draw
/// uniform buffer carrying gradient type, stops, and geometry.
pub struct GradientPipeline {
    /// The render pipeline (vertex + fragment stages, alpha blending).
    pub pipeline: wgpu::RenderPipeline,
    /// Layout of bind group 0: viewport uniform (binding 0) and gradient
    /// uniform buffer (binding 1).
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl GradientPipeline {
    /// Build the gradient pipeline for a colour target in [`TARGET_FORMAT`].
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu gradient.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/gradient.wgsl").into()),
        });

        // Bind group 0:
        //   binding 0 — Globals (viewport, vertex stage only)
        //   binding 1 — GradientUniforms (fragment stage only)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu gradient bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxiui-render-wgpu gradient pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // Gradient vertex layout (16 bytes):
        //   position  vec2  @0   offset 0
        //   local     vec2  @1   offset 8
        let attrs = [
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
        ];

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<GradientVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("oxiui-render-wgpu gradient pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
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
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }
}
