#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn simple() {
    let mut rt = CurrentThreadRuntime::new().unwrap();
    let signal = run_with_timeout(&mut rt, Signal::new(libc::SIGUSR1))
        .expect("failed to create signal");

    send_signal(libc::SIGUSR1);

    run_with_timeout(&mut rt, signal.into_future())
        .ok()
        .expect("failed to get signal");
}
