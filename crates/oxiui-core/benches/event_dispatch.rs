/// Event dispatch benchmark for oxiui-core.
///
/// Measures the overhead of dispatching UI events through a simple routing
/// table, simulating the hot path of the event system.
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn bench_event_dispatch_small(c: &mut Criterion) {
    // Simulate dispatching to 8 listeners (typical widget subtree).
    let listeners: Vec<u64> = (0..8).collect();

    c.bench_function("event_dispatch_8_listeners", |b| {
        b.iter(|| {
            let mut handled = false;
            for &id in &listeners {
                if black_box(id) == 4 {
                    handled = true;
                    break;
                }
            }
            black_box(handled)
        })
    });
}

fn bench_event_dispatch_deep(c: &mut Criterion) {
    // Simulate bubbling through a 32-deep widget tree.
    let ancestors: Vec<u64> = (0..32).rev().collect();

    c.bench_function("event_bubble_32_levels", |b| {
        b.iter(|| {
            let mut stopped = false;
            for &id in &ancestors {
                if black_box(id) == 16 {
                    stopped = true;
                    break;
                }
            }
            black_box(stopped)
        })
    });
}

criterion_group!(
    benches,
    bench_event_dispatch_small,
    bench_event_dispatch_deep
);
criterion_main!(benches);
