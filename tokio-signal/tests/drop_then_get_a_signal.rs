#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use libc;

pub mod support;
use crate::support::*;

#[tokio::test]
async fn drop_then_get_a_signal() {
    let signal = with_timeout(Signal::new(libc::SIGUSR1))
        .await
        .expect("failed to create first signal");
    drop(signal);

    send_signal(libc::SIGUSR1);
    let signal = with_timeout(Signal::new(libc::SIGUSR1))
        .await
        .expect("failed to create second signal");

    let _ = with_timeout(signal.into_future()).await;
}
