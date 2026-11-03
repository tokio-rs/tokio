#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(unix)]
#![cfg(not(miri))] // No `sigaction` in miri.

mod support {
    pub mod signal;
}
use support::signal::send_signal;

use tokio::runtime::Runtime;
use tokio::signal::unix::{signal, SignalKind};

#[test]
fn dropping_loops_does_not_cause_starvation() {
    let kind = SignalKind::user_defined1();

    let first_rt = rt();
    let mut first_signal =
        first_rt.block_on(async { signal(kind).expect("failed to register first signal") });

    let second_rt = rt();
    let mut second_signal =
        second_rt.block_on(async { signal(kind).expect("failed to register second signal") });

    send_signal(libc::SIGUSR1);

    first_rt
        .block_on(first_signal.recv())
        .expect("failed to await first signal");

    drop(first_rt);
    drop(first_signal);

    send_signal(libc::SIGUSR1);

    second_rt.block_on(second_signal.recv());
}

#[test]
fn signal_errors_after_drop_current_thread() {
    signal_errors_after_drop(rt());
}

#[test]
fn signal_errors_after_drop_multi_thread() {
    signal_errors_after_drop(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    );
}

fn signal_errors_after_drop(rt: Runtime) {
    let handle = rt.handle().clone();
    drop(rt);

    let _enter = handle.enter();
    assert!(signal(SignalKind::user_defined1()).is_err());
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
