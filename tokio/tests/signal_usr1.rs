#![cfg(unix)]
#![warn(rust_2018_idioms)]

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use tokio::prelude::*;
use tokio::signal::unix::{signal, SignalKind};
use tokio_test::assert_ok;

#[tokio::test]
async fn signal_usr1() {
    let signal = assert_ok!(
        signal(SignalKind::user_defined1()),
        "failed to create signal"
    );

    send_signal(libc::SIGUSR1);

    let _ = signal.into_future().await;
}
