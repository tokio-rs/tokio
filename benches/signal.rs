//! Benchmark the delay in propagating OS signals to any listeners.
#![cfg(unix)]

use bencher::{benchmark_group, benchmark_main, Bencher};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::runtime;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;

struct Spinner {
    count: usize,
}

impl Future for Spinner {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.count > 3 {
            Poll::Ready(())
        } else {
            self.count += 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

impl Spinner {
    fn new() -> Self {
        Self { count: 0 }
    }
}

pub fn send_signal(signal: libc::c_int) {
    use libc::{getpid, kill};

    unsafe {
        assert_eq!(kill(getpid(), signal), 0);
    }
}

fn many_signals(bench: &mut Bencher) {
    let num_signals = 10;
    let (tx, mut rx) = mpsc::channel(num_signals);

    let rt = runtime::Builder::new()
        // Intentionally single threaded to measure delays in propagating wakes
        .basic_scheduler()
        .enable_all()
        .build()
        .unwrap();

    let spawn_signal = |kind| {
        let mut tx = tx.clone();
        rt.spawn(async move {
            let mut signal = signal(kind).expect("failed to create signal");

            while signal.recv().await.is_some() {
                if tx.send(()).await.is_err() {
                    break;
                }
            }
        });
    };

    for _ in 0..num_signals {
        // Pick some random signals which don't terminate the test harness
        spawn_signal(SignalKind::child());
        spawn_signal(SignalKind::io());
    }
    drop(tx);

    // Turn the runtime for a while to ensure that all the spawned
    // tasks have been polled at least once
    rt.block_on(Spinner::new());

    bench.iter(|| {
        rt.block_on(async {
            send_signal(libc::SIGCHLD);
            for _ in 0..num_signals {
                rx.recv().await.expect("channel closed");
            }

            send_signal(libc::SIGIO);
            for _ in 0..num_signals {
                rx.recv().await.expect("channel closed");
            }
        });
    });
}

benchmark_group!(signal_group, many_signals,);

benchmark_main!(signal_group);
