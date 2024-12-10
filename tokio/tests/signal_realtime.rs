#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(any(target_os = "linux", target_os = "illumos"))]
#![cfg(not(miri))] // No `sigaction` in Miri.

mod support {
    pub mod signal;
}

use support::signal::send_signal;

use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{sleep, Duration};
use tokio_test::assert_ok;

#[tokio::test]
async fn signal_realtime() {
    // Attempt to register a real-time signal for everything between SIGRTMIN
    // and SIGRTMAX.
    let signals = (libc::SIGRTMIN()..=libc::SIGRTMAX())
        .map(|signum| {
            let sig = assert_ok!(
                signal(SignalKind::from_raw(signum)),
                "failed to create signal for SIGRTMIN+{} (signal {})",
                signum - libc::SIGRTMIN(),
                signum
            );
            (signum, sig)
        })
        .collect::<Vec<_>>();

    eprintln!(
        "registered {} signals in the range {}..={}",
        signals.len(),
        libc::SIGRTMIN(),
        libc::SIGRTMAX()
    );

    // Now send signals to each of the registered signals.
    for signum in libc::SIGRTMIN()..=libc::SIGRTMAX() {
        send_signal(signum);
    }

    let futures = signals
        .into_iter()
        .map(|(signum, mut sig)| async move {
            let res = sig.recv().await;
            (signum, res)
        })
        .collect::<FuturesUnordered<_>>();

    // Ensure that all signals are received in time -- attempt to get whatever
    // we can.
    let sleep = std::pin::pin!(sleep(Duration::from_secs(5)));
    let done = futures.take_until(sleep).collect::<HashMap<_, _>>().await;

    let mut none = Vec::new();
    let mut missing = Vec::new();
    for signum in libc::SIGRTMIN()..=libc::SIGRTMAX() {
        match done.get(&signum) {
            Some(Some(())) => {}
            Some(None) => none.push(signum),
            None => missing.push(signum),
        }
    }

    if none.is_empty() && missing.is_empty() {
        return;
    }

    let mut msg = String::new();
    if !none.is_empty() {
        msg.push_str("no signals received for:\n");
        for signum in none {
            msg.push_str(&format!(
                "- SIGRTMIN+{} (signal {})\n",
                signum - libc::SIGRTMIN(),
                signum
            ));
        }
    }

    if !missing.is_empty() {
        msg.push_str("missing signals for:\n");
        for signum in missing {
            msg.push_str(&format!(
                "- SIGRTMIN+{} (signal {})\n",
                signum - libc::SIGRTMIN(),
                signum
            ));
        }
    }

    panic!("{}", msg);
}
