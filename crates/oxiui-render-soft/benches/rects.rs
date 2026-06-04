//! Criterion benchmarks for the oxiui-render-soft CPU framebuffer backend.
//!
//! # Running
//!
//! ```sh
//! cargo bench -p oxiui-render-soft --bench rects
//! ```
//!
//! HTML reports are written to `target/criterion/`.
//!
//! # Key benchmark
//!
//! `fill_10k_rects/1920x1080` — fill 10 000 coloured 16×16 rectangles into a
//! 1920×1080 canvas and measure the total elapsed time.  This is the primary
//! throughput target from the TODO.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use oxiui_core::Color;
use oxiui_render_soft::{
    headless::render_headless_scene, simd_fill::fill_solid, Framebuffer, GradientStop,
    LinearGradient,
};
use std::hint::black_box;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a deterministic sequence of 16×16 non-overlapping rects spread
/// across a canvas.  Each rect is at a unique grid cell; if the canvas is too
/// small for `count` rows, rects wrap and overlap (the benchmark still works).
fn make_rects(count: usize, canvas_w: u32, canvas_h: u32) -> Vec<(f32, f32, f32, f32, Color)> {
    let tile = 18u32; // 16px rect + 2px gap
    let cols = (canvas_w / tile).max(1);
    (0..count)
        .map(|i| {
            let col = (i as u32) % cols;
            let row = (i as u32) / cols;
            let x = (col * tile).min(canvas_w.saturating_sub(16)) as f32;
            let y = (row * tile).min(canvas_h.saturating_sub(16)) as f32;
            let c = Color(
                ((i.wrapping_mul(73)) & 0xFF) as u8,
                ((i.wrapping_mul(137)) & 0xFF) as u8,
                ((i.wrapping_mul(211)) & 0xFF) as u8,
                255,
            );
            (x, y, 16.0, 16.0, c)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// fill_10k_rects — the primary TODO benchmark target
// ---------------------------------------------------------------------------

/// **Primary TODO benchmark.**
///
/// Fill 10 000 16×16 coloured rectangles into a 1920×1080 canvas.  The
/// entire render pipeline (Framebuffer allocation, Canvas creation, 10k rect
/// fills) is timed.
fn bench_fill_10k_rects(c: &mut Criterion) {
    let w = 1920u32;
    let h = 1080u32;
    let rects = make_rects(10_000, w, h);

    let mut group = c.benchmark_group("fill_10k_rects");
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("1920x1080", |b| {
        b.iter(|| {
            let _ = render_headless_scene(w, h, |canvas| {
                for &(x, y, rw, rh, col) in &rects {
                    canvas.fill_rect(
                        black_box(x),
                        black_box(y),
                        black_box(rw),
                        black_box(rh),
                        black_box(col),
                    );
                }
            });
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Parametric fill_rect at multiple canvas sizes
// ---------------------------------------------------------------------------

fn bench_fill_rect(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_rect");
    for &(w, h) in &[
        (16u32, 16u32),
        (64, 64),
        (256, 256),
        (800, 600),
        (1920, 1080),
    ] {
        let pixels = (w * h) as usize;
        group.throughput(Throughput::Elements(pixels as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{w}x{h}")),
            &(w, h),
            |b, &(fw, fh)| {
                b.iter(|| {
                    let _ = render_headless_scene(fw, fh, |canvas| {
                        canvas.fill_rect(
                            0.0,
                            0.0,
                            black_box(fw as f32),
                            black_box(fh as f32),
                            black_box(Color(64, 128, 200, 255)),
                        );
                    });
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// SIMD solid fill (raw framebuffer, no Canvas overhead)
// ---------------------------------------------------------------------------

fn bench_simd_fill_solid(c: &mut Criterion) {
    let color = 0xFF_30_64_96u32;
    let n_pixels = 1920usize * 1080;

    let mut group = c.benchmark_group("simd_fill_solid");
    group.throughput(Throughput::Elements(n_pixels as u64));
    group.bench_function("1920x1080", |b| {
        let mut buf = vec![0u32; n_pixels];
        b.iter(|| {
            fill_solid(black_box(&mut buf), black_box(color));
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Linear gradient (full-canvas)
// ---------------------------------------------------------------------------

fn bench_linear_gradient(c: &mut Criterion) {
    use oxiui_render_soft::clip::ClipRect;

    let stops = vec![
        GradientStop {
            offset: 0.0,
            color: Color(255, 0, 0, 255),
        },
        GradientStop {
            offset: 0.5,
            color: Color(0, 255, 0, 255),
        },
        GradientStop {
            offset: 1.0,
            color: Color(0, 0, 255, 255),
        },
    ];

    let mut group = c.benchmark_group("linear_gradient");
    group.throughput(Throughput::Elements((800 * 600) as u64));
    group.bench_function("800x600", |b| {
        b.iter(|| {
            let mut fb = Framebuffer::with_fill(800, 600, Color(0, 0, 0, 255));
            let grad = LinearGradient::new((0.0, 0.0), (800.0, 0.0), black_box(stops.clone()));
            let clip = ClipRect::full(800, 600);
            grad.fill_rect(&mut fb, &clip, 0.0, 0.0, 800.0, 600.0);
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Alpha blend row
// ---------------------------------------------------------------------------

fn bench_alpha_blend_row(c: &mut Criterion) {
    use oxiui_render_soft::simd_fill::alpha_blend_row;

    let mut group = c.benchmark_group("alpha_blend_row");
    for &n in &[128usize, 1024, 4096] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let src: Vec<u8> = (0..n * 4)
                .map(|i| ((i.wrapping_mul(31).wrapping_add(17)) & 0xFF) as u8)
                .collect();
            let mut dst = vec![0xFF_00_00_FFu32; n];
            b.iter(|| {
                alpha_blend_row(black_box(&src), black_box(&mut dst));
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// PNG encode
// ---------------------------------------------------------------------------

fn bench_png_encode(c: &mut Criterion) {
    // Pre-render a 1920×1080 framebuffer once; benchmark only the PNG encode step.
    let fb = {
        let bg = Color(50, 100, 150, 255);
        let mut f = Framebuffer::with_fill(1920, 1080, bg);
        // Simulate some content.
        for y in 100..300 {
            for x in 100..400 {
                f.set(x, y, 0xFF_FF_C0_00); // golden rect
            }
        }
        f
    };

    c.bench_function("png_encode_1920x1080", |b| {
        b.iter(|| {
            let rgba = fb.to_rgba_buffer();
            let path = std::env::temp_dir().join("oxiui_bench_encode.png");
            let _ = rgba.save_png(black_box(&path));
        });
    });
}

// ---------------------------------------------------------------------------
// Criterion groups
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_fill_10k_rects,
    bench_fill_rect,
    bench_simd_fill_solid,
    bench_linear_gradient,
    bench_alpha_blend_row,
    bench_png_encode,
);
criterion_main!(benches);
