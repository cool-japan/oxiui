//! Benchmark: pipeline compilation time — cold versus warm [`PipelineCache`].
//!
//! Measures:
//! - Cold compilation: each `get_or_compile` call compiles a new unique shader.
//! - Warm lookup: repeated calls with the same (source, entry-point) key return
//!   a cached `Arc<ComputePipeline>` without recompilation.
//!
//! GPU tests are skipped gracefully when no adapter is available.
//!
//! Run with:
//! ```shell
//! cargo bench -p oxiui-compute-wgpu --bench pipeline_cache
//! ```
use criterion::{criterion_group, criterion_main, Criterion};
use oxiui_compute_wgpu::{ComputeContext, PipelineCache};
use std::hint::black_box;

/// A minimal valid WGSL compute shader used in compilation benchmarks.
const NOOP_WGSL: &str = r#"
@group(0) @binding(0) var<storage, read_write> buf: array<f32>;
@compute @workgroup_size(64)
fn noop(@builtin(global_invocation_id) gid: vec3<u32>) {
    buf[gid.x] = buf[gid.x];
}
"#;

/// Generate a family of distinct WGSL shaders for cold-compilation benchmarking.
///
/// Each shader reads from a different binding index comment so the source
/// text differs and hashes to a different key.
fn make_unique_shader(id: usize) -> String {
    format!(
        r#"
// shader id: {id}
@group(0) @binding(0) var<storage, read_write> buf: array<f32>;
@compute @workgroup_size(64)
fn noop_{id}(@builtin(global_invocation_id) gid: vec3<u32>) {{
    buf[gid.x] = buf[gid.x] + 0.0;
}}
"#
    )
}

fn bench_pipeline_cache_cold_gpu(c: &mut Criterion) {
    let ctx = match ComputeContext::new() {
        Ok(c) => c,
        Err(_) => {
            bench_pipeline_cache_cold_cpu_fallback(c);
            return;
        }
    };

    c.bench_function("pipeline_cache_cold_compile_1", |b| {
        b.iter(|| {
            // Each iteration creates a fresh cache and compiles one shader.
            let mut cache = PipelineCache::new();
            let p = cache.get_or_compile(&ctx.device, black_box(NOOP_WGSL), "noop");
            black_box(p);
        });
    });
}

fn bench_pipeline_cache_warm_gpu(c: &mut Criterion) {
    let ctx = match ComputeContext::new() {
        Ok(c) => c,
        Err(_) => {
            // No GPU — skip warm bench (no meaningful data without GPU).
            return;
        }
    };

    // Pre-warm the cache.
    let mut cache = PipelineCache::new();
    let _warmup = cache.get_or_compile(&ctx.device, NOOP_WGSL, "noop");

    c.bench_function("pipeline_cache_warm_hit", |b| {
        b.iter(|| {
            // Cache hit: no recompilation, just Arc clone.
            let p = cache.get_or_compile(&ctx.device, black_box(NOOP_WGSL), "noop");
            black_box(p);
        });
    });
}

/// CPU-only fallback: benchmark `PipelineCache`'s internal hash-map mechanics
/// (key hashing + Arc clone) without actual shader compilation.
fn bench_pipeline_cache_cold_cpu_fallback(c: &mut Criterion) {
    // Simulate the HashMap insert / lookup overhead without GPU.
    c.bench_function("pipeline_cache_hashmap_insert_100", |b| {
        b.iter(|| {
            let mut map = std::collections::HashMap::<u64, std::sync::Arc<String>>::new();
            for i in 0..100u64 {
                let src = make_unique_shader(i as usize);
                let arc = std::sync::Arc::new(src);
                map.insert(black_box(i), arc);
            }
            black_box(map.len())
        })
    });

    c.bench_function("pipeline_cache_hashmap_lookup_hit", |b| {
        let mut map = std::collections::HashMap::<u64, std::sync::Arc<String>>::new();
        for i in 0..64u64 {
            map.insert(i, std::sync::Arc::new(format!("shader_{i}")));
        }
        b.iter(|| {
            let arc = map.get(black_box(&32u64)).map(std::sync::Arc::clone);
            black_box(arc);
        })
    });
}

criterion_group!(
    benches,
    bench_pipeline_cache_cold_gpu,
    bench_pipeline_cache_warm_gpu
);
criterion_main!(benches);
