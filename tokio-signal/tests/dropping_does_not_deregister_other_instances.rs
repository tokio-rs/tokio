#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use libc;

pub mod support;
use crate::support::*;

const TEST_SIGNAL: libc::c_int = libc::SIGUSR1;

#[tokio::test]
async fn dropping_signal_does_not_deregister_any_other_instances() {
    // NB: Testing for issue alexcrichton/tokio-signal#38:
    // signals should not starve based on ordering
    let first_duplicate_signal =
        Signal::new(TEST_SIGNAL).expect("failed to register first duplicate signal");
    let signal = Signal::new(TEST_SIGNAL).expect("failed to register signal");
    let second_duplicate_signal =
        Signal::new(TEST_SIGNAL).expect("failed to register second duplicate signal");

    drop(first_duplicate_signal);
    drop(second_duplicate_signal);

    send_signal(TEST_SIGNAL);
    let _ = with_timeout(signal.into_future()).await;
}
