#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn notify_both() {
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let signal1 = run_core_with_timeout(&mut lp, Signal::with_handle(
        libc::SIGUSR2,
        &handle.new_tokio_handle(),
    )).expect("failed to create signal1");

    let signal2 = run_core_with_timeout(&mut lp, Signal::with_handle(
        libc::SIGUSR2,
        &handle.new_tokio_handle(),
    )).expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    run_core_with_timeout(&mut lp, signal1.into_future().join(signal2.into_future()))
        .ok()
        .expect("failed to create signal2");
}
