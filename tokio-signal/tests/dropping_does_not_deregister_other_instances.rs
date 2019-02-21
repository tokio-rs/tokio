#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

const TEST_SIGNAL: libc::c_int = libc::SIGUSR1;

#[test]
fn dropping_signal_does_not_deregister_any_other_instances() {
    // NB: Deadline requires a timer registration which is provided by
    // tokio's `current_thread::Runtime`, but isn't available by just using
    // tokio's default CurrentThread executor which powers `current_thread::block_on_all`.
    let mut rt = CurrentThreadRuntime::new().expect("failed to init runtime");

    // NB: Testing for issue #38: signals should not starve based on ordering
    let first_duplicate_signal = run_with_timeout(&mut rt, Signal::new(TEST_SIGNAL))
        .expect("failed to register first duplicate signal");
    let signal =
        run_with_timeout(&mut rt, Signal::new(TEST_SIGNAL)).expect("failed to register signal");
    let second_duplicate_signal = run_with_timeout(&mut rt, Signal::new(TEST_SIGNAL))
        .expect("failed to register second duplicate signal");

    drop(first_duplicate_signal);
    drop(second_duplicate_signal);

    send_signal(TEST_SIGNAL);
    run_with_timeout(&mut rt, signal.into_future().map_err(|(e, _)| e))
        .expect("failed to get signal");
}
