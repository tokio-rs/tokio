extern crate env_logger;
extern crate futures;
extern crate tokio;

use std::time::{Instant, Duration};

use futures::stream::{Stream};
use tokio::reactor::{Core, Interval};

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
    let start = Instant::now();
    let interval = t!(Interval::new(dur, &l.handle()));
    t!(l.run(interval.take(1).collect()));
    assert!(start.elapsed() >= dur);
}

#[test]
fn two_times() {
    drop(env_logger::init());
    let mut l = t!(Core::new());
    let dur = Duration::from_millis(10);
    let start = Instant::now();
    let interval = t!(Interval::new(dur, &l.handle()));
    let result = t!(l.run(interval.take(2).collect()));
    assert!(start.elapsed() >= dur*2);
    assert_eq!(result, vec![(), ()]);
}
