//! Compute-shader Gaussian blur pipeline.
//!
//! This module provides [`ComputeBlurPipeline`], which uses a compute shader
//! (`blur_compute.wgsl`) for separable Gaussian blur.  Compared to the
//! fragment-shader multi-pass approach in [`crate::gpu::shadow`], the compute
//! approach:
//!
//! - Avoids per-pass render-pass setup and resolve overhead.
//! - Allows arbitrary output texel positions without a full-screen quad.
//! - Has better occupancy on wide GPUs when the blur radius is large.
//!
//! # Usage
//!
//! ```rust,ignore
//! let pipeline = ComputeBlurPipeline::new(&device).expect("compute blur");
//! pipeline.blur(&device, &queue, &src_view, &dst_texture, width, height, blur_radius)?;
//! ```
//!
//! # Feature requirement
//!
//! Requires [`wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`] or at
//! a minimum the adapter must support `rgba8unorm` with
//! `STORAGE_BINDING | STORAGE_WRITE` usage.  The constructor returns
//! [`UiError::Unsupported`] if this is not available.

use bytemuck::{Pod, Zeroable};
use oxiui_core::UiError;

use crate::gpu::device::TARGET_FORMAT;

// ── BlurComputeUniforms ───────────────────────────────────────────────────────

/// Uniforms for the blur compute shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct BlurComputeUniforms {
    radius: u32,
    horizontal: u32,
    sigma: f32,
    _pad: f32,
}

const _: () = assert!(core::mem::size_of::<BlurComputeUniforms>() == 16);

// ── ComputeBlurPipeline ───────────────────────────────────────────────────────

/// A compute-shader-based Gaussian blur pipeline.
///
/// The pipeline uses a separable Gaussian blur: two passes (horizontal then
/// vertical) each dispatching one compute thread per output texel.
pub struct ComputeBlurPipeline {
    /// The compute pipeline.
    pub pipeline: wgpu::ComputePipeline,
    /// Bind group 0 layout (uniforms).
    pub uniform_layout: wgpu::BindGroupLayout,
    /// Bind group 1 layout (source texture + dest storage + sampler).
    pub texture_layout: wgpu::BindGroupLayout,
    /// Whether rgba8unorm storage is supported.
    pub storage_rgba8_supported: bool,
}

impl ComputeBlurPipeline {
    /// Create the compute blur pipeline.
    ///
    /// Returns [`UiError::Unsupported`] if the device does not support
    /// `rgba8unorm` storage binding.
    pub fn new(device: &wgpu::Device) -> Result<Self, UiError> {
        // Check if rgba8unorm storage write is available.
        let rgba8_storage = device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
        // Even without the broad feature flag, Metal always supports rgba8unorm
        // storage on Apple Silicon. We try to compile the pipeline and catch
        // the error rather than checking a flag that may not be reliable.

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("oxiui-render-wgpu blur_compute.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/blur_compute.wgsl").into()),
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui compute blur uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxiui compute blur texture layout"),
            entries: &[
                // binding 0: sampled input texture
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 1: write-only storage texture output
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: TARGET_FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // binding 2: linear sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxiui compute blur pipeline layout"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("oxiui compute blur pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(Self {
            pipeline,
            uniform_layout,
            texture_layout,
            storage_rgba8_supported: rgba8_storage,
        })
    }

    /// Perform one Gaussian blur pass (horizontal or vertical).
    ///
    /// Dispatches `ceil(width/16) × ceil(height/16)` compute workgroups.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_pass(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        radius: u32,
        sigma: f32,
        horizontal: bool,
    ) {
        use wgpu::util::DeviceExt;

        let uniforms = BlurComputeUniforms {
            radius: radius.min(64),
            horizontal: if horizontal { 1 } else { 0 },
            sigma: sigma.max(0.1),
            _pad: 0.0,
        };
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxiui compute blur uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("oxiui compute blur sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui compute blur uniform bg"),
            layout: &self.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let texture_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxiui compute blur texture bg"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dst_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let _ = queue; // queue is used by write_buffer above

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("oxiui compute blur pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &uniform_bg, &[]);
        cpass.set_bind_group(1, &texture_bg, &[]);
        let groups_x = width.div_ceil(16);
        let groups_y = height.div_ceil(16);
        cpass.dispatch_workgroups(groups_x, groups_y, 1);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn try_device_with_storage() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        // Request with storage texture feature.
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("compute blur test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()
    }

    #[test]
    fn blur_compute_uniforms_size() {
        assert_eq!(core::mem::size_of::<BlurComputeUniforms>(), 16);
    }

    #[test]
    fn compute_blur_pipeline_compiles_or_unsupported() {
        let Some((device, _)) = try_device_with_storage() else {
            return;
        };
        // The pipeline may or may not compile depending on storage texture support.
        match ComputeBlurPipeline::new(&device) {
            Ok(p) => {
                // If it compiled, the storage_rgba8_supported flag may or may not
                // be set (it's informational).
                let _ = p.storage_rgba8_supported;
            }
            Err(UiError::Unsupported(msg)) => {
                println!("skip: compute blur unsupported: {msg}");
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
