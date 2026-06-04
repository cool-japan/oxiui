//! Tiled matrix multiply example for oxiui-compute-wgpu.
//!
//! Run: `cargo run --example matrix_mul -p oxiui-compute-wgpu`
//! Requires a GPU. Prints a skip message and exits cleanly without one.

use oxiui_compute_wgpu::{
    bytemuck, compute_pipeline, dispatch_2d, read_back, storage_buffer_init, uniform_buffer, wgpu,
    ComputeContext, SHADER_MATMUL,
};

/// Uniform struct matching `struct MatDims { M: u32, K: u32, N: u32 }` in the WGSL shader.
/// Field order: M (rows of A/C), K (cols of A / rows of B), N (cols of B/C).
/// The `_pad` field brings the struct to 16 bytes for WGSL uniform alignment.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MatDims {
    m: u32,
    k: u32,
    n: u32,
    _pad: u32,
}

/// Simple CPU reference matrix multiply (row-major).
fn cpu_matmul(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
    let mut c = vec![0.0f32; m * n];
    for i in 0..m {
        for j in 0..n {
            for l in 0..k {
                c[i * n + j] += a[i * k + l] * b[l * n + j];
            }
        }
    }
    c
}

fn main() {
    let Some(ctx) = ComputeContext::try_new() else {
        println!("[skip] No GPU adapter found — matrix_mul example requires a GPU.");
        return;
    };

    println!("Adapter: {:?}", ctx.adapter_info().backend);

    // 4×3 × 3×2 → 4×2
    let m = 4usize;
    let k = 3usize;
    let n = 2usize;

    let a: Vec<f32> = vec![
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
    ];
    let b: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];

    let expected = cpu_matmul(&a, &b, m, k, n);

    // Upload A and B buffers
    let buf_a = storage_buffer_init(&ctx.device, "mat-A", bytemuck::cast_slice(&a));
    let buf_b = storage_buffer_init(&ctx.device, "mat-B", bytemuck::cast_slice(&b));

    // Output buffer C (zero-initialized, STORAGE | COPY_SRC)
    let c_size = (m * n * std::mem::size_of::<f32>()) as u64;
    let buf_c = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mat-C"),
        size: c_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Uniform dims — field order must match WGSL `struct MatDims { M, K, N }`
    let dims = MatDims {
        m: m as u32,
        k: k as u32,
        n: n as u32,
        _pad: 0,
    };
    let buf_dims = uniform_buffer(&ctx.device, "mat-dims", bytemuck::bytes_of(&dims));

    // Compile shader and create bind group
    let pipeline = compute_pipeline(&ctx.device, SHADER_MATMUL, "main_cs");
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("matmul-bg"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buf_a.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buf_b.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: buf_c.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: buf_dims.as_entire_binding(),
            },
        ],
    });

    // Dispatch: ceil(N/16) x-workgroups (columns), ceil(M/16) y-workgroups (rows).
    // dispatch_2d(width=N, height=M, wg_x=16, wg_y=16) matches gid.x=col, gid.y=row.
    let (gx, gy) = dispatch_2d(n as u32, m as u32, 16, 16);
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("matmul-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(gx, gy, 1);
    }
    ctx.queue.submit(std::iter::once(encoder.finish()));

    // Read back C
    let output: Vec<f32> = read_back(&ctx.device, &ctx.queue, &buf_c, m * n);

    println!("A ({}x{}): {:?}", m, k, a);
    println!("B ({}x{}): {:?}", k, n, b);
    println!("Expected C ({}x{}): {:?}", m, n, expected);
    println!("GPU C     ({}x{}): {:?}", m, n, output);

    let ok = output
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| (a - b).abs() < 0.1);
    if ok {
        println!("OK — matrix multiply matches CPU reference.");
    } else {
        println!("MISMATCH — GPU result differs from CPU reference!");
        std::process::exit(1);
    }
}
