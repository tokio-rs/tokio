#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn drop_then_get_a_signal() {
    let signal = run_with_timeout_default(Signal::new(libc::SIGUSR1))
        .expect("failed to create first signal");
    drop(signal);

    send_signal(libc::SIGUSR1);
    let signal = run_with_timeout_default(Signal::new(libc::SIGUSR1))
        .expect("failed to create signal")
        .into_future()
        .map(|_| ())
        .map_err(|(e, _)| panic!("{}", e));

    run_with_timeout_default(signal).expect("failed to get signal");
}
