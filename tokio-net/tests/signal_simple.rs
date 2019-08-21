#![cfg(unix)]
#![cfg(feature = "signal")]
#![warn(rust_2018_idioms)]

mod support;
use support::*;

#[tokio::test]
async fn simple() {
    let signal = signal(SignalKind::user_defined1()).expect("failed to create signal");

    send_signal(libc::SIGUSR1);

    let _ = with_timeout(signal.into_future()).await;
}

#[tokio::test]
#[cfg(unix)]
async fn ctrl_c() {
    use tokio::sync::oneshot;
    use tokio_net::signal::ctrl_c;

    let ctrl_c = ctrl_c().expect("failed to init ctrl_c");

    let (fire, wait) = oneshot::channel();

    // NB: simulate a signal coming in by exercising our signal handler
    // to avoid complications with sending SIGINT to the test process
    tokio::spawn(async {
        wait.await.expect("wait failed");
        send_signal(libc::SIGINT);
    });

    let _ = fire.send(());
    let _ = with_timeout(ctrl_c.into_future()).await;
}
