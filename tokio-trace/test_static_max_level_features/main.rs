#[macro_use]
extern crate log;

use std::sync::{Arc, Mutex};
use log::{Level, LevelFilter, Log, Record, Metadata};

#[cfg(feature = "std")]
use log::set_boxed_logger;
#[cfg(not(feature = "std"))]
fn set_boxed_logger(logger: Box<Log>) -> Result<(), log::SetLoggerError> {
    unsafe {
        log::set_logger(&*Box::into_raw(logger))
    }
}

struct State {
    last_log: Mutex<Option<Level>>,
}

struct Logger(Arc<State>);

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        *self.0.last_log.lock().unwrap() = Some(record.level());
    }

    fn flush(&self) {}
}

fn main() {
    let me = Arc::new(State { last_log: Mutex::new(None) });
    let a = me.clone();
    set_boxed_logger(Box::new(Logger(me))).unwrap();

    test(&a, LevelFilter::Off);
    test(&a, LevelFilter::Error);
    test(&a, LevelFilter::Warn);
    test(&a, LevelFilter::Info);
    test(&a, LevelFilter::Debug);
    test(&a, LevelFilter::Trace);
}

fn test(a: &State, filter: LevelFilter) {
    log::set_max_level(filter);
    error!("");
    last(&a, t(Level::Error, filter));
    warn!("");
    last(&a, t(Level::Warn, filter));
    info!("");
    last(&a, t(Level::Info, filter));

    debug!("");
    if cfg!(debug_assertions) {
        last(&a, t(Level::Debug, filter));
    } else {
        last(&a, None);
    }

    trace!("");
    last(&a, None);

    fn t(lvl: Level, filter: LevelFilter) -> Option<Level> {
        if lvl <= filter {Some(lvl)} else {None}
    }
}

fn last(state: &State, expected: Option<Level>) {
    let mut lvl = state.last_log.lock().unwrap();
    assert_eq!(*lvl, expected);
    *lvl = None;
}