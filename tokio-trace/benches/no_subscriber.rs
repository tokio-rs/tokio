#[macro_use]
extern crate tokio_trace;
#[macro_use]
extern crate log;
#[macro_use]
extern crate criterion;

use criterion::Criterion;
use tokio_trace::Level;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("span_no_subscriber", |b| {
        b.iter(|| {
            span!(Level::TRACE, "span");
        })
    });
    c.bench_function("bench_log_no_logger", |b| {
        b.iter(|| {
            log!(log::Level::Info, "log");
        });
    });
    c.bench_function("bench_costly_field_no_subscriber", |b| {
        b.iter(|| {
            span!(
                Level::TRACE,
                "span",
                foo = tokio_trace::field::display(format!("bar {:?}", 2))
            );
        });
    });
    // This is just included as a baseline.
    c.bench_function("bench_no_span_no_subscriber", |b| {
        b.iter(|| {});
    });
    c.bench_function("bench_1_atomic_load", |b| {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let foo = AtomicUsize::new(1);
        b.iter(|| foo.load(Ordering::Relaxed));
    });
}
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
