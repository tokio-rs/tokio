#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]

pub mod support;
use crate::support::*;

use libc;

#[test]
fn tokio_simple() {
    let signal_future = Signal::new(libc::SIGUSR1).and_then(|signal| {
        send_signal(libc::SIGUSR1);
        signal.into_future().map(|_| ()).map_err(|(err, _)| err)
    });

    let mut rt = CurrentThreadRuntime::new().expect("failed to init runtime");
    run_with_timeout(&mut rt, signal_future).expect("failed");
}
