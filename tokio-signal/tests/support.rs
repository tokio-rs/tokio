#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]

use libc::{c_int, getpid, kill};
use std::time::Duration;
use tokio::timer::Timeout;

pub use futures::{Future, Stream};
pub use tokio::runtime::current_thread::{self, Runtime as CurrentThreadRuntime};
pub use tokio_signal::unix::Signal;

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
