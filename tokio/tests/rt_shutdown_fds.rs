#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", target_os = "linux", not(miri)))]

//! Regression test for #8703: dropping a runtime must release its I/O driver's
//! file descriptors even while a `RuntimeMetrics` (and so a `Handle`) is alive.
//!
//! This file has a single test on purpose: tests in one binary run in
//! parallel, and any concurrent fd activity would skew the count.

use tokio::runtime::{Builder, Runtime, RuntimeMetrics};

const ROUNDS: usize = 50;

/// Extra fds tolerated for process-wide activity unrelated to the runtimes. A
/// leak of even one fd per runtime would exceed it.
const SLACK: usize = 2;

fn open_fds() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

fn leaked_fds(build: fn() -> Runtime) -> usize {
    // Warm up process-wide state, such as the signal driver's global socket pair.
    drop(build());

    let before = open_fds();
    let mut kept: Vec<RuntimeMetrics> = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let rt = build();
        rt.block_on(async {
            tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        });
        kept.push(rt.metrics());
        drop(rt);
    }
    let after = open_fds();
    drop(kept);

    after.saturating_sub(before)
}

#[test]
fn dropped_runtime_releases_fds_while_metrics_alive() {
    let current = leaked_fds(|| Builder::new_current_thread().enable_all().build().unwrap());
    let multi = leaked_fds(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    });

    assert!(
        current <= SLACK && multi <= SLACK,
        "fds leaked over {ROUNDS} runtimes: current_thread {current}, multi_thread {multi}"
    );
}
