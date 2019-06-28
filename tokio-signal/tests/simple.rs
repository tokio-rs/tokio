#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

pub mod support;
use crate::support::*;

use libc;

#[tokio::test]
async fn simple() {
    let signal = with_timeout(Signal::new(libc::SIGUSR1))
        .await
        .expect("failed to create signal");

    send_signal(libc::SIGUSR1);

    with_timeout(signal.into_future()).await;
}
