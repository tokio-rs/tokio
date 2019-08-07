#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

#[macro_use]
extern crate log;

use std::io;
use std::process::{Command, ExitStatus, Stdio};

use futures_util::future;
use futures_util::future::FutureExt;
use futures_util::stream::StreamExt;
use tokio::codec::{FramedRead, LinesCodec};
use tokio::io::AsyncWriteExt;
use tokio_process::{Child, CommandExt};

mod support;

fn cat() -> Command {
    let mut cmd = support::cmd("cat");
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    cmd
}

async fn feed_cat(mut cat: Child, n: usize) -> io::Result<ExitStatus> {
    let mut stdin = cat.stdin().take().unwrap();
    let stdout = cat.stdout().take().unwrap();

    // Produce n lines on the child's stdout.
    let write = async {
        debug!("starting to feed");

        for i in 0..n {
            debug!("sending line {} to child", i);
            let bytes = format!("line {}\n", i).into_bytes();
            stdin.write_all(&bytes).await.unwrap();
        }

        drop(stdin);
    };

    let read = async {
        let mut reader = FramedRead::new(stdout, LinesCodec::new());
        let mut num_lines = 0;

        // Try to read `n + 1` lines, ensuring the last one is empty
        // (i.e. EOF is reached after `n` lines.
        loop {
            debug!("starting read from child");

            let data = reader
                .next()
                .await
                .unwrap_or_else(|| Ok(String::new()))
                .expect("failed to read line");

            let num_read = data.len();
            let done = num_lines >= n;

            debug!(
                "read line {} from child ({} bytes, done: {})",
                num_lines, num_read, done
            );

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
    future::join3(write, read, cat)
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
    let child = cat().spawn_async().unwrap();
    let status = support::with_timeout(feed_cat(child, 10000)).await.unwrap();
    assert_eq!(status.code(), Some(0));
}

#[tokio::test]
async fn wait_with_output_captures() {
    let mut child = cat().spawn_async().unwrap();
    let mut stdin = child.stdin().take().unwrap();

    let write_bytes = b"1234";

    let future = async {
        stdin.write_all(write_bytes).await?;
        drop(stdin);
        let out = child.wait_with_output();
        out.await
    };

    let output = support::with_timeout(future).await.unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, write_bytes);
    assert_eq!(output.stderr.len(), 0);
}

#[tokio::test]
async fn status_closes_any_pipes() {
    // Cat will open a pipe between the parent and child.
    // If `status_async` doesn't ensure the handles are closed,
    // we would end up blocking forever (and time out).
    let child = cat().status_async().expect("failed to spawn child");

    support::with_timeout(child)
        .await
        .expect("time out exceeded! did we get stuck waiting on the child?");
}
