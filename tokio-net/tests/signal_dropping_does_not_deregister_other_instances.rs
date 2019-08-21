#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]

pub mod support;
use support::*;

#[tokio::test]
async fn dropping_signal_does_not_deregister_any_other_instances() {
    let kind = SignalKind::user_defined1();

    // NB: Testing for issue alexcrichton/tokio-signal#38:
    // signals should not starve based on ordering
    let first_duplicate_signal = signal(kind).expect("failed to register first duplicate signal");
    let sig = signal(kind).expect("failed to register signal");
    let second_duplicate_signal = signal(kind).expect("failed to register second duplicate signal");

    drop(first_duplicate_signal);
    drop(second_duplicate_signal);

    send_signal(libc::SIGUSR1);
    let _ = with_timeout(sig.into_future()).await;
}
