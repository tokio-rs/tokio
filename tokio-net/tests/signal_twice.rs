#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]
#![feature(async_await)]

#[path = "support/signal.rs"]
pub mod support;
use crate::support::*;

#[tokio::test]
async fn twice() {
    let kind = SignalKind::user_defined1();
    let mut signal = Signal::new(kind).expect("failed to get signal");

    for _ in 0..2 {
        send_signal(libc::SIGUSR1);

        let (item, sig) = with_timeout(signal.into_future()).await;
        assert_eq!(item, Some(()));

        signal = sig;
    }
}
