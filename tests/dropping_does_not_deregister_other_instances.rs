#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio;
extern crate tokio_signal;

use futures::stream::Stream;
use futures::Future;
use tokio_signal::unix::Signal;
use std::time::{Duration, Instant};

#[test]
fn dropping_signal_does_not_deregister_any_other_instances() {
    // NB: Deadline requires a timer registration which is provided by
    // tokio's `current_thread::Runtime`, but isn't available by just using
    // tokio's default CurrentThread executor which powers `current_thread::block_on_all`.
    let mut rt = tokio::runtime::current_thread::Runtime::new()
        .expect("failed to init runtime");

    // FIXME(38): there appears to be a bug with the order these are created/destroyed
    // If the duplicate signal is created first and dropped, it appears that `signal`
    // will starve. This ordering also appears to be OS specific...
    let first_signal = rt.block_on(Signal::new(libc::SIGUSR1))
        .expect("failed to register duplicate signal");
    let second_signal = rt.block_on(Signal::new(libc::SIGUSR1))
        .expect("failed to register signal");
    let (duplicate, signal) = if cfg!(target_os = "linux") {
        (second_signal, first_signal)
    } else {
        // macOS
        (first_signal, second_signal)
    };

    drop(duplicate);

    unsafe { assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0); }

    let signal_future = signal.into_future()
        .map_err(|(e, _)| e);

    let deadline_time = Instant::now() + Duration::from_secs(1);
    rt.block_on(tokio::timer::Deadline::new(signal_future, deadline_time))
        .expect("failed to get signal");
}
