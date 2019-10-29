#![cfg(unix)]
#![warn(rust_2018_idioms)]

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use tokio::prelude::*;
use tokio::signal::unix::{signal, SignalKind};

#[tokio::test]
async fn dropping_signal_does_not_deregister_any_other_instances() {
    let kind = SignalKind::user_defined1();

    // Signals should not starve based on ordering
    let first_duplicate_signal = signal(kind).expect("failed to register first duplicate signal");
    let sig = signal(kind).expect("failed to register signal");
    let second_duplicate_signal = signal(kind).expect("failed to register second duplicate signal");

    drop(first_duplicate_signal);
    drop(second_duplicate_signal);

    send_signal(libc::SIGUSR1);
    let _ = sig.into_future().await;
}
