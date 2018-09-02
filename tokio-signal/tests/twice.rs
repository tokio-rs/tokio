#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn twice() {
    let signal = run_with_timeout_default(Signal::new(libc::SIGUSR1)).unwrap();

    send_signal(libc::SIGUSR1);
    let (num, signal) = run_with_timeout_default(signal.into_future()).ok().unwrap();
    assert_eq!(num, Some(libc::SIGUSR1));

    send_signal(libc::SIGUSR1);
    run_with_timeout_default(signal.into_future()).ok().unwrap();
}
