extern crate env_logger;
extern crate futures;
extern crate tokio_core;

use std::time::{Instant, Duration};

use futures::stream::{Stream};
use tokio_core::reactor::{Core, Interval};

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn single() {
    drop(env_logger::init());
    let mut l = t!(Core::new());
    let dur = Duration::from_millis(10);
    let interval = t!(Interval::new(dur, &l.handle()));
    let start = Instant::now();
    t!(l.run(interval.take(1).collect()));
    assert!(start.elapsed() >= dur);
}

#[test]
fn two_times() {
    drop(env_logger::init());
    let mut l = t!(Core::new());
    let dur = Duration::from_millis(10);
    let interval = t!(Interval::new(dur, &l.handle()));
    let start = Instant::now();
    let result = t!(l.run(interval.take(2).collect()));
    assert!(start.elapsed() >= dur*2);
    assert_eq!(result, vec![(), ()]);
}
