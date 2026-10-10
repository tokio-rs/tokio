#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", unix, not(target_os = "wasi"), not(miri)))]

use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::time::timeout;

fn child_with_stdin() -> Child {
    Command::new("wc")
        .arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn shutdown_does_not_send_eof_without_dropping_stdin() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"hello\n").await.unwrap();
    stdin.shutdown().await.unwrap();

    let result = timeout(Duration::from_secs(5), child.wait_with_output()).await;
    assert!(
        result.is_err(),
        "expected the existing stdin shutdown stall"
    );
}
