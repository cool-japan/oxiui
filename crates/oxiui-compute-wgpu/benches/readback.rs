//! Benchmark: `read_back` throughput for 1 MiB, 16 MiB, and 256 MiB buffers.
//!
//! Reports throughput in GiB/s.  GPU tests are skipped automatically when no
//! adapter is available (e.g. headless CI).  The CPU-only fallback path
//! benchmarks metadata overhead so CI always produces some data.
//!
//! Run with:
//! ```shell
//! cargo bench -p oxiui-compute-wgpu --bench readback
//! ```
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxiui_compute_wgpu::{
    buffer::{read_back, storage_buffer_init},
    ComputeContext,
};
use std::hint::black_box;

/// Buffer sizes to benchmark: 1 MiB, 16 MiB, 256 MiB.
const SIZES_MIB: &[u64] = &[1, 16, 256];

fn bench_readback_throughput(c: &mut Criterion) {
    // Skip GPU benchmarks gracefully when no adapter is available.
    let ctx = match ComputeContext::new() {
        Ok(c) => c,
        Err(_) => {
            // No GPU on this machine — run a trivial CPU fallback.
            bench_readback_cpu_fallback(c);
            return;
        }
    };

    let mut group = c.benchmark_group("readback_throughput_gpu");
    // Fewer samples for large GPU allocations to keep wall time reasonable.
    group.sample_size(10);

    for &mib in SIZES_MIB {
        let size_bytes = mib * 1024 * 1024;
        let n_f32 = (size_bytes / 4) as usize;
        group.throughput(Throughput::Bytes(size_bytes));

        // Pre-allocate the GPU buffer filled with sequential f32 values.
        let data: Vec<f32> = (0..n_f32).map(|i| i as f32).collect();
        let gpu_buf =
            storage_buffer_init(&ctx.device, "bench-readback", bytemuck::cast_slice(&data));

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{mib}_MiB")),
            &size_bytes,
            |b, _| {
                b.iter(|| {
                    let result: Vec<f32> =
                        read_back(&ctx.device, &ctx.queue, &gpu_buf, black_box(n_f32));
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

/// CPU-only fallback: measure Vec allocation + copy overhead so CI machines
/// without GPU still produce benchmark data in the same size categories.
fn bench_readback_cpu_fallback(c: &mut Criterion) {
    let mut group = c.benchmark_group("readback_throughput_cpu_fallback");
    group.sample_size(20);

    for &mib in SIZES_MIB {
        let size_bytes = mib * 1024 * 1024;
        let n_f32 = (size_bytes / 4) as usize;
        group.throughput(Throughput::Bytes(size_bytes));

        let src: Vec<f32> = (0..n_f32).map(|i| i as f32).collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{mib}_MiB")),
            &n_f32,
            |b, &n| {
                b.iter(|| {
                    // Simulate the host-side copy that a real read_back performs.
                    let out: Vec<f32> = black_box(&src[..n]).to_vec();
                    black_box(out);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_readback_throughput);
criterion_main!(benches);
