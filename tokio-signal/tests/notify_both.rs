#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn notify_both() {
    let signal1 = run_with_timeout_default(Signal::new(libc::SIGUSR2))
        .expect("failed to create signal1");

    let signal2 = run_with_timeout_default(Signal::new(libc::SIGUSR2))
        .expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    run_with_timeout_default(signal1.into_future().join(signal2.into_future()))
        .ok()
        .expect("failed to receive");
}
