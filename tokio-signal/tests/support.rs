#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_signal;

use self::libc::{c_int, getpid, kill};
use self::tokio::timer::Timeout;
use std::time::Duration;

pub use self::futures::{Future, Stream};
pub use self::tokio::runtime::current_thread::{self, Runtime as CurrentThreadRuntime};
pub use self::tokio_signal::unix::Signal;

pub fn with_timeout<F: Future>(future: F) -> impl Future<Item = F::Item, Error = F::Error> {
    Timeout::new(future, Duration::from_secs(1)).map_err(|e| {
        if e.is_timer() {
            panic!("failed to register timer");
        } else if e.is_elapsed() {
            panic!("timed out")
        } else {
            e.into_inner().expect("missing inner error")
        }
    })
}

pub fn run_with_timeout<F>(rt: &mut CurrentThreadRuntime, future: F) -> Result<F::Item, F::Error>
where
    F: Future,
{
    rt.block_on(with_timeout(future))
}

#[cfg(unix)]
pub fn send_signal(signal: c_int) {
    unsafe {
        assert_eq!(kill(getpid(), signal), 0);
    }
}
