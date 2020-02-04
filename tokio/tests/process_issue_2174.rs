#![cfg(all(unix, feature = "process"))]
#![warn(rust_2018_idioms)]

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
