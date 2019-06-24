#[macro_use]
extern crate tokio_trace;
#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion};
use tokio_trace::Level;

use std::{
    fmt,
    sync::{Mutex, MutexGuard},
};
use tokio_trace::{field, span, Event, Id, Metadata};

/// A subscriber that is enabled but otherwise does nothing.
struct EnabledSubscriber;

impl tokio_trace::Subscriber for EnabledSubscriber {
    fn new_span(&self, span: &span::Attributes) -> Id {
        let _ = span;
        Id::from_u64(0xDEADFACE)
    }

    fn event(&self, event: &Event) {
        let _ = event;
    }

    fn record(&self, span: &Id, values: &span::Record) {
        let _ = (span, values);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        let _ = (span, follows);
    }

    fn enabled(&self, metadata: &Metadata) -> bool {
        let _ = metadata;
        true
    }

    fn enter(&self, span: &Id) {
        let _ = span;
    }

    fn exit(&self, span: &Id) {
        let _ = span;
    }
}

/// Simulates a subscriber that records span data.
struct VisitingSubscriber(Mutex<String>);

struct Visitor<'a>(MutexGuard<'a, String>);

impl<'a> field::Visit for Visitor<'a> {
    fn record_debug(&mut self, _field: &field::Field, value: &fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(&mut *self.0, "{:?}", value);
    }
}

impl tokio_trace::Subscriber for VisitingSubscriber {
    fn new_span(&self, span: &span::Attributes) -> Id {
        let mut visitor = Visitor(self.0.lock().unwrap());
        span.record(&mut visitor);
        Id::from_u64(0xDEADFACE)
    }

    fn record(&self, _span: &Id, values: &span::Record) {
        let mut visitor = Visitor(self.0.lock().unwrap());
        values.record(&mut visitor);
    }

    fn event(&self, event: &Event) {
        let mut visitor = Visitor(self.0.lock().unwrap());
        event.record(&mut visitor);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        let _ = (span, follows);
    }

    fn enabled(&self, metadata: &Metadata) -> bool {
        let _ = metadata;
        true
    }

    fn enter(&self, span: &Id) {
        let _ = span;
    }

    fn exit(&self, span: &Id) {
        let _ = span;
    }
}

const N_SPANS: usize = 100;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("span_no_fields", |b| {
        tokio_trace::subscriber::with_default(EnabledSubscriber, || {
            b.iter(|| span!(Level::TRACE, "span"))
        });
    });

    c.bench_function("enter_span", |b| {
        tokio_trace::subscriber::with_default(EnabledSubscriber, || {
            let span = span!(Level::TRACE, "span");
            b.iter(|| black_box(span.in_scope(|| {})))
        });
    });

    c.bench_function("span_repeatedly", |b| {
        #[inline]
        fn mk_span(i: u64) -> tokio_trace::Span {
            span!(Level::TRACE, "span", i = i)
        }

        let n = black_box(N_SPANS);
        tokio_trace::subscriber::with_default(EnabledSubscriber, || {
            b.iter(|| (0..n).fold(mk_span(0), |_, i| mk_span(i as u64)))
        });
    });

    c.bench_function("span_with_fields", |b| {
        tokio_trace::subscriber::with_default(EnabledSubscriber, || {
            b.iter(|| {
                span!(
                    Level::TRACE,
                    "span",
                    foo = "foo",
                    bar = "bar",
                    baz = 3,
                    quuux = tokio_trace::field::debug(0.99)
                )
            })
        });
    });

    c.bench_function("span_with_fields_record", |b| {
        let subscriber = VisitingSubscriber(Mutex::new(String::from("")));
        tokio_trace::subscriber::with_default(subscriber, || {
            b.iter(|| {
                span!(
                    Level::TRACE,
                    "span",
                    foo = "foo",
                    bar = "bar",
                    baz = 3,
                    quuux = tokio_trace::field::debug(0.99)
                )
            })
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
