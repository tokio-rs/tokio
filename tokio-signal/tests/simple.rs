#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn simple() {
    let mut lp = Core::new().unwrap();

    let signal = run_core_with_timeout(&mut lp, Signal::new(libc::SIGUSR1))
        .expect("failed to create signal");

    send_signal(libc::SIGUSR1);

    run_core_with_timeout(&mut lp, signal.into_future())
        .ok()
        .expect("failed to get signal");
}
