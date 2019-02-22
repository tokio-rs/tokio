#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

const TEST_SIGNAL: libc::c_int = libc::SIGUSR1;

#[test]
fn dropping_loops_does_not_cause_starvation() {
    let (mut rt, signal) = {
        let mut first_rt = CurrentThreadRuntime::new().expect("failed to init first runtime");

        let first_signal = run_with_timeout(&mut first_rt, Signal::new(TEST_SIGNAL))
            .expect("failed to register first signal");

        let mut second_rt = CurrentThreadRuntime::new().expect("failed to init second runtime");

        let second_signal = run_with_timeout(&mut second_rt, Signal::new(TEST_SIGNAL))
            .expect("failed to register second signal");

        drop(first_rt);
        drop(first_signal);

        (second_rt, second_signal)
    };

    send_signal(TEST_SIGNAL);

    let signal_future = signal.into_future().map_err(|(e, _)| e);

    run_with_timeout(&mut rt, signal_future).expect("failed to get signal");
}
