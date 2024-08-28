#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))]

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::join;
use tokio::process::{Child, Command};
use tokio_test::assert_ok;

use futures::future::{self, FutureExt};
use std::env;
use std::io;
use std::process::{ExitStatus, Stdio};
use std::task::ready;

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

#[tokio::test]
async fn pipe_from_one_command_to_another() {
    let mut first = cat().spawn().expect("first cmd");
    let mut third = cat().spawn().expect("third cmd");

    // Convert ChildStdout to Stdio
    let second_stdin: Stdio = first
        .stdout
        .take()
        .expect("first.stdout")
        .try_into()
        .expect("first.stdout into Stdio");

    // Convert ChildStdin to Stdio
    let second_stdout: Stdio = third
        .stdin
        .take()
        .expect("third.stdin")
        .try_into()
        .expect("third.stdin into Stdio");

    let mut second = cat()
        .stdin(second_stdin)
        .stdout(second_stdout)
        .spawn()
        .expect("first cmd");

    let msg = "hello world! please pipe this message through";

    let mut stdin = first.stdin.take().expect("first.stdin");
    let write = async move { stdin.write_all(msg.as_bytes()).await };

    let mut stdout = third.stdout.take().expect("third.stdout");
    let read = async move {
        let mut data = String::new();
        stdout.read_to_string(&mut data).await.map(|_| data)
    };

    let (read, write, first_status, second_status, third_status) =
        join!(read, write, first.wait(), second.wait(), third.wait());

    assert_eq!(msg, read.expect("read result"));
    write.expect("write result");

    assert!(first_status.expect("first status").success());
    assert!(second_status.expect("second status").success());
    assert!(third_status.expect("third status").success());
}

#[tokio::test]
async fn vectored_writes() {
    use bytes::{Buf, Bytes};
    use std::{io::IoSlice, pin::Pin};
    use tokio::io::AsyncWrite;

    let mut cat = cat().spawn().unwrap();
    let mut stdin = cat.stdin.take().unwrap();
    let are_writes_vectored = stdin.is_write_vectored();
    let mut stdout = cat.stdout.take().unwrap();

    let write = async {
        let mut input = Bytes::from_static(b"hello\n").chain(Bytes::from_static(b"world!\n"));
        let mut writes_completed = 0;

        futures::future::poll_fn(|cx| loop {
            let mut slices = [IoSlice::new(&[]); 2];
            let vectored = input.chunks_vectored(&mut slices);
            if vectored == 0 {
                return std::task::Poll::Ready(std::io::Result::Ok(()));
            }
            let n = ready!(Pin::new(&mut stdin).poll_write_vectored(cx, &slices))?;
            writes_completed += 1;
            input.advance(n);
        })
        .await?;

        drop(stdin);

        std::io::Result::Ok(writes_completed)
    };

    let read = async {
        let mut buffer = Vec::with_capacity(6 + 7);
        stdout.read_to_end(&mut buffer).await?;
        std::io::Result::Ok(buffer)
    };

    let (write, read, status) = future::join3(write, read, cat.wait()).await;

    assert!(status.unwrap().success());

    let writes_completed = write.unwrap();
    // on unix our small payload should always fit in whatever default sized pipe with a single
    // syscall. if multiple are used, then the forwarding does not work, or we are on a platform
    // for which the `std` does not support vectored writes.
    assert_eq!(writes_completed == 1, are_writes_vectored);

    assert_eq!(&read.unwrap(), b"hello\nworld!\n");
}
