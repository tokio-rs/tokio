#![cfg(unix)]
#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

pub mod support;
use crate::support::*;

use libc;

#[tokio::test]
async fn twice() {
    let mut signal = Signal::new(libc::SIGUSR1).expect("failed to get signal");

    for _ in 0..2 {
        send_signal(libc::SIGUSR1);

        let (item, sig) = with_timeout(signal.into_future()).await;
        assert_eq!(item, Some(()));

        signal = sig;
    }
}
