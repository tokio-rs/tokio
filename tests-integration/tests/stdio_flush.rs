#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use std::process::Stdio;

fn stdio_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_test-stdio-flush"));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    cmd
}

async fn assert_flush_works_fn() {
    let mut proc = stdio_cmd().spawn().expect("spawning test process");

    let mut stdin = proc.stdin.take().unwrap();
    let mut stdout = proc.stdout.take().unwrap();

    let mut buf = [0; b"Hello world!".len()];
    stdout.read_exact(&mut buf).await.unwrap();

    assert_eq!(&buf, b"Hello world!");

    stdin.write_all(&[16]).await.unwrap();
    stdin.flush().await.unwrap();
    drop(stdin);

    let status = proc.wait().await.unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn assert_flush_works() {
    if timeout(Duration::from_secs(10), assert_flush_works_fn())
        .await
        .is_err()
    {
        panic!("Test timed out.");
    }
}
