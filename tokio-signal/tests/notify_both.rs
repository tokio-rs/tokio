#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

pub mod support;
use crate::support::*;

use libc;

#[tokio::test]
async fn notify_both() {
    let signal1 = with_timeout(Signal::new(libc::SIGUSR2))
        .await
        .expect("failed to create signal1");

    let signal2 = with_timeout(Signal::new(libc::SIGUSR2))
        .await
        .expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    with_timeout(future::join(signal1.into_future(), signal2.into_future())).await;
}
