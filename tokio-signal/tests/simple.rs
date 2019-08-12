#![cfg(unix)]
#![warn(rust_2018_idioms)]
#![feature(async_await)]

pub mod support;
use crate::support::*;

use libc;

#[tokio::test]
async fn simple() {
    let signal = Signal::new(SignalKind::sigusr1()).expect("failed to create signal");

    send_signal(libc::SIGUSR1);

    let _ = with_timeout(signal.into_future()).await;
}
