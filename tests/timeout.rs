extern crate env_logger;
extern crate futures;
extern crate futures_mio;

use std::time::{Instant, Duration};

use futures::Future;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn smoke() {
    drop(env_logger::init());
    let mut l = t!(futures_mio::Loop::new());
    let dur = Duration::from_millis(10);
    let timeout = l.handle().timeout(dur).and_then(|t| t);
    let start = Instant::now();
    t!(l.run(timeout));
    assert!(start.elapsed() >= dur);
}
