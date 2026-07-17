//! Regression tests for emscripten-specific behavior: the `u64`-microseconds
//! `Instant` (whose f64-milliseconds predecessor broke exact arithmetic) and
//! the unsupported-`spawn_blocking` contract. Broader async coverage comes
//! from the standard suite, which runs on emscripten.

#![cfg(target_os = "emscripten")]

use std::time::Duration;

use tokio::time::Instant;

#[test]
fn instant_now_works() {
    let now = Instant::now();
    let _ = now.elapsed();
}

#[test]
fn instant_comparison() {
    let a = Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(());
    }
    let b = Instant::now();
    assert!(b >= a, "later instant should be >= earlier instant");
}

#[test]
fn instant_arithmetic() {
    // Pins the round-trip property `(t + d) - d == t` that the original
    // f64-milliseconds Instant impl broke. The u64-microseconds storage
    // makes this exact.
    let now = Instant::now();
    let duration = Duration::from_millis(100);
    let later = now + duration;
    assert!(later > now);
    let diff = later - now;
    assert_eq!(diff, duration);
    let back = later - duration;
    assert_eq!(back, now);
}

#[test]
fn instant_with_seconds() {
    let now = Instant::now();
    let duration = Duration::from_secs(1);
    let later = now + duration;
    let elapsed = later.duration_since(now);
    assert_eq!(elapsed.as_secs(), 1);
}

/// There is no threadpool on a single-threaded JS worker: `spawn_blocking`
/// is unsupported on emscripten, as on the other single-threaded wasm
/// targets. `tokio::fs` and `tokio::io::{stdin, stdout, stderr}` do not rely
/// on it there — their syscalls complete synchronously.
#[tokio::test]
#[should_panic = "OS can't spawn worker thread"]
async fn spawn_blocking_is_unsupported() {
    let _ = tokio::task::spawn_blocking(|| 42).await;
}
