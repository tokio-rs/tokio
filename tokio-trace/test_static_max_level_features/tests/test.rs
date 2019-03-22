#[macro_use]
extern crate tokio_trace;

use std::sync::{Arc, Mutex};
use tokio_trace::span::{Attributes, Record};
use tokio_trace::{span, Event, Id, Level, Metadata, Subscriber};

struct State {
    last_level: Mutex<Option<Level>>,
}

struct TestSubscriber(Arc<State>);

impl Subscriber for TestSubscriber {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes) -> Id {
        span::Id::from_u64(42)
    }

    fn record(&self, _span: &Id, _values: &Record) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event) {
        *self.0.last_level.lock().unwrap() = Some(event.metadata().level().clone());
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

#[cfg(test)]
fn test_static_max_level_features() {
    let me = Arc::new(State {
        last_level: Mutex::new(None),
    });
    let a = me.clone();
    tokio_trace::subscriber::with_default(TestSubscriber(me), || {
        error!("");
        last(&a, Some(Level::ERROR));
        warn!("");
        last(&a, Some(Level::WARN));
        info!("");
        last(&a, Some(Level::INFO));
        debug!("");
        last(&a, Some(Level::DEBUG));
        trace!("");
        last(&a, None);

        span!(level: Level::ERROR, "");
        last(&a, None);
        span!(level: Level::WARN, "");
        last(&a, None);
        span!(level: Level::INFO, "");
        last(&a, None);
        span!(level: Level::DEBUG, "");
        last(&a, None);
        span!(level: Level::TRACE, "");
        last(&a, None);
    });
}

fn last(state: &State, expected: Option<Level>) {
    let mut lvl = state.last_level.lock().unwrap();
    assert_eq!(*lvl, expected);
    *lvl = None;
}
