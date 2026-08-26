//! Standard I/O tests for emscripten.
//!
//! `tokio::io::{stdout, stderr}` round through emscripten's libc to the JS
//! `print`/`printErr` hooks; these tests mostly check that writes don't fail —
//! observable output verification is left to manual `--nocapture` runs.
//!
//! `tokio::io::stdin` reads fd 0 synchronously in emscripten, so this only
//! completes when stdin is non-interactive (EOF), as under CI where
//! the runner's stdin is `/dev/null`. The contract worth pinning is "a stdin
//! read returns rather than deadlocking", not a specific errno.

#![cfg(all(target_os = "emscripten", feature = "io-std"))]

use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn stdout_write_completes() {
    let mut out = tokio::io::stdout();
    out.write_all(b"hello from stdout\n").await.unwrap();
    out.flush().await.unwrap();
}

#[tokio::test]
async fn stderr_write_completes() {
    let mut err = tokio::io::stderr();
    err.write_all(b"hello from stderr\n").await.unwrap();
    err.flush().await.unwrap();
}

#[tokio::test]
async fn stdout_large_multichunk_write_completes() {
    // Exercises the BufWriter chunking inside `Stdout` (writes larger than
    // the internal buffer force multiple underlying writes).
    let mut out = tokio::io::stdout();
    let data = vec![b'x'; 64 * 1024];
    out.write_all(&data).await.unwrap();
    out.flush().await.unwrap();
}

#[tokio::test]
async fn stdout_interleaved_writes_complete() {
    let mut out = tokio::io::stdout();
    for i in 0..16 {
        out.write_all(format!("line {i}\n").as_bytes())
            .await
            .unwrap();
    }
    out.flush().await.unwrap();
}

#[tokio::test]
async fn stdin_read_does_not_hang() {
    // The runner detaches stdin onto the null device, so a read must return
    // promptly (Ok(0) EOF or an I/O error) rather than blocking the host
    // loop. The runner's watchdog fails the test if this ever deadlocks.
    let mut stdin = tokio::io::stdin();
    let mut buf = [0u8; 32];
    let _ = stdin.read(&mut buf).await;
}
