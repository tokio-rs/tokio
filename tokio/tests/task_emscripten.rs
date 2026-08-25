#![cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]

/// There is no threadpool on a single-threaded JS worker: the public
/// `spawn_blocking` is unsupported on non-pthread emscripten, as on the other
/// single-threaded wasm targets. `tokio::fs` and
/// `tokio::io::{stdin, stdout, stderr}` do not rely on it there — their
/// syscalls complete synchronously.
#[tokio::test]
#[should_panic = "OS can't spawn worker thread"]
async fn spawn_blocking_is_unsupported() {
    let _ = tokio::task::spawn_blocking(|| 42).await;
}
