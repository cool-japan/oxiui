//! High-level compute dispatch helpers built on [`ComputeContext`].
//!
//! [`Dispatcher`] wraps a reference to a [`ComputeContext`] and exposes
//! ergonomic, zero-boilerplate GPU operations for common patterns:
//!
//! | Method | Description |
//! |--------|-------------|
//! | [`Dispatcher::map_f32`] | Element-wise transform via a WGSL expression |
//! | [`Dispatcher::zip_map_f32`] | Binary element-wise transform via a WGSL expression |
//! | [`Dispatcher::reduce_sum_f32`] | Sum all elements |
//! | [`Dispatcher::sph_density`] | SPH density using the cubic spline kernel |
//! | [`Dispatcher::sort_f32`] | Ascending sort via bitonic sort |
//!
//! Each method compiles the shader, uploads inputs, dispatches workgroups, and
//! reads back results — all within a single synchronous call.

use crate::{
    buffer::{read_back, storage_buffer_init, uniform_buffer},
    context::ComputeContext,
    pipeline::compute_pipeline,
    wgsl::{
        SHADER_BITONIC_SORT, SHADER_MAP_F32_TEMPLATE, SHADER_REDUCTION_SUM, SHADER_SPH_DENSITY,
        SHADER_ZIP_MAP_F32_TEMPLATE,
    },
};
use wgpu;

/// The mathematical constant π for use in SPH kernel coefficient computation.
const PI: f32 = std::f32::consts::PI;

// ── WGSL expression validation ────────────────────────────────────────────────

/// Validate that `op` is a safe WGSL arithmetic expression before template
/// interpolation.
///
/// Rejects any byte outside the safe ASCII set for arithmetic expressions:
/// letters, digits, whitespace, `+`, `-`, `*`, `/`, `%`, `!`, `<`, `>`, `=`,
/// `(`, `)`, `,`, `.`, `_`.  In particular, blocks structural WGSL characters
/// `{`, `}`, `;`, `@`, `#`, newlines, colons, and quotes that could escape the
/// expression context and inject arbitrary shader code.
///
/// # Errors
/// Returns `Err` with a description if `op` contains a forbidden byte.
fn validate_wgsl_op(op: &str) -> Result<(), &'static str> {
    for b in op.bytes() {
        let allowed = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b' ' | b'\t'
                    | b'+'
                    | b'-'
                    | b'*'
                    | b'/'
                    | b'%'
                    | b'!'
                    | b'<'
                    | b'>'
                    | b'='
                    | b'('
                    | b')'
                    | b','
                    | b'.'
                    | b'_'
            );
        if !allowed {
            return Err("op contains a character not permitted in a WGSL expression");
        }
    }
    Ok(())
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// High-level compute dispatch helpers built on [`ComputeContext`].
///
/// Obtain a `Dispatcher` via [`ComputeContext::dispatcher`] or
/// [`Dispatcher::new`].
///
/// # Example
/// ```rust,no_run
/// use oxiui_compute_wgpu::ComputeContext;
///
/// if let Some(ctx) = ComputeContext::try_new() {
///     let d = ctx.dispatcher();
///     let out = d.map_f32(&[1.0, 2.0, 3.0], "x * 2.0");
///     assert_eq!(out, vec![2.0, 4.0, 6.0]);
/// }
/// ```
pub struct Dispatcher<'a> {
    ctx: &'a ComputeContext,
}

impl<'a> Dispatcher<'a> {
    /// Create a `Dispatcher` borrowing `ctx`.
    pub fn new(ctx: &'a ComputeContext) -> Self {
        Self { ctx }
    }

    /// Map each `f32` element of `src` through a WGSL expression `op`.
    ///
    /// `op` is a WGSL expression that uses the variable `x` (the current
    /// element) and must evaluate to an `f32`.  For example `"x * 2.0"` doubles
    /// every element.
    ///
    /// Returns a `Vec<f32>` of the same length as `src`.
    ///
    /// # Panics
    /// Panics if `src` is empty.
    /// Panics if `op` contains characters outside the allowed set for WGSL arithmetic
    /// expressions (letters, digits, whitespace, `+−*/%(,).!<>=_`). This rejects structural
    /// WGSL characters (`{`, `}`, `;`, `@`, newlines, etc.) that could escape the expression
    /// context and inject arbitrary shader code.
    pub fn map_f32(&self, src: &[f32], op: &str) -> Vec<f32> {
        assert!(
            !src.is_empty(),
            "Dispatcher::map_f32: src must be non-empty"
        );
        assert!(
            validate_wgsl_op(op).is_ok(),
            "Dispatcher::map_f32: invalid WGSL expression — op contains forbidden characters: {op:?}"
        );
        let n = src.len() as u32;

        // Instantiate shader template.
        let wgsl = SHADER_MAP_F32_TEMPLATE.replace("%%OP%%", op);

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        // Upload input buffer.
        let src_buf = storage_buffer_init(device, "map-src", bytemuck::cast_slice(src));

        // Allocate output buffer (zeros).
        let dst_buf = storage_buffer_init(
            device,
            "map-dst",
            bytemuck::cast_slice(&vec![0.0_f32; src.len()]),
        );

        // Upload uniform n.
        let n_buf = uniform_buffer(device, "map-n", bytemuck::bytes_of(&n));

        // Build pipeline.
        let pipeline = compute_pipeline(device, &wgsl, "main_map");
        let bg_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("map-bg"),
            layout: &bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: src_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dst_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: n_buf.as_entire_binding(),
                },
            ],
        });

        // Dispatch.
        let workgroups = n.div_ceil(64);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("map-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("map-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));

        read_back::<f32>(device, queue, &dst_buf, src.len())
    }

    /// Zip two `f32` slices with a WGSL binary expression `op`.
    ///
    /// `op` is a WGSL expression that uses `a` (element from the first slice)
    /// and `b` (element from the second slice) and must evaluate to an `f32`.
    /// For example `"a + b"` computes element-wise addition.
    ///
    /// Returns a `Vec<f32>` of the same length as `a`.
    ///
    /// # Panics
    /// Panics if `a` is empty or `a.len() != b.len()`.
    /// Panics if `op` contains characters outside the allowed set for WGSL arithmetic
    /// expressions (letters, digits, whitespace, `+−*/%(,).!<>=_`). This rejects structural
    /// WGSL characters (`{`, `}`, `;`, `@`, newlines, etc.) that could escape the expression
    /// context and inject arbitrary shader code.
    pub fn zip_map_f32(&self, a: &[f32], b: &[f32], op: &str) -> Vec<f32> {
        assert!(
            !a.is_empty(),
            "Dispatcher::zip_map_f32: a must be non-empty"
        );
        assert_eq!(
            a.len(),
            b.len(),
            "Dispatcher::zip_map_f32: a and b must have equal length"
        );
        assert!(
            validate_wgsl_op(op).is_ok(),
            "Dispatcher::zip_map_f32: invalid WGSL expression — op contains forbidden characters: {op:?}"
        );
        let n = a.len() as u32;

        // Instantiate shader template.
        let wgsl = SHADER_ZIP_MAP_F32_TEMPLATE.replace("%%OP%%", op);

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        let a_buf = storage_buffer_init(device, "zip-a", bytemuck::cast_slice(a));
        let b_buf = storage_buffer_init(device, "zip-b", bytemuck::cast_slice(b));
        let dst_buf = storage_buffer_init(
            device,
            "zip-dst",
            bytemuck::cast_slice(&vec![0.0_f32; a.len()]),
        );
        let n_buf = uniform_buffer(device, "zip-n", bytemuck::bytes_of(&n));

        let pipeline = compute_pipeline(device, &wgsl, "main_zip_map");
        let bg_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zip-bg"),
            layout: &bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: a_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: b_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dst_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: n_buf.as_entire_binding(),
                },
            ],
        });

        let workgroups = n.div_ceil(64);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zip-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("zip-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));

        read_back::<f32>(device, queue, &dst_buf, a.len())
    }

    /// Sum all `f32` elements in `data` using the GPU reduction shader.
    ///
    /// Uses the built-in `SHADER_REDUCTION_SUM` (single workgroup, ≤ 256
    /// elements).  For data longer than 256 elements the result will silently
    /// only sum the first 256; callers requiring arbitrarily large reductions
    /// should tile manually.
    ///
    /// # Panics
    /// Panics if `data` is empty.
    pub fn reduce_sum_f32(&self, data: &[f32]) -> f32 {
        assert!(
            !data.is_empty(),
            "Dispatcher::reduce_sum_f32: data must be non-empty"
        );

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        let input_buf = storage_buffer_init(device, "reduce-in", bytemuck::cast_slice(data));
        let output_buf =
            storage_buffer_init(device, "reduce-out", bytemuck::cast_slice(&[0.0_f32]));

        let pipeline = compute_pipeline(device, SHADER_REDUCTION_SUM, "main_cs");
        let bg_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reduce-bg"),
            layout: &bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("reduce-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("reduce-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));

        let result = read_back::<f32>(device, queue, &output_buf, 1);
        result[0]
    }

    /// Compute SPH density for all particles using the poly-6 kernel W(r,h).
    ///
    /// The density for particle `i` is:
    /// `ρᵢ = Σⱼ mⱼ · (315 / (64π h⁹)) · (h² − |rᵢ − rⱼ|²)³`  for `|rᵢ − rⱼ| ≤ h`.
    ///
    /// # Parameters
    /// - `positions` — flat `[[x, y, z]; n]` positions for each particle.
    /// - `masses`    — per-particle mass; must have the same length as `positions`.
    /// - `h`         — smoothing radius.
    ///
    /// Returns `Vec<f32>` of densities, one per particle.
    ///
    /// # Panics
    /// Panics if `positions` is empty or `positions.len() != masses.len()`.
    pub fn sph_density(&self, positions: &[[f32; 3]], masses: &[f32], h: f32) -> Vec<f32> {
        assert!(
            !positions.is_empty(),
            "Dispatcher::sph_density: positions must be non-empty"
        );
        assert_eq!(
            positions.len(),
            masses.len(),
            "Dispatcher::sph_density: positions and masses must have equal length"
        );

        let n = positions.len() as u32;
        let h_sq = h * h;
        // Poly-6 kernel coefficient: 315 / (64 π h⁹)
        let kernel_coeff = 315.0_f32 / (64.0 * PI * h.powi(9));

        // Pack positions as vec4 (xyz, 0.0).
        let positions_vec4: Vec<f32> = positions
            .iter()
            .flat_map(|&[x, y, z]| [x, y, z, 0.0_f32])
            .collect();

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        let pos_buf = storage_buffer_init(device, "sph-pos", bytemuck::cast_slice(&positions_vec4));
        let mass_buf = storage_buffer_init(device, "sph-mass", bytemuck::cast_slice(masses));
        let density_buf = storage_buffer_init(
            device,
            "sph-density",
            bytemuck::cast_slice(&vec![0.0_f32; positions.len()]),
        );

        // SphParams struct: { n: u32, h_sq: f32, kernel_coeff: f32, _pad: u32 }
        // Laid out as 4 × u32/f32 = 16 bytes.
        let params_bytes: [u8; 16] = {
            let mut bytes = [0u8; 16];
            bytes[0..4].copy_from_slice(&n.to_ne_bytes());
            bytes[4..8].copy_from_slice(&h_sq.to_ne_bytes());
            bytes[8..12].copy_from_slice(&kernel_coeff.to_ne_bytes());
            bytes[12..16].copy_from_slice(&0u32.to_ne_bytes());
            bytes
        };
        let params_buf = uniform_buffer(device, "sph-params", &params_bytes);

        let pipeline = compute_pipeline(device, SHADER_SPH_DENSITY, "main_sph");
        let bg_layout = pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sph-bg"),
            layout: &bg_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: pos_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mass_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: density_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        let workgroups = n.div_ceil(64);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sph-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("sph-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        queue.submit(std::iter::once(encoder.finish()));

        read_back::<f32>(device, queue, &density_buf, positions.len())
    }

    /// Sort a slice of `f32` values in ascending order using the GPU bitonic sort.
    ///
    /// The input is padded to the next power of two with `f32::MAX` sentinel
    /// values so padding elements always sort to the end.  The returned `Vec`
    /// has the same length as the input (padding is stripped).
    ///
    /// The implementation issues one GPU dispatch per (k, j) step of the
    /// standard bitonic-sort algorithm, so any padded size is supported.
    ///
    /// # Panics
    /// Panics if `data` is empty.
    pub fn sort_f32(&self, data: &[f32]) -> Vec<f32> {
        assert!(
            !data.is_empty(),
            "Dispatcher::sort_f32: data must be non-empty"
        );

        let original_len = data.len();

        // Pad to next power of two so the bitonic network is well-defined.
        let padded_len = original_len.next_power_of_two();

        let mut padded = Vec::with_capacity(padded_len);
        padded.extend_from_slice(data);
        padded.resize(padded_len, f32::MAX);

        let n = padded_len as u32;

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        let data_buf = storage_buffer_init(device, "sort-data", bytemuck::cast_slice(&padded));

        // Compile the per-step pipeline once.
        let pipeline = compute_pipeline(device, SHADER_BITONIC_SORT, "main_bitonic");
        let bg_layout = pipeline.get_bind_group_layout(0);
        let workgroups = n.div_ceil(64);

        // BitonicStep uniform layout: { n: u32, k: u32, j: u32, _pad: u32 }
        // Outer loop: k = 2, 4, 8, … n
        // Inner loop: j = k/2, k/4, … 1
        let mut k: u32 = 2;
        while k <= n {
            let mut j = k >> 1;
            while j >= 1 {
                // Upload per-step uniform.
                let step_bytes: [u8; 16] = {
                    let mut b = [0u8; 16];
                    b[0..4].copy_from_slice(&n.to_ne_bytes());
                    b[4..8].copy_from_slice(&k.to_ne_bytes());
                    b[8..12].copy_from_slice(&j.to_ne_bytes());
                    b[12..16].copy_from_slice(&0u32.to_ne_bytes());
                    b
                };
                let step_buf = uniform_buffer(device, "sort-step", &step_bytes);

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sort-bg"),
                    layout: &bg_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: data_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: step_buf.as_entire_binding(),
                        },
                    ],
                });

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("sort-step-encoder"),
                });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("sort-step-pass"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &bind_group, &[]);
                    pass.dispatch_workgroups(workgroups, 1, 1);
                }
                queue.submit(std::iter::once(encoder.finish()));

                // Ensure each step completes before the next (reads-after-writes).
                device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .expect("sort_f32: device poll failed");

                j >>= 1;
            }
            k <<= 1;
        }

        let sorted = read_back::<f32>(device, queue, &data_buf, padded_len);
        // Truncate to original length, removing f32::MAX padding sentinels.
        sorted[..original_len].to_vec()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_f32_doubles() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let d = Dispatcher::new(&ctx);
        let out = d.map_f32(&[1.0_f32, 2.0, 3.0], "x * 2.0");
        assert_eq!(out.len(), 3);
        assert!((out[0] - 2.0).abs() < 1e-5, "expected 2.0, got {}", out[0]);
        assert!((out[1] - 4.0).abs() < 1e-5, "expected 4.0, got {}", out[1]);
        assert!((out[2] - 6.0).abs() < 1e-5, "expected 6.0, got {}", out[2]);
    }

    #[test]
    fn zip_map_f32_adds() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let d = Dispatcher::new(&ctx);
        let out = d.zip_map_f32(&[1.0_f32, 2.0], &[3.0, 4.0], "a + b");
        assert_eq!(out.len(), 2);
        assert!((out[0] - 4.0).abs() < 1e-5, "expected 4.0, got {}", out[0]);
        assert!((out[1] - 6.0).abs() < 1e-5, "expected 6.0, got {}", out[1]);
    }

    #[test]
    fn reduce_sum_f32_correct() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let d = Dispatcher::new(&ctx);
        let sum = d.reduce_sum_f32(&[1.0_f32, 2.0, 3.0, 4.0]);
        assert!((sum - 10.0).abs() < 1e-3, "expected 10.0, got {sum}");
    }

    #[test]
    fn sph_density_single_particle() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let d = Dispatcher::new(&ctx);
        // Single particle at origin, mass 1.0, smoothing length h=1.0.
        // Self-contribution: W(0, 1) = (315/(64π·1⁹)) · (1² − 0²)³ = 315/(64π) > 0.
        let positions = [[0.0_f32, 0.0, 0.0]];
        let masses = [1.0_f32];
        let densities = d.sph_density(&positions, &masses, 1.0);
        assert_eq!(densities.len(), 1);
        assert!(
            densities[0] > 0.0,
            "single-particle density must be > 0, got {}",
            densities[0]
        );
    }

    #[test]
    fn sort_f32_small() {
        oxiui_core::require_gpu!(ctx, ComputeContext::try_new());
        let d = Dispatcher::new(&ctx);
        let out = d.sort_f32(&[4.0_f32, 2.0, 3.0, 1.0]);
        assert_eq!(out, vec![1.0_f32, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn map_f32_rejects_injection() {
        // No GPU needed — validation fires before any GPU call.
        let result =
            std::panic::catch_unwind(|| validate_wgsl_op("x; } @compute fn evil() {").unwrap());
        assert!(result.is_err(), "injection expression should be rejected");
    }

    #[test]
    fn validate_wgsl_op_accepts_valid_expressions() {
        for expr in &[
            "x * 2.0",
            "a + b",
            "sqrt(x)",
            "max(a, b)",
            "sin(x) + cos(x)",
            "x * x + 1.0",
            "a / (b + 1.0)",
        ] {
            assert!(
                validate_wgsl_op(expr).is_ok(),
                "should accept valid expression: {expr}"
            );
        }
    }

    #[test]
    fn validate_wgsl_op_rejects_injection_chars() {
        for bad in &[
            "x; }",
            "x\n@compute",
            "x{evil}",
            "x; @group(0)",
            "x // comment\n}",
            "x: f32",
        ] {
            assert!(validate_wgsl_op(bad).is_err(), "should reject: {bad:?}");
        }
    }
}
