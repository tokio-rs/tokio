#![cfg(feature = "process")]
#![warn(rust_2018_idioms)]
// This test reveals a difference in behavior of kqueue on FreeBSD. When the
// reader disconnects, there does not seem to be an `EVFILT_WRITE` filter that
// is returned.
//
// It is expected that `EVFILT_WRITE` would be returned with either the
// `EV_EOF` or `EV_ERROR` flag set. If either flag is set a write would be
// attempted, but that does not seem to occur.
#![cfg(all(unix, not(target_os = "freebsd")))]

use std::process::Stdio;
use std::time::Duration;
use tokio::prelude::*;
use tokio::process::Command;
use tokio::time;
use tokio_test::assert_err;

#[tokio::test]
async fn issue_2174() {
    let mut child = Command::new("sleep")
        .arg("2")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();

    // Writes will buffer up to 65_636. This *should* loop at least 8 times
    // and then register interest.
    let handle = tokio::spawn(async move {
        let data = [0u8; 8192];
        loop {
            input.write_all(&data).await.unwrap();
        }
    });

    // Sleep enough time so that the child process's stdin's buffer fills.
    time::delay_for(Duration::from_secs(1)).await;

    // Kill the child process.
    child.kill().unwrap();
    let _ = child.await;

    assert_err!(handle.await);
}
