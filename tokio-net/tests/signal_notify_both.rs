#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]
#![feature(async_await)]

pub mod signal_support;
use crate::signal_support::*;

#[tokio::test]
async fn notify_both() {
    let kind = SignalKind::user_defined2();
    let signal1 = Signal::new(kind).expect("failed to create signal1");

    let signal2 = Signal::new(kind).expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    let _ = with_timeout(future::join(signal1.into_future(), signal2.into_future())).await;
}
