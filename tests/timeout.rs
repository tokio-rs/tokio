extern crate env_logger;
extern crate futures;
extern crate tokio_core;

use std::time::{Instant, Duration};

use futures::Future;
use tokio_core::reactor::{Core, Timeout};

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn smoke() {
    drop(env_logger::init());
    let mut l = t!(Core::new());
    let dur = Duration::from_millis(10);
    let timeout = Timeout::new(dur, &l.handle()).and_then(|t| t);
    let start = Instant::now();
    t!(l.run(timeout));
    assert!(start.elapsed() >= dur);
}
