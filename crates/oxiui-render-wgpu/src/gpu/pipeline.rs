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

use crate::gpu::buffer::{GradientUniforms, GradientVertex, TexVertex, Vertex};
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
    ///
    /// `sample_count` sets the MSAA multisample count (1 = no MSAA, 4 or 8 =
    /// MSAA).  Passing `1` reproduces the exact original code path.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
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
                count: sample_count,
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
    ///
    /// `sample_count` sets the MSAA multisample count (1 = no MSAA, 4 or 8 =
    /// MSAA).  Passing `1` reproduces the exact original code path.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu gradient.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/gradient.wgsl").into()),
        });

        // Bind group 0:
        //   binding 0 — Globals (viewport, vertex stage only, static offset)
        //   binding 1 — GradientUniforms (fragment stage only, dynamic offset for batching)
        //
        // Binding 1 uses has_dynamic_offset: true so that a single combined
        // uniform buffer can be shared across all gradient draws in one render
        // pass — each draw issues set_bind_group with a different byte offset
        // into the same buffer (stride = align_up(288, min_uniform_buffer_offset_alignment)).
        let grad_uniform_size = core::mem::size_of::<GradientUniforms>() as u64;
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
                        has_dynamic_offset: true,
                        min_binding_size: core::num::NonZeroU64::new(grad_uniform_size),
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
                count: sample_count,
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

// ── TexturedPipeline ──────────────────────────────────────────────────────────

/// The compiled textured pipeline for rendering image quads and nine-slice
/// patches with optional tint.
///
/// Two bind group layouts:
/// - `globals_layout` (group 0): the viewport `Globals` uniform (vertex stage).
/// - `texture_layout` (group 1): a `texture_2d<f32>` (binding 0, fragment) and
///   a `sampler` (binding 1, fragment).
pub struct TexturedPipeline {
    /// The render pipeline (vertex + fragment stages, alpha blending).
    pub pipeline: wgpu::RenderPipeline,
    /// Layout of bind group 0 (the viewport `Globals` uniform).
    pub globals_layout: wgpu::BindGroupLayout,
    /// Layout of bind group 1 (texture + sampler).
    pub texture_layout: wgpu::BindGroupLayout,
}

impl TexturedPipeline {
    /// Build the textured pipeline for a colour target in [`TARGET_FORMAT`].
    ///
    /// `sample_count` sets the MSAA multisample count (1 = no MSAA, 4 or 8 =
    /// MSAA).  Passing `1` reproduces the exact original code path.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu textured.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/textured.wgsl").into()),
        });

        // Bind group 0: Globals uniform (vertex stage only)
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu textured globals layout"),
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

        // Bind group 1: texture (binding 0) + sampler (binding 1), fragment stage
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu textured texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxiui-render-wgpu textured pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        // Vertex attributes mirror `TexVertex` byte offsets exactly (32 bytes):
        //   position  vec2  @0   offset  0
        //   uv        vec2  @1   offset  8
        //   tint      vec4  @2   offset 16
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
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 2,
            },
        ];

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<TexVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("oxiui-render-wgpu textured pipeline"),
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
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            globals_layout,
            texture_layout,
        }
    }
}

// ── BlurPipeline ──────────────────────────────────────────────────────────────

/// The compiled separable Gaussian blur pipeline.
///
/// Bind group layout:
/// - `globals_layout` (group 0): viewport `Globals` uniform (vertex stage).
/// - `source_layout` (group 1): source texture (binding 0), sampler (binding 1),
///   [`crate::gpu::buffer::BlurUniforms`] (binding 2) — all fragment stage.
pub struct BlurPipeline {
    /// The render pipeline (vertex + fragment stages, alpha blending).
    pub pipeline: wgpu::RenderPipeline,
    /// Layout of bind group 0 (the viewport `Globals` uniform).
    pub globals_layout: wgpu::BindGroupLayout,
    /// Layout of bind group 1 (source texture + sampler + `BlurUniforms`).
    pub source_layout: wgpu::BindGroupLayout,
}

impl BlurPipeline {
    /// Build the blur pipeline for a colour target in [`TARGET_FORMAT`].
    ///
    /// `sample_count` sets the MSAA multisample count.  Shadow ping-pong
    /// textures are always count=1, so this should always be called with `1`.
    /// The parameter is present for consistency with the other pipelines.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu blur.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/blur.wgsl").into()),
        });

        // Bind group 0: Globals uniform (vertex stage only).
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu blur globals layout"),
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

        // Bind group 1: source texture (0) + sampler (1) + BlurUniforms (2), fragment stage.
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu blur source layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
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
            label: Some("oxiui-render-wgpu blur pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&source_layout)],
            immediate_size: 0,
        });

        // Reuse GradientVertex layout (16 bytes): position @0, local @1.
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
            label: Some("oxiui-render-wgpu blur pipeline"),
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
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            globals_layout,
            source_layout,
        }
    }
}

// ── CompositePipeline ─────────────────────────────────────────────────────────

/// The compiled shadow composite pipeline.
///
/// Bind group layout mirrors [`BlurPipeline`]: group 0 = `Globals`, group 1 =
/// mask texture + sampler + [`crate::gpu::buffer::CompUniforms`].
pub struct CompositePipeline {
    /// The render pipeline (vertex + fragment stages, alpha blending).
    pub pipeline: wgpu::RenderPipeline,
    /// Layout of bind group 0 (the viewport `Globals` uniform).
    pub globals_layout: wgpu::BindGroupLayout,
    /// Layout of bind group 1 (mask texture + sampler + `CompUniforms`).
    pub source_layout: wgpu::BindGroupLayout,
}

impl CompositePipeline {
    /// Build the composite pipeline for a colour target in [`TARGET_FORMAT`].
    ///
    /// `sample_count` sets the MSAA multisample count for the output target.
    /// The composite pass writes to the main screen target, which may have MSAA
    /// active, so this should match `GpuContext::sample_count`.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu composite.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/composite.wgsl").into()),
        });

        // Bind group 0: Globals uniform (vertex stage only).
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu composite globals layout"),
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

        // Bind group 1: mask texture (0) + sampler (1) + CompUniforms (2), fragment stage.
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu composite source layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
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
            label: Some("oxiui-render-wgpu composite pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&source_layout)],
            immediate_size: 0,
        });

        // Reuse GradientVertex layout (16 bytes): position @0, local @1.
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
            label: Some("oxiui-render-wgpu composite pipeline"),
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
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            globals_layout,
            source_layout,
        }
    }
}
