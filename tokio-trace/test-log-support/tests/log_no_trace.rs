extern crate log;
#[macro_use]
extern crate tokio_trace;

use log::{LevelFilter, Log, Metadata, Record};
use std::sync::{Arc, Mutex};
use tokio_trace::Level;

struct State {
    last_log: Mutex<Option<String>>,
}

struct Logger(Arc<State>);

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let line = format!("{}", record.args());
        println!("{:<5} {} {}", record.level(), record.target(), line);
        *self.0.last_log.lock().unwrap() = Some(line);
    }

    fn flush(&self) {}
}

#[test]
fn test_always_log() {
    let me = Arc::new(State {
        last_log: Mutex::new(None),
    });
    let a = me.clone();
    log::set_boxed_logger(Box::new(Logger(me))).unwrap();
    log::set_max_level(LevelFilter::Trace);

    error!(foo = 5);
    last(&a, "foo=5");
    warn!("hello {};", "world");
    last(&a, "hello world;");
    info!(message = "hello world;", thingy = 42, other_thingy = 666);
    last(&a, "hello world; thingy=42 other_thingy=666");

    let foo = span!(Level::TRACE, "foo");
    last(&a, "foo;");
    foo.in_scope(|| {
        last(&a, "-> foo");

        trace!({foo = 3, bar = 4}, "hello {};", "san francisco");
        last(&a, "hello san francisco; foo=3 bar=4");
    });
    last(&a, "<- foo");

    span!(Level::TRACE, "foo", bar = 3, baz = false);
    last(&a, "foo; bar=3 baz=false");

    // TODO(#1138): determine a new syntax for uninitialized span fields, and
    // re-enable these.
    // let span = span!(Level::TRACE, "foo", bar = _, baz = _);
    // span.record("bar", &3);
    // last(&a, "foo; bar=3");
    // span.record("baz", &"a string");
    // last(&a, "foo; baz=\"a string\"");
}

fn last(state: &State, expected: &str) {
    let mut lock = state.last_log.lock().unwrap();
    {
        let last = lock.as_ref().map(|s| s.as_str().trim());
        assert_eq!(last, Some(expected));
    }
    *lock = None;
}
