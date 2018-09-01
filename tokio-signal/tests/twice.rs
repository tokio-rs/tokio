#![cfg(unix)]

extern crate libc;

pub mod support;
use support::*;

#[test]
fn twice() {
    let mut lp = Core::new().unwrap();
    let signal = run_core_with_timeout(&mut lp, Signal::new(libc::SIGUSR1)).unwrap();

    send_signal(libc::SIGUSR1);
    let (num, signal) = run_core_with_timeout(&mut lp, signal.into_future()).ok().unwrap();
    assert_eq!(num, Some(libc::SIGUSR1));

    send_signal(libc::SIGUSR1);
    run_core_with_timeout(&mut lp, signal.into_future()).ok().unwrap();
}
