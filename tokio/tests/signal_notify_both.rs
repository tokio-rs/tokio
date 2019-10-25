#![cfg(unix)]
#![warn(rust_2018_idioms)]

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use tokio::prelude::*;
use tokio::signal::unix::{signal, SignalKind};

use futures::future;

#[tokio::test]
async fn notify_both() {
    let kind = SignalKind::user_defined2();
    let signal1 = signal(kind).expect("failed to create signal1");

    let signal2 = signal(kind).expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    let _ = future::join(signal1.into_future(), signal2.into_future()).await;
}
