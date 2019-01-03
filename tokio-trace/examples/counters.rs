#[macro_use]
extern crate tokio_trace;

use tokio_trace::{
    field, span,
    subscriber::{self, Subscriber},
    Id, Metadata,
};

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
};

#[derive(Clone)]
struct Counters(Arc<RwLock<HashMap<String, AtomicUsize>>>);

struct CounterSubscriber {
    ids: AtomicUsize,
    counters: Counters,
}

impl Subscriber for CounterSubscriber {
    fn register_callsite(&self, meta: &tokio_trace::Metadata) -> subscriber::Interest {
        let mut interest = subscriber::Interest::NEVER;
        for key in meta.fields() {
            if let Some(name) = key.name() {
                if name.contains("count") {
                    self.counters
                        .0
                        .write()
                        .unwrap()
                        .entry(name.to_owned())
                        .or_insert_with(|| AtomicUsize::new(0));
                    interest = subscriber::Interest::ALWAYS;
                }
            }
        }
        interest
    }

    fn new_span(&self, _new_span: &Metadata) -> Id {
        let id = self.ids.fetch_add(1, Ordering::SeqCst);
        Id::from_u64(id as u64)
    }

    fn add_follows_from(&self, _span: &Id, _follows: Id) {
        // unimplemented
    }

    fn record_i64(&self, _id: &Id, field: &field::Field, value: i64) {
        let registry = self.counters.0.read().unwrap();
        if let Some(counter) = field.name().and_then(|name| registry.get(name)) {
            if value > 0 {
                counter.fetch_add(value as usize, Ordering::Release);
            } else {
                counter.fetch_sub(value as usize, Ordering::Release);
            }
        };
    }

    fn record_u64(&self, _id: &Id, field: &field::Field, value: u64) {
        let registry = self.counters.0.read().unwrap();
        if let Some(counter) = field.name().and_then(|name| registry.get(name)) {
            counter.fetch_add(value as usize, Ordering::Release);
        };
    }

    fn record_debug(&self, _id: &Id, _field: &field::Field, _value: &::std::fmt::Debug) {}

    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata
            .fields()
            .iter()
            .any(|f| f.name().map(|name| name.contains("count")).unwrap_or(false))
    }

    fn enter(&self, _span: &Id) {}
    fn exit(&self, _span: &Id) {}
}

impl Counters {
    fn print_counters(&self) {
        for (k, v) in self.0.read().unwrap().iter() {
            println!("{}: {}", k, v.load(Ordering::Acquire));
        }
    }

    fn new() -> (Self, CounterSubscriber) {
        let counters = Counters(Arc::new(RwLock::new(HashMap::new())));
        let subscriber = CounterSubscriber {
            ids: AtomicUsize::new(0),
            counters: counters.clone(),
        };
        (counters, subscriber)
    }
}

fn main() {
    let (counters, subscriber) = Counters::new();

    tokio_trace::subscriber::with_default(subscriber, || {
        let mut foo: u64 = 2;
        span!("my_great_span", foo_count = &foo).enter(|| {
            foo += 1;
            info!({ yak_shaved = true, yak_count = 1 }, "hi from inside my span");
            span!("my other span", foo_count = &foo, baz_count = 5).enter(|| {
                warn!({ yak_shaved = false, yak_count = -1 }, "failed to shave yak");
            });
        });
    });

    counters.print_counters();
}
