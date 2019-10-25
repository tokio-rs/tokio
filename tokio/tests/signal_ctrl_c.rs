#![cfg(unix)]
#![warn(rust_2018_idioms)]

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use tokio::prelude::*;
use tokio::signal;
use tokio::sync::oneshot;

#[tokio::test]
async fn ctrl_c() {
    let ctrl_c = signal::ctrl_c().expect("failed to init ctrl_c");

    let (fire, wait) = oneshot::channel();

    // NB: simulate a signal coming in by exercising our signal handler
    // to avoid complications with sending SIGINT to the test process
    tokio::spawn(async {
        wait.await.expect("wait failed");
        send_signal(libc::SIGINT);
    });

    let _ = fire.send(());
    let _ = ctrl_c.into_future().await;
}
