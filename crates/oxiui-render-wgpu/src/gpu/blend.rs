//! Blend mode pipeline variants and runtime blend-state selection.
//!
//! OxiUI's [`oxiui_core::paint::DrawCommand::SetBlendMode`] allows UI elements to be composited
//! with custom blend operations (Multiply, Screen, Overlay, etc.).  This module
//! provides:
//!
//! - [`blend_state_for_mode`] — map a [`BlendMode`] to a [`wgpu::BlendState`].
//! - [`BlendPipelineSet`] — a set of pre-compiled solid-pass pipelines, one per
//!   supported blend mode.  Switching blend modes within a frame is achieved by
//!   calling `set_pipeline` with the appropriate variant rather than recreating
//!   a pipeline at runtime.
//!
//! # wgpu blend-state mapping
//!
//! | [`BlendMode`]  | wgpu colour blend equation                           |
//! |----------------|------------------------------------------------------|
//! | `Normal`       | `Src + Dst × (1 − SrcAlpha)` (standard source-over) |
//! | `Multiply`     | `Src × Dst + Dst × 0`                              |
//! | `Screen`       | `Src + Dst − Src × Dst` (approx: `Src + Dst × (1-Src)`) |
//! | `Overlay`      | Not directly expressible in fixed-function blend;    |
//! |                | falls back to `Normal` in hardware blend mode.       |
//! | `Darken`       | `min(Src, Dst)` — not expressible; falls back.       |
//! | `Lighten`      | `max(Src, Dst)` — not expressible; falls back.       |
//! | `Copy`         | `Src × 1 + Dst × 0` (replace).                     |
//! | `Destination`  | `Src × 0 + Dst × 1` (no change).                   |
//!
//! Modes that require per-pixel arithmetic beyond what fixed-function hardware
//! supports (Overlay, Darken, Lighten) fall back to `Normal`.  A future
//! improvement could implement them via custom fragment shader variants.

use oxiui_core::paint::BlendMode;

use crate::gpu::buffer::Vertex;
use crate::gpu::device::TARGET_FORMAT;

// ── blend_state_for_mode ─────────────────────────────────────────────────────

/// Return the [`wgpu::BlendState`] that best approximates `mode`.
///
/// Modes that cannot be expressed in fixed-function blending fall back to
/// standard source-over (`Normal`).
pub fn blend_state_for_mode(mode: BlendMode) -> wgpu::BlendState {
    match mode {
        BlendMode::Normal => wgpu::BlendState::ALPHA_BLENDING,

        BlendMode::Multiply => wgpu::BlendState {
            color: wgpu::BlendComponent {
                // Dst × Src (multiply blend)
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        },

        BlendMode::Screen => wgpu::BlendState {
            color: wgpu::BlendComponent {
                // Src + Dst × (1 − Src) ≈ Screen
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        },

        // Overlay, Darken, Lighten cannot be expressed in fixed-function blend;
        // fall back to Normal (source-over).
        BlendMode::Overlay | BlendMode::Darken | BlendMode::Lighten => {
            wgpu::BlendState::ALPHA_BLENDING
        }

        BlendMode::Copy => wgpu::BlendState::REPLACE,

        BlendMode::Destination => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },

        // Non-exhaustive fallback for future variants.
        #[allow(unreachable_patterns)]
        _ => wgpu::BlendState::ALPHA_BLENDING,
    }
}

// ── BlendPipelineSet ──────────────────────────────────────────────────────────

/// A set of pre-compiled solid-pass pipeline variants, one per blend mode.
///
/// Building this set at startup avoids pipeline creation at runtime when the
/// active blend mode changes.  The set shares the `globals_layout` bind group
/// layout so a single globals bind group can be used across all modes.
pub struct BlendPipelineSet {
    /// Globals bind group layout (viewport uniform, group 0).
    pub globals_layout: wgpu::BindGroupLayout,
    /// `Normal` blend mode pipeline (source-over).
    pub normal: wgpu::RenderPipeline,
    /// `Multiply` blend mode pipeline.
    pub multiply: wgpu::RenderPipeline,
    /// `Screen` blend mode pipeline.
    pub screen: wgpu::RenderPipeline,
    /// `Copy` blend mode pipeline (replace destination).
    pub copy: wgpu::RenderPipeline,
    /// `Destination` blend mode pipeline (keep destination, ignore source).
    pub destination: wgpu::RenderPipeline,
}

impl BlendPipelineSet {
    /// Build all blend-mode pipeline variants.
    ///
    /// `sample_count` controls MSAA (1 = no MSAA).
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu blend-mode solid.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/solid.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu blend globals layout"),
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
            label: Some("oxiui-render-wgpu blend pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });

        let build = |label: &'static str, blend: wgpu::BlendState| {
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
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                        blend: Some(blend),
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
            })
        };

        let normal = build(
            "oxiui-render-wgpu blend normal",
            blend_state_for_mode(BlendMode::Normal),
        );
        let multiply = build(
            "oxiui-render-wgpu blend multiply",
            blend_state_for_mode(BlendMode::Multiply),
        );
        let screen = build(
            "oxiui-render-wgpu blend screen",
            blend_state_for_mode(BlendMode::Screen),
        );
        let copy = build(
            "oxiui-render-wgpu blend copy",
            blend_state_for_mode(BlendMode::Copy),
        );
        let destination = build(
            "oxiui-render-wgpu blend destination",
            blend_state_for_mode(BlendMode::Destination),
        );

        Self {
            globals_layout,
            normal,
            multiply,
            screen,
            copy,
            destination,
        }
    }

    /// Return a reference to the pipeline for the given [`BlendMode`].
    ///
    /// Modes without a dedicated pipeline (Overlay, Darken, Lighten) fall back
    /// to the `Normal` pipeline.
    pub fn pipeline_for(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Normal | BlendMode::Overlay | BlendMode::Darken | BlendMode::Lighten => {
                &self.normal
            }
            BlendMode::Multiply => &self.multiply,
            BlendMode::Screen => &self.screen,
            BlendMode::Copy => &self.copy,
            BlendMode::Destination => &self.destination,
            #[allow(unreachable_patterns)]
            _ => &self.normal,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_state_for_normal_is_alpha_blending() {
        let bs = blend_state_for_mode(BlendMode::Normal);
        assert_eq!(bs, wgpu::BlendState::ALPHA_BLENDING);
    }

    #[test]
    fn blend_state_for_copy_is_replace() {
        let bs = blend_state_for_mode(BlendMode::Copy);
        assert_eq!(bs, wgpu::BlendState::REPLACE);
    }

    #[test]
    fn overlay_falls_back_to_normal() {
        let overlay = blend_state_for_mode(BlendMode::Overlay);
        let normal = blend_state_for_mode(BlendMode::Normal);
        assert_eq!(overlay, normal);
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
            label: Some("blend test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    #[test]
    fn blend_pipeline_set_compiles() {
        let Some((device, _)) = try_device() else {
            return;
        };
        let set = BlendPipelineSet::new(&device, 1);
        // pipeline_for returns a reference to the right variant
        let _normal = set.pipeline_for(BlendMode::Normal);
        let _multiply = set.pipeline_for(BlendMode::Multiply);
        let _screen = set.pipeline_for(BlendMode::Screen);
        let _copy = set.pipeline_for(BlendMode::Copy);
        // Overlay falls back to Normal (same object pointer).
        let _ = set.pipeline_for(BlendMode::Overlay);
    }
}
