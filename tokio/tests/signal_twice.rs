#![cfg(unix)]
#![warn(rust_2018_idioms)]

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use tokio::prelude::*;
use tokio::signal::unix::{signal, SignalKind};

#[tokio::test]
async fn twice() {
    let kind = SignalKind::user_defined1();
    let mut sig = signal(kind).expect("failed to get signal");

    for _ in 0..2 {
        send_signal(libc::SIGUSR1);

        let (item, sig_next) = sig.into_future().await;
        assert_eq!(item, Some(()));

        sig = sig_next;
    }
}
