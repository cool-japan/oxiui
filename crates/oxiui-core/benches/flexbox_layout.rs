/// Flexbox layout benchmark for oxiui-core.
///
/// Benchmarks the cost of computing a flex layout pass over a list of items.
/// Uses the real `FlexLayout` engine from `oxiui-core` for accurate measurements.
use criterion::{criterion_group, criterion_main, Criterion};
use oxiui_core::geometry::{Rect, Size};
use oxiui_core::layout::{layout_subtrees_parallel, FlexItem, FlexLayout, LayoutTask};
use std::hint::black_box;

fn bench_flex_layout_10(c: &mut Criterion) {
    let layout = FlexLayout::row();
    let items: Vec<FlexItem> = (0..10)
        .map(|i| FlexItem::fixed(Size::new(80.0 + i as f32, 24.0)))
        .collect();
    let container = Rect::new(0.0, 0.0, 1024.0, 40.0);
    c.bench_function("flex_layout_10_items", |b| {
        b.iter(|| black_box(layout.layout(black_box(container), black_box(&items))))
    });
}

fn bench_flex_layout_100(c: &mut Criterion) {
    let layout = FlexLayout::row();
    let items: Vec<FlexItem> = (0..100)
        .map(|i| FlexItem::fixed(Size::new(40.0 + (i % 5) as f32 * 10.0, 20.0)))
        .collect();
    let container = Rect::new(0.0, 0.0, 1920.0, 40.0);
    c.bench_function("flex_layout_100_items", |b| {
        b.iter(|| black_box(layout.layout(black_box(container), black_box(&items))))
    });
}

fn bench_parallel_layout_8x10(c: &mut Criterion) {
    let tasks: Vec<LayoutTask> = (0..8)
        .map(|i| LayoutTask {
            layout: FlexLayout::row(),
            container: Rect::new(0.0, 0.0, 1024.0, 40.0),
            items: (0..10)
                .map(|j| FlexItem::fixed(Size::new(80.0 + (i + j) as f32, 24.0)))
                .collect(),
        })
        .collect();
    c.bench_function("parallel_layout_8_containers_10_items_each", |b| {
        b.iter(|| black_box(layout_subtrees_parallel(black_box(&tasks))))
    });
}

criterion_group!(
    benches,
    bench_flex_layout_10,
    bench_flex_layout_100,
    bench_parallel_layout_8x10,
);
criterion_main!(benches);
