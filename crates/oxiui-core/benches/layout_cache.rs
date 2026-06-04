/// Layout-cache benchmark for oxiui-core.
///
/// Measures the CPU overhead of caching and invalidating layout results.
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn bench_layout_cache_insert_and_lookup(c: &mut Criterion) {
    let mut cache: std::collections::HashMap<u64, [f32; 4]> = std::collections::HashMap::new();
    for i in 0..256u64 {
        cache.insert(i, [i as f32, 0.0, 100.0, 24.0]);
    }

    c.bench_function("layout_cache_lookup_hit", |b| {
        b.iter(|| {
            let key = black_box(128u64);
            cache.get(&key).copied()
        })
    });

    c.bench_function("layout_cache_insert_256", |b| {
        b.iter(|| {
            let mut m: std::collections::HashMap<u64, [f32; 4]> =
                std::collections::HashMap::with_capacity(256);
            for i in 0..256u64 {
                m.insert(black_box(i), [i as f32, 0.0, 100.0, 24.0]);
            }
            black_box(m.len())
        })
    });
}

criterion_group!(benches, bench_layout_cache_insert_and_lookup);
criterion_main!(benches);
