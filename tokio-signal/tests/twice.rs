#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn twice() {
    let mut rt = CurrentThreadRuntime::new().unwrap();
    let signal = run_with_timeout(&mut rt, Signal::new(libc::SIGUSR1)).unwrap();

    send_signal(libc::SIGUSR1);
    let (num, signal) = run_with_timeout(&mut rt, signal.into_future())
        .ok()
        .unwrap();
    assert_eq!(num, Some(libc::SIGUSR1));

    send_signal(libc::SIGUSR1);
    run_with_timeout(&mut rt, signal.into_future())
        .ok()
        .unwrap();
}
