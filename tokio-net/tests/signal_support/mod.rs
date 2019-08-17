#![cfg(unix)]
#![warn(rust_2018_idioms)]

pub use tokio::runtime::current_thread::{self, Runtime as CurrentThreadRuntime};
use tokio::timer::Timeout;
pub use tokio_net::signal::unix::{Signal, SignalKind};

pub use futures_util::future;
use futures_util::future::FutureExt;
pub use futures_util::stream::StreamExt;
use libc::{c_int, getpid, kill};
use std::future::Future;
use std::time::Duration;

pub fn with_timeout<F: Future>(future: F) -> impl Future<Output = F::Output> {
    Timeout::new(future, Duration::from_secs(1)).map(Result::unwrap)
}

pub fn run_with_timeout<F>(rt: &mut CurrentThreadRuntime, future: F) -> F::Output
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
