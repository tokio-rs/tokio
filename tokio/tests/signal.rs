#![cfg(unix)]
#![warn(rust_2018_idioms)]

use tokio::prelude::*;
use tokio::runtime::current_thread::Runtime;
use tokio::signal;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::oneshot;
use tokio_test::assert_ok;

use futures::future;
use std::sync::mpsc::channel;
use std::thread;

#[tokio::test]
async fn signal_usr1() {
    let signal = assert_ok!(signal(SignalKind::user_defined1()), "failed to create signal");

    send_signal(libc::SIGUSR1);

    let _ = signal.into_future().await;
}

#[tokio::test]
async fn ctrl_c() {
    let ctrl_c = signal::ctrl_c().expect("failed to init ctrl_c");

    let (fire, wait) = oneshot::channel();

    // NB: simulate a signal coming in by exercising our signal handler
    // to avoid complications with sending SIGINT to the test process
    tokio::spawn(async {
        wait.await.expect("wait failed");
        send_signal(libc::SIGINT);
    });

    let _ = fire.send(());
    let _ = ctrl_c.into_future().await;
}

#[tokio::test]
async fn notify_both() {
    let kind = SignalKind::user_defined2();
    let signal1 = signal(kind).expect("failed to create signal1");

    let signal2 = signal(kind).expect("failed to create signal2");

    send_signal(libc::SIGUSR2);
    let _ = future::join(signal1.into_future(), signal2.into_future()).await;
}

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

#[test]
fn dropping_loops_does_not_cause_starvation() {
    let kind = SignalKind::user_defined1();

    let mut first_rt = Runtime::new().expect("failed to init first runtime");
    let mut first_signal =
        first_rt.block_on(async { signal(kind).expect("failed to register first signal") });

    let mut second_rt = Runtime::new().expect("failed to init second runtime");
    let mut second_signal =
        second_rt.block_on(async { signal(kind).expect("failed to register second signal") });

    send_signal(libc::SIGUSR1);

    first_rt.block_on(first_signal.next()).expect("failed to await first signal");

    drop(first_rt);
    drop(first_signal);

    send_signal(libc::SIGUSR1);

    second_rt.block_on(second_signal.next());
}

#[tokio::test]
async fn drop_then_get_a_signal() {
    let kind = SignalKind::user_defined1();
    let sig = signal(kind).expect("failed to create first signal");
    drop(sig);

    send_signal(libc::SIGUSR1);
    let sig = signal(kind).expect("failed to create second signal");

    let _ = sig.into_future().await;
}

#[tokio::test]
async fn dropping_signal_does_not_deregister_any_other_instances() {
    let kind = SignalKind::user_defined1();

    // Signals should not starve based on ordering
    let first_duplicate_signal = signal(kind).expect("failed to register first duplicate signal");
    let sig = signal(kind).expect("failed to register signal");
    let second_duplicate_signal = signal(kind).expect("failed to register second duplicate signal");

    drop(first_duplicate_signal);
    drop(second_duplicate_signal);

    send_signal(libc::SIGUSR1);
    let _ = sig.into_future().await;
}

#[test]
fn multi_loop() {
    // An "ordinary" (non-future) channel
    let (sender, receiver) = channel();
    // Run multiple times, to make sure there are no race conditions
    for _ in 0..10 {
        // Run multiple event loops, each one in its own thread
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let sender = sender.clone();
                thread::spawn(move || {
                    let mut rt = Runtime::new().unwrap();
                    let _ = rt.block_on(async {
                        let signal = signal(SignalKind::hangup()).unwrap();
                        sender.send(()).unwrap();
                        signal.into_future().await
                    });
                })
            })
            .collect();
        // Wait for them to declare they're ready
        for &_ in threads.iter() {
            receiver.recv().unwrap();
        }
        // Send a signal
        send_signal(libc::SIGHUP);
        // Make sure the threads terminated correctly
        for t in threads {
            t.join().unwrap();
        }
    }
}

#[test]
#[should_panic]
fn no_runtime_panics_creating_signals() {
    let _ = signal(SignalKind::hangup());
}

fn send_signal(signal: libc::c_int) {
    use libc::{getpid, kill};

    unsafe {
        assert_eq!(kill(getpid(), signal), 0);
    }
}
