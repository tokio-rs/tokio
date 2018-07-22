#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_signal;

use futures::stream::Stream;
use futures::Future;
use tokio_signal::unix::Signal;
use std::time::{Duration, Instant};

const TEST_SIGNAL: libc::c_int = libc::SIGUSR1;

#[test]
fn dropping_signal_does_not_deregister_any_other_instances() {
    // NB: Deadline requires a timer registration which is provided by
    // tokio's `current_thread::Runtime`, but isn't available by just using
    // tokio's default CurrentThread executor which powers `current_thread::block_on_all`.
    let mut rt = tokio::runtime::current_thread::Runtime::new()
        .expect("failed to init runtime");

    // NB: Testing for issue #38: signals should not starve based on ordering
    let first_duplicate_signal = rt.block_on(Signal::new(TEST_SIGNAL))
        .expect("failed to register first duplicate signal");
    let signal = rt.block_on(Signal::new(TEST_SIGNAL))
        .expect("failed to register signal");
    let second_duplicate_signal = rt.block_on(Signal::new(TEST_SIGNAL))
        .expect("failed to register second duplicate signal");

    drop(first_duplicate_signal);
    drop(second_duplicate_signal);

    unsafe { assert_eq!(libc::kill(libc::getpid(), TEST_SIGNAL), 0); }

    let signal_future = signal.into_future()
        .map_err(|(e, _)| e);

    let deadline_time = Instant::now() + Duration::from_secs(1);
    rt.block_on(tokio::timer::Deadline::new(signal_future, deadline_time))
        .expect("failed to get signal");
}
