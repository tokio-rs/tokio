#![feature(test)]
#[macro_use]
extern crate tokio_trace;
#[macro_use]
extern crate log;
extern crate test;
use test::Bencher;

#[bench]
fn bench_span_no_subscriber(b: &mut Bencher) {
    b.iter(|| {
        span!("span");
    });
}

#[bench]
fn bench_log_no_logger(b: &mut Bencher) {
    b.iter(|| {
        log!(log::Level::Info, "log");
    });
}

#[bench]
fn bench_costly_field_no_subscriber(b: &mut Bencher) {
    b.iter(|| {
        span!(
            "span",
            foo = tokio_trace::field::display(format!("bar {:?}", 2))
        );
    });
}

#[bench]
fn bench_no_span_no_subscriber(b: &mut Bencher) {
    b.iter(|| {});
}

#[bench]
fn bench_1_atomic_load(b: &mut Bencher) {
    // This is just included as a baseline.
    use std::sync::atomic::{AtomicUsize, Ordering};
    let foo = AtomicUsize::new(1);
    b.iter(|| foo.load(Ordering::Relaxed));
}
