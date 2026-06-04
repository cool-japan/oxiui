/// Rounded-rectangle rasterisation benchmark for oxiui-render-wgpu.
///
/// Measures the CPU-side cost of preparing rounded-rect draw commands
/// (bounding box computation, clip-rect intersection) without requiring
/// GPU hardware.
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

/// Simulate the bounding-box computation for a rounded rect.
fn compute_rrect_bounds(x: f32, y: f32, w: f32, h: f32, r: f32) -> [f32; 4] {
    let clamped_r = r.min(w / 2.0).min(h / 2.0);
    [
        x - clamped_r * 0.05,
        y - clamped_r * 0.05,
        w + clamped_r * 0.1,
        h + clamped_r * 0.1,
    ]
}

fn bench_rrect_bounds_1000(c: &mut Criterion) {
    c.bench_function("rrect_bounds_1000", |b| {
        b.iter(|| {
            let mut sum = 0.0f32;
            for i in 0..1000u32 {
                let r = i as f32 % 20.0;
                let bounds = compute_rrect_bounds(
                    black_box(i as f32),
                    black_box(i as f32 * 0.5),
                    black_box(100.0 + r),
                    black_box(24.0 + r * 0.5),
                    black_box(r),
                );
                sum += bounds[0] + bounds[1] + bounds[2] + bounds[3];
            }
            black_box(sum)
        })
    });
}

criterion_group!(benches, bench_rrect_bounds_1000);
criterion_main!(benches);
