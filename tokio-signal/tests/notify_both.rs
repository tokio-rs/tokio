#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]

pub mod support;
use crate::support::*;

use libc;

#[test]
fn notify_both() {
    let mut rt = CurrentThreadRuntime::new().unwrap();
    let signal1 =
        run_with_timeout(&mut rt, Signal::new(libc::SIGUSR2)).expect("failed to create signal1");

    let signal2 =
        run_with_timeout(&mut rt, Signal::new(libc::SIGUSR2)).expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    run_with_timeout(&mut rt, signal1.into_future().join(signal2.into_future()))
        .ok()
        .expect("failed to receive");
}
