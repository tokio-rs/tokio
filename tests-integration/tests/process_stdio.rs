#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio_test::assert_ok;

use futures::future::{self, FutureExt};
use std::env;
use std::io;
use std::process::{ExitStatus, Stdio};

fn cat() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_test-cat"));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    cmd
}

async fn feed_cat(mut cat: Child, n: usize) -> io::Result<ExitStatus> {
    let mut stdin = cat.stdin.take().unwrap();
    let stdout = cat.stdout.take().unwrap();

    // Produce n lines on the child's stdout.
    let write = async {
        for i in 0..n {
            let bytes = format!("line {}\n", i).into_bytes();
            stdin.write_all(&bytes).await.unwrap();
        }

        drop(stdin);
    };

    let read = async {
        let mut reader = BufReader::new(stdout).lines();
        let mut num_lines = 0;

        // Try to read `n + 1` lines, ensuring the last one is empty
        // (i.e. EOF is reached after `n` lines.
        loop {
            let data = reader
                .next_line()
                .await
                .unwrap_or_else(|_| Some(String::new()))
                .expect("failed to read line");

            let num_read = data.len();
            let done = num_lines >= n;

            match (done, num_read) {
                (false, 0) => panic!("broken pipe"),
                (true, n) if n != 0 => panic!("extraneous data"),
                _ => {
                    let expected = format!("line {}", num_lines);
                    assert_eq!(expected, data);
                }
            };

            num_lines += 1;
            if num_lines >= n {
                break;
            }
        }
    };

    // Compose reading and writing concurrently.
    future::join3(write, read, cat.wait())
        .map(|(_, _, status)| status)
        .await
}

/// Check for the following properties when feeding stdin and
/// consuming stdout of a cat-like process:
///
/// - A number of lines that amounts to a number of bytes exceeding a
///   typical OS buffer size can be fed to the child without
///   deadlock. This tests that we also consume the stdout
///   concurrently; otherwise this would deadlock.
///
/// - We read the same lines from the child that we fed it.
///
/// - The child does produce EOF on stdout after the last line.
#[tokio::test]
async fn feed_a_lot() {
    let child = cat().spawn().unwrap();
    let status = feed_cat(child, 10000).await.unwrap();
    assert_eq!(status.code(), Some(0));
}

#[tokio::test]
async fn wait_with_output_captures() {
    let mut child = cat().spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();

    let write_bytes = b"1234";

    let future = async {
        stdin.write_all(write_bytes).await?;
        drop(stdin);
        let out = child.wait_with_output();
        out.await
    };

    let output = future.await.unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, write_bytes);
    assert_eq!(output.stderr.len(), 0);
}

#[tokio::test]
async fn status_closes_any_pipes() {
    // Cat will open a pipe between the parent and child.
    // If `status_async` doesn't ensure the handles are closed,
    // we would end up blocking forever (and time out).
    let child = cat().status();

    assert_ok!(child.await);
}

#[tokio::test]
async fn try_wait() {
    let mut child = cat().spawn().unwrap();

    let id = child.id().expect("missing id");
    assert!(id > 0);

    assert_eq!(None, assert_ok!(child.try_wait()));

    // Drop the child's stdio handles so it can terminate
    drop(child.stdin.take());
    drop(child.stderr.take());
    drop(child.stdout.take());

    assert_ok!(child.wait().await);

    // test that the `.try_wait()` method is fused just like the stdlib
    assert!(assert_ok!(child.try_wait()).unwrap().success());

    // Can't get id after process has exited
    assert_eq!(child.id(), None);
}
