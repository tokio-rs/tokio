#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]
#![feature(async_await)]

pub mod signal_support;
use crate::signal_support::*;

#[tokio::test]
async fn drop_then_get_a_signal() {
    let kind = SignalKind::user_defined1();
    let signal = Signal::new(kind).expect("failed to create first signal");
    drop(signal);

    send_signal(libc::SIGUSR1);
    let signal = Signal::new(kind).expect("failed to create second signal");

    let _ = with_timeout(signal.into_future()).await;
}
