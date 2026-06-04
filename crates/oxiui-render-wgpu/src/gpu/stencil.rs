//! Stencil-buffer clipping for non-rectangular clip paths.
//!
//! Rectangular scissors are already handled by the hardware `set_scissor_rect`
//! API (see `exec.rs` and `geometry.rs`).  This module provides stencil-based
//! clipping for **non-rectangular** shapes: rounded-rectangle clip masks,
//! arbitrary path clip masks, and ellipses.
//!
//! # Strategy
//!
//! 1. Create a **depth + stencil** texture (`Depth24PlusStencil8`) alongside the
//!    colour target.
//! 2. A *stencil-fill pass* renders the clip geometry into the stencil buffer
//!    (colour writes disabled) with `StencilOperation::Replace`.  The reference
//!    value is 1.
//! 3. The *content pass* renders normally but with a stencil test: only pixels
//!    where the stencil value equals 1 pass.
//! 4. A *stencil-clear pass* resets the stencil to 0 when the clip is popped.
//!
//! # Limitations
//!
//! - Nested non-rectangular clips are supported by incrementing the stencil
//!   reference value up to 255 (hardware limit).
//! - Interaction with MSAA: the depth/stencil texture must use the same
//!   `sample_count` as the colour target.

use oxiui_core::UiError;

use crate::gpu::device::TARGET_FORMAT;

/// The depth+stencil format used for stencil-based clipping.
pub const DEPTH_STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

// ── StencilTarget ─────────────────────────────────────────────────────────────

/// Owns the depth+stencil texture and view for one render target.
///
/// Create one `StencilTarget` per `GpuContext` / `RenderTarget`; it must
/// have the same `width`, `height`, and `sample_count`.
pub struct StencilTarget {
    /// The depth+stencil texture.
    pub texture: wgpu::Texture,
    /// View over the depth+stencil texture.
    pub view: wgpu::TextureView,
    /// Target width (must match the colour texture).
    pub width: u32,
    /// Target height (must match the colour texture).
    pub height: u32,
    /// MSAA sample count (must match the colour pipeline).
    pub sample_count: u32,
}

impl StencilTarget {
    /// Create a depth+stencil target for a colour target of `width × height`
    /// pixels with the given MSAA `sample_count`.
    ///
    /// # Errors
    ///
    /// Returns [`UiError::Unsupported`] if `width` or `height` is zero.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Result<Self, UiError> {
        if width == 0 || height == 0 {
            return Err(UiError::Unsupported(
                "StencilTarget dimensions must be non-zero".to_string(),
            ));
        }
        let sc = sample_count.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxiui-render-wgpu stencil target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: sc,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_STENCIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            texture,
            view,
            width,
            height,
            sample_count: sc,
        })
    }

    /// Resize the stencil target.
    ///
    /// Recreates the GPU texture; existing views become invalid.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        new_width: u32,
        new_height: u32,
    ) -> Result<(), UiError> {
        *self = Self::new(device, new_width, new_height, self.sample_count)?;
        Ok(())
    }
}

// ── StencilPipeline ───────────────────────────────────────────────────────────

/// A render pipeline variant that writes to the stencil buffer.
///
/// The pipeline outputs *no* colour (colour write mask = NONE) and uses
/// `StencilOperation::Replace` to write the reference value on every fragment
/// that passes the stencil test.
///
/// Two variants are available:
/// - `write` — writes reference value on stencil pass (populates clip mask).
/// - `clear` — writes 0 on every fragment (clears clip mask).
pub struct StencilWritePipeline {
    /// Pipeline that writes the reference value into the stencil buffer.
    pub write: wgpu::RenderPipeline,
    /// Pipeline that clears (zeroes) the stencil buffer by rendering a fullscreen quad.
    pub clear: wgpu::RenderPipeline,
    /// Globals bind group layout (viewport uniform).
    pub globals_layout: wgpu::BindGroupLayout,
}

impl StencilWritePipeline {
    /// Build the stencil-write pipeline.
    ///
    /// `sample_count` must match the colour target and `StencilTarget`.
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        // Reuse the solid shader — we only need the vertex stage.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu stencil solid shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/solid.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui-render-wgpu stencil globals layout"),
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
            label: Some("oxiui-render-wgpu stencil pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });

        // Stencil state for the write pipeline: always replace with reference.
        let write_stencil_face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };

        // Vertex attribute layout mirrors Vertex (56 bytes).
        let attrs = vertex_attrs_56();
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: 56,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        };

        let write = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("oxiui-render-wgpu stencil write pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: std::slice::from_ref(&vertex_layout),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(), // no colour output
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_STENCIL_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState {
                    front: write_stencil_face,
                    back: write_stencil_face,
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // Clear pipeline: zero out stencil.
        let clear_stencil_face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Zero,
            depth_fail_op: wgpu::StencilOperation::Zero,
            pass_op: wgpu::StencilOperation::Zero,
        };

        let clear = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("oxiui-render-wgpu stencil clear pipeline"),
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
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_STENCIL_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState {
                    front: clear_stencil_face,
                    back: clear_stencil_face,
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            write,
            clear,
            globals_layout,
        }
    }
}

/// Build the 56-byte vertex attribute array for the solid shader.
fn vertex_attrs_56() -> [wgpu::VertexAttribute; 7] {
    [
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
    ]
}

// ── StencilClipState ──────────────────────────────────────────────────────────

/// Tracks the current stencil clip depth.
///
/// Each `push_stencil_clip` increments the reference value (max 254);
/// each `pop_stencil_clip` decrements it.
#[derive(Clone, Debug, Default)]
pub struct StencilClipState {
    depth: u8,
}

impl StencilClipState {
    /// Current stencil reference value.  Draw passes should use this as the
    /// stencil compare reference.
    pub fn reference(&self) -> u32 {
        u32::from(self.depth)
    }

    /// Increment the clip depth (called when pushing a non-rectangular clip).
    /// Returns the new reference value, or `None` if the depth is already at
    /// the maximum (254).
    pub fn push(&mut self) -> Option<u32> {
        if self.depth >= 254 {
            return None;
        }
        self.depth += 1;
        Some(u32::from(self.depth))
    }

    /// Decrement the clip depth (called when popping a non-rectangular clip).
    pub fn pop(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Reset the clip depth to zero.
    pub fn reset(&mut self) {
        self.depth = 0;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use oxiui_core::UiError;

    fn try_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("stencil test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    #[test]
    fn stencil_clip_state_push_pop() {
        let mut s = StencilClipState::default();
        assert_eq!(s.reference(), 0);
        let r = s.push().expect("first push");
        assert_eq!(r, 1);
        assert_eq!(s.reference(), 1);
        s.pop();
        assert_eq!(s.reference(), 0);
    }

    #[test]
    fn stencil_clip_state_max_depth() {
        let mut s = StencilClipState::default();
        for _ in 0..254 {
            assert!(s.push().is_some());
        }
        assert_eq!(s.reference(), 254);
        assert!(s.push().is_none(), "should not exceed 254");
    }

    #[test]
    fn stencil_clip_state_pop_at_zero_is_safe() {
        let mut s = StencilClipState::default();
        s.pop(); // should not panic
        assert_eq!(s.reference(), 0);
    }

    #[test]
    fn stencil_target_zero_dimensions_fail() {
        let Some((device, _)) = try_device() else {
            return;
        };
        assert!(matches!(
            StencilTarget::new(&device, 0, 64, 1),
            Err(UiError::Unsupported(_))
        ));
    }

    #[test]
    fn stencil_target_creates_ok() {
        let Some((device, _)) = try_device() else {
            return;
        };
        let st = StencilTarget::new(&device, 64, 64, 1).expect("create stencil target");
        assert_eq!(st.width, 64);
        assert_eq!(st.height, 64);
        assert_eq!(st.sample_count, 1);
    }

    #[test]
    fn stencil_write_pipeline_compiles() {
        let Some((device, _)) = try_device() else {
            return;
        };
        // Building the pipeline validates the shader; no panic = success.
        let _pipeline = StencilWritePipeline::new(&device, 1);
    }

    #[test]
    fn stencil_format_is_depth24plus_stencil8() {
        assert_eq!(
            DEPTH_STENCIL_FORMAT,
            wgpu::TextureFormat::Depth24PlusStencil8
        );
    }
}
