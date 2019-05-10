#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]

pub mod support;
use crate::support::*;

use libc;

#[test]
fn simple() {
    let mut rt = CurrentThreadRuntime::new().unwrap();
    let signal =
        run_with_timeout(&mut rt, Signal::new(libc::SIGUSR1)).expect("failed to create signal");

    send_signal(libc::SIGUSR1);

    run_with_timeout(&mut rt, signal.into_future())
        .ok()
        .expect("failed to get signal");
}
