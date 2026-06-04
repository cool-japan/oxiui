//! SDF text rendering pipeline for `oxiui-render-wgpu`.
//!
//! Enabled by the `text` Cargo feature.  Uploads glyph SDFs produced by
//! [`oxitext_sdf`] to a GPU texture atlas and renders them via a dedicated
//! WGSL SDF fragment shader that computes coverage from the stored distance
//! field, yielding sharp text at any scale without per-pixel rasterization.
//!
//! # Architecture
//!
//! 1. **Build phase** — [`SdfTextPipeline::new`] creates the wgpu bind-group
//!    layout, render pipeline, and an empty GPU texture atlas (R8Unorm, size
//!    `atlas_size × atlas_size`).
//! 2. **Atlas upload** — [`SdfTextPipeline::upload_glyph`] writes an
//!    [`oxitext_sdf::SdfTile`] into the atlas texture at the position returned
//!    by the shelf packer.  The UV rect and glyph metrics are stored in an
//!    in-memory map keyed by glyph ID.
//! 3. **Draw phase** — [`SdfTextPipeline::draw_text`] walks shaped glyph
//!    positions, looks up each glyph in the atlas map, emits one quad
//!    (two triangles) per visible glyph into a vertex buffer, and issues a
//!    single `draw_indexed` call via the SDF render pipeline.
//!
//! # WGSL shader
//!
//! The SDF fragment shader converts the atlas sample `d` (0 = outside,
//! 255 = fully inside after uploading u8 values) to a coverage alpha using
//! the standard SDF soft-edge formula:
//!
//! ```wgsl
//! let d = textureSample(t_sdf, s_sdf, uv).r;
//! let alpha = smoothstep(0.5 - EDGE_SOFTNESS, 0.5 + EDGE_SOFTNESS, d) * color.a;
//! ```
//!
//! Requires the `text` Cargo feature.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "text", not(test)))] {
//! use oxiui_render_wgpu::sdf_text::{SdfTextPipeline, SdfTextConfig};
//! use oxitext_sdf::{SdfAtlas, SdfTile};
//! use wgpu::{Device, Queue, TextureFormat};
//!
//! // device / queue / format obtained from wgpu device initialization.
//! # fn doc(device: &Device, queue: &Queue, format: TextureFormat) {
//! let config = SdfTextConfig { atlas_size: 1024, edge_softness: 0.06 };
//! let mut pipeline = SdfTextPipeline::new(device, format, config);
//!
//! // Upload tiles from a pre-generated SdfAtlas.
//! let tile = SdfTile {
//!     glyph_id: 65, // 'A'
//!     width: 32, height: 32,
//!     data: vec![128u8; 32 * 32],
//!     bearing_x: 0, bearing_y: 28,
//!     advance_x: 34.0,
//! };
//! pipeline.upload_glyph(device, queue, &tile);
//! # }
//! # }
//! ```

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use oxitext_sdf::SdfTile;
use oxiui_core::UiError;
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendComponent,
    BlendFactor, BlendOperation, BlendState, ColorTargetState, ColorWrites, Device, Extent3d,
    FilterMode, FragmentState, FrontFace, IndexFormat, MipmapFilterMode, MultisampleState,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, Queue, RenderPass,
    RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, TexelCopyBufferLayout,
    TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
    VertexStepMode,
};

// ── WGSL SDF Shader ──────────────────────────────────────────────────────────

/// The WGSL SDF text shader source.
///
/// Vertex stage: transforms a per-quad vertex (position in clip space,
/// UV coordinates into the atlas, per-quad text colour) through the
/// shader without any additional transform matrix (the host pre-computes
/// clip-space positions).
///
/// Fragment stage: samples the R8Unorm SDF atlas and applies the standard
/// smoothstep coverage formula to produce per-fragment alpha.
const SDF_TEXT_SHADER_WGSL: &str = r#"
// ─── Uniforms ────────────────────────────────────────────────────────────────

struct SdfUniforms {
    // Softness of the SDF edge (typical range: 0.03 – 0.10).
    edge_softness: f32,
    // Padding to meet minimum 16-byte alignment.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> sdf_uniforms: SdfUniforms;
@group(0) @binding(1) var t_sdf: texture_2d<f32>;
@group(0) @binding(2) var s_sdf: sampler;

// ─── Vertex ───────────────────────────────────────────────────────────────────

struct VertexIn {
    @location(0) position: vec2<f32>,  // clip-space X, Y (already [-1, 1])
    @location(1) uv: vec2<f32>,        // atlas UV in [0, 1]
    @location(2) color: vec4<f32>,     // premultiplied RGBA
};

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

// ─── Fragment ─────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Sample the SDF atlas — R channel holds the distance in [0, 1].
    // Value 0.5 is the outline edge; values > 0.5 are inside the glyph.
    let d = textureSample(t_sdf, s_sdf, in.uv).r;

    let half = 0.5;
    let softness = sdf_uniforms.edge_softness;
    // Coverage: smooth transition from fully-transparent to fully-opaque.
    let coverage = smoothstep(half - softness, half + softness, d);

    // Premultiply: output = color * coverage (alpha already baked in color.a).
    let alpha = coverage * in.color.a;
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
"#;

// ── Vertex layout ─────────────────────────────────────────────────────────────

/// A single vertex for an SDF text quad.
///
/// Layout must match `VertexIn` in the WGSL shader above.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct SdfVertex {
    /// Clip-space position `(x, y)`.  The host computes clip-space coordinates
    /// directly to avoid a GPU matrix multiply.
    pub position: [f32; 2],
    /// Atlas UV coordinates in `[0, 1]`.
    pub uv: [f32; 2],
    /// Premultiplied RGBA color `[r, g, b, a]` in `[0, 1]`.
    pub color: [f32; 4],
}

// ── SdfTextConfig ─────────────────────────────────────────────────────────────

/// Configuration parameters for [`SdfTextPipeline`].
#[derive(Clone, Debug)]
pub struct SdfTextConfig {
    /// Atlas texture size in pixels (square).
    ///
    /// A larger atlas reduces the number of atlas rebuilds but uses more GPU
    /// memory.  Recommended starting value: 1024 or 2048.
    pub atlas_size: u32,

    /// SDF edge smoothness factor passed to the fragment shader's `smoothstep`
    /// call.  Typical range: `0.03` (sharp, good at large sizes) to `0.10`
    /// (soft, good at small sizes).
    pub edge_softness: f32,
}

impl Default for SdfTextConfig {
    fn default() -> Self {
        SdfTextConfig {
            atlas_size: 1024,
            edge_softness: 0.06,
        }
    }
}

// ── Atlas shelf packer ────────────────────────────────────────────────────────

/// Minimal CPU-side shelf packer for the SDF atlas.
///
/// Allocates rectangular regions in an `atlas_size × atlas_size` grid using
/// a shelf algorithm: tiles are placed left-to-right within a shelf; when a
/// new tile doesn't fit horizontally a new shelf is started.
struct ShelfPacker {
    atlas_size: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_height: u32,
}

impl ShelfPacker {
    fn new(atlas_size: u32) -> Self {
        ShelfPacker {
            atlas_size,
            cursor_x: 0,
            cursor_y: 0,
            shelf_height: 0,
        }
    }

    /// Allocate a region of size `(w, h)`.
    ///
    /// Returns `(x, y)` of the top-left corner, or `None` if the atlas is full.
    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.atlas_size || h > self.atlas_size {
            return None;
        }
        // Try to fit on the current shelf.
        if self.cursor_x + w <= self.atlas_size {
            let x = self.cursor_x;
            let y = self.cursor_y;
            self.cursor_x += w;
            self.shelf_height = self.shelf_height.max(h);
            return Some((x, y));
        }
        // Start a new shelf.
        let new_y = self.cursor_y + self.shelf_height;
        if new_y + h > self.atlas_size {
            return None; // atlas is full
        }
        let x = 0;
        self.cursor_x = w;
        self.cursor_y = new_y;
        self.shelf_height = h;
        Some((x, new_y))
    }
}

// ── Per-glyph atlas entry ─────────────────────────────────────────────────────

/// Metadata for a glyph that has been uploaded to the GPU atlas.
#[derive(Clone, Debug)]
pub struct AtlasEntry {
    /// UV coordinates in the atlas texture (normalized [0, 1]).
    pub uv_min: [f32; 2],
    /// UV coordinates in the atlas texture (normalized [0, 1]).
    pub uv_max: [f32; 2],
    /// Glyph bearing X in pixels (pen → left edge).
    pub bearing_x: i32,
    /// Glyph bearing Y in pixels (baseline → top edge).
    pub bearing_y: i32,
    /// Horizontal advance in pixels.
    pub advance_x: f32,
    /// Glyph width in pixels.
    pub width_px: u32,
    /// Glyph height in pixels.
    pub height_px: u32,
}

// ── SdfTextPipeline ───────────────────────────────────────────────────────────

/// GPU SDF text rendering pipeline.
///
/// Owns a wgpu render pipeline, a GPU texture atlas, a bind group, and an
/// in-memory map from glyph ID to [`AtlasEntry`].  Use [`upload_glyph`] to
/// populate the atlas and [`draw_text`] to emit quads during a render pass.
///
/// Requires the `text` Cargo feature.
///
/// [`upload_glyph`]: SdfTextPipeline::upload_glyph
/// [`draw_text`]: SdfTextPipeline::draw_text
pub struct SdfTextPipeline {
    /// The wgpu render pipeline (SDF vertex + fragment shader).
    pipeline: RenderPipeline,
    /// The bind group (uniforms + SDF atlas texture + sampler).
    bind_group: BindGroup,
    /// The GPU atlas texture (R8Unorm, `atlas_size × atlas_size`).
    atlas_texture: Texture,
    /// Atlas texture view (bound in the bind group; kept for future bind-group rebuilds).
    _atlas_view: TextureView,
    /// Uniform buffer holding [`SdfUniforms`] (edge_softness + padding; kept for bind group).
    _uniform_buffer: wgpu::Buffer,
    /// CPU-side shelf packer for tracking used atlas regions.
    packer: ShelfPacker,
    /// Map from glyph ID → atlas entry (UV rect + metrics).
    entries: HashMap<u16, AtlasEntry>,
    /// Atlas size in pixels (square).
    atlas_size: u32,
}

impl SdfTextPipeline {
    /// Create a new [`SdfTextPipeline`] on the given device with the target
    /// surface format and configuration.
    ///
    /// This allocates the GPU atlas texture (R8Unorm), the SDF shader module,
    /// the render pipeline, and the bind group.  No glyphs are uploaded yet.
    pub fn new(device: &Device, surface_format: TextureFormat, config: SdfTextConfig) -> Self {
        // ── Shader ─────────────────────────────────────────────────────────────
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("sdf_text_shader"),
            source: ShaderSource::Wgsl(SDF_TEXT_SHADER_WGSL.into()),
        });

        // ── Atlas texture ──────────────────────────────────────────────────────
        let atlas_texture = device.create_texture(&TextureDescriptor {
            label: Some("sdf_atlas"),
            size: Extent3d {
                width: config.atlas_size,
                height: config.atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&TextureViewDescriptor::default());

        // ── Sampler ────────────────────────────────────────────────────────────
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("sdf_text_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // ── Uniform buffer ─────────────────────────────────────────────────────
        // SdfUniforms: 4 f32 = 16 bytes.
        let uniform_data: [f32; 4] = [config.edge_softness, 0.0, 0.0, 0.0];
        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("sdf_uniforms"),
            contents: bytemuck::cast_slice(&uniform_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group layout ──────────────────────────────────────────────────
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("sdf_text_bgl"),
            entries: &[
                // Binding 0: SdfUniforms
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Binding 1: atlas texture
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 2: sampler
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Bind group ─────────────────────────────────────────────────────────
        let bind_group = Self::build_bind_group(
            device,
            &bind_group_layout,
            &uniform_buffer,
            &atlas_view,
            &sampler,
        );

        // ── Pipeline layout ────────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("sdf_text_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // ── Vertex buffer layout ───────────────────────────────────────────────
        let vertex_buffer_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<SdfVertex>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32>  → location 0
                VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                // uv: vec2<f32>  → location 1
                VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: std::mem::offset_of!(SdfVertex, uv) as u64,
                    shader_location: 1,
                },
                // color: vec4<f32>  → location 2
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: std::mem::offset_of!(SdfVertex, color) as u64,
                    shader_location: 2,
                },
            ],
        };

        // ── Render pipeline ────────────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("sdf_text_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_buffer_layout],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                front_face: FrontFace::Ccw,
                polygon_mode: PolygonMode::Fill,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: surface_format,
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::One,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::One,
                            dst_factor: BlendFactor::OneMinusSrcAlpha,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        SdfTextPipeline {
            pipeline,
            bind_group,
            atlas_texture,
            _atlas_view: atlas_view,
            _uniform_buffer: uniform_buffer,
            packer: ShelfPacker::new(config.atlas_size),
            entries: HashMap::new(),
            atlas_size: config.atlas_size,
        }
    }

    // ── Atlas upload ──────────────────────────────────────────────────────────

    /// Upload an [`SdfTile`] from [`oxitext_sdf`] to the GPU atlas.
    ///
    /// The tile is placed by the shelf packer and its UV rect + metrics
    /// are stored in the internal glyph map.  Subsequent calls to
    /// [`draw_text`] will use this entry.
    ///
    /// Returns `Err(UiError::Render)` if:
    /// - The atlas is full and cannot fit the tile.
    /// - The tile dimensions are zero.
    ///
    /// Uploading the same `glyph_id` again overwrites the existing entry.
    ///
    /// [`draw_text`]: SdfTextPipeline::draw_text
    pub fn upload_glyph(
        &mut self,
        _device: &Device,
        queue: &Queue,
        tile: &SdfTile,
    ) -> Result<(), UiError> {
        if tile.width == 0 || tile.height == 0 {
            return Err(UiError::Render("upload_glyph: zero-size tile".to_string()));
        }

        let (ax, ay) = self
            .packer
            .allocate(tile.width, tile.height)
            .ok_or_else(|| {
                UiError::Render(format!(
                    "SDF atlas full — cannot fit glyph {} ({}×{})",
                    tile.glyph_id, tile.width, tile.height
                ))
            })?;

        // Write glyph pixels to the atlas texture.
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: ax, y: ay, z: 0 },
                aspect: TextureAspect::All,
            },
            &tile.data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tile.width),
                rows_per_image: Some(tile.height),
            },
            Extent3d {
                width: tile.width,
                height: tile.height,
                depth_or_array_layers: 1,
            },
        );

        let inv = self.atlas_size as f32;
        let entry = AtlasEntry {
            uv_min: [ax as f32 / inv, ay as f32 / inv],
            uv_max: [
                (ax + tile.width) as f32 / inv,
                (ay + tile.height) as f32 / inv,
            ],
            bearing_x: tile.bearing_x,
            bearing_y: tile.bearing_y,
            advance_x: tile.advance_x,
            width_px: tile.width,
            height_px: tile.height,
        };
        self.entries.insert(tile.glyph_id, entry);
        Ok(())
    }

    // ── Draw ──────────────────────────────────────────────────────────────────

    /// Emit SDF text quads to `render_pass`.
    ///
    /// `glyphs` is a slice of `(glyph_id, pen_x_clip, pen_y_clip)` tuples
    /// where `pen_x_clip` and `pen_y_clip` are in clip-space coordinates
    /// (`[-1, 1]`). `color` is a premultiplied RGBA tuple in `[0, 1]`.
    /// `viewport_w` and `viewport_h` are the viewport dimensions in pixels
    /// (used to convert the glyph pixel metrics to clip-space extents).
    ///
    /// Glyphs whose IDs are not present in the atlas are silently skipped.
    ///
    /// Returns `Err(UiError::Render)` if the vertex or index buffer cannot be
    /// created.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &self,
        device: &Device,
        render_pass: &mut RenderPass<'_>,
        glyphs: &[(u16, f32, f32)],
        color: [f32; 4],
        viewport_w: f32,
        viewport_h: f32,
    ) -> Result<(), UiError> {
        if glyphs.is_empty() {
            return Ok(());
        }

        let mut vertices: Vec<SdfVertex> = Vec::with_capacity(glyphs.len() * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(glyphs.len() * 6);
        let mut quad_count: u32 = 0;

        for &(glyph_id, pen_x, pen_y) in glyphs {
            let entry = match self.entries.get(&glyph_id) {
                Some(e) => e,
                None => continue, // skip glyphs not in atlas
            };

            // Convert pixel bearing/size to clip-space offsets.
            let w_clip = entry.width_px as f32 / viewport_w * 2.0;
            let h_clip = entry.height_px as f32 / viewport_h * 2.0;
            let bx_clip = entry.bearing_x as f32 / viewport_w * 2.0;
            let by_clip = entry.bearing_y as f32 / viewport_h * 2.0;

            // Quad corners (clip-space) — Y is inverted (clip-space Y+ is up).
            let x0 = pen_x + bx_clip;
            let y0 = pen_y - by_clip; // top (baseline minus ascent)
            let x1 = x0 + w_clip;
            let y1 = y0 - h_clip; // bottom

            let [u0, v0] = entry.uv_min;
            let [u1, v1] = entry.uv_max;

            let base = quad_count * 4;
            // Top-left, top-right, bottom-right, bottom-left.
            vertices.extend_from_slice(&[
                SdfVertex {
                    position: [x0, y0],
                    uv: [u0, v0],
                    color,
                },
                SdfVertex {
                    position: [x1, y0],
                    uv: [u1, v0],
                    color,
                },
                SdfVertex {
                    position: [x1, y1],
                    uv: [u1, v1],
                    color,
                },
                SdfVertex {
                    position: [x0, y1],
                    uv: [u0, v1],
                    color,
                },
            ]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            quad_count += 1;
        }

        if quad_count == 0 {
            return Ok(()); // nothing visible
        }

        let vbuf = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("sdf_text_vbuf"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("sdf_text_ibuf"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, vbuf.slice(..));
        render_pass.set_index_buffer(ibuf.slice(..), IndexFormat::Uint32);
        render_pass.draw_indexed(0..indices.len() as u32, 0, 0..1);

        Ok(())
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Return the number of glyphs currently stored in the atlas.
    pub fn glyph_count(&self) -> usize {
        self.entries.len()
    }

    /// Look up a glyph's atlas entry by ID.
    pub fn entry(&self, glyph_id: u16) -> Option<&AtlasEntry> {
        self.entries.get(&glyph_id)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn build_bind_group(
        device: &Device,
        layout: &BindGroupLayout,
        uniform_buffer: &wgpu::Buffer,
        atlas_view: &TextureView,
        sampler: &Sampler,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("sdf_text_bg"),
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(atlas_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_packer_simple_allocation() {
        let mut packer = ShelfPacker::new(512);
        let r1 = packer.allocate(32, 32);
        assert_eq!(r1, Some((0, 0)));
        let r2 = packer.allocate(32, 32);
        assert_eq!(r2, Some((32, 0)));
    }

    #[test]
    fn shelf_packer_new_shelf_when_row_full() {
        let mut packer = ShelfPacker::new(64);
        let _ = packer.allocate(32, 32); // fits (0, 0)
        let _ = packer.allocate(32, 32); // fits (32, 0) — row full
        let r3 = packer.allocate(32, 16); // should start new shelf at y=32
        assert_eq!(r3, Some((0, 32)));
    }

    #[test]
    fn shelf_packer_atlas_full_returns_none() {
        let mut packer = ShelfPacker::new(64);
        // Fill the atlas completely.
        let _ = packer.allocate(64, 64); // takes entire atlas
        let r = packer.allocate(1, 1);
        assert!(r.is_none());
    }

    #[test]
    fn shelf_packer_oversized_tile_returns_none() {
        let mut packer = ShelfPacker::new(64);
        assert!(packer.allocate(128, 32).is_none());
        assert!(packer.allocate(32, 128).is_none());
    }

    #[test]
    fn sdf_text_config_default() {
        let cfg = SdfTextConfig::default();
        assert_eq!(cfg.atlas_size, 1024);
        assert!((cfg.edge_softness - 0.06).abs() < 1e-6);
    }

    #[test]
    fn atlas_entry_fields() {
        let entry = AtlasEntry {
            uv_min: [0.0, 0.0],
            uv_max: [0.5, 0.5],
            bearing_x: -1,
            bearing_y: 12,
            advance_x: 14.0,
            width_px: 16,
            height_px: 16,
        };
        assert_eq!(entry.width_px, 16);
        assert_eq!(entry.advance_x, 14.0);
    }

    #[test]
    fn sdf_vertex_is_pod() {
        // bytemuck::Pod requires the type to be plain-old-data; just check it
        // compiles + can be cast.
        let v = SdfVertex {
            position: [0.0, 0.0],
            uv: [0.5, 0.5],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let _bytes: &[u8] = bytemuck::bytes_of(&v);
    }
}
