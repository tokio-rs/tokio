extern crate env_logger;
extern crate futures;
extern crate tokio_core;

use std::time::{Instant, Duration};

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
    let timeout = t!(Timeout::new(dur, &l.handle()));
    let start = Instant::now();
    t!(l.run(timeout));
    assert!(start.elapsed() >= dur);
}
