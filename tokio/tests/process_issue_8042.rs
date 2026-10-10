#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", unix, not(target_os = "wasi"), not(miri)))]

use std::future::poll_fn;
use std::os::fd::OwnedFd;
use std::pin::Pin;
use std::process::Stdio;
use std::task::Poll;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::process::{ChildStdin, Command};
use tokio::time::timeout;

async fn read_chunks<R: AsyncRead + Unpin>(
    reader: &mut R,
    stdin: &mut ChildStdin,
    first: &[u8],
    second: &[u8],
) {
    let mut buf = vec![0; first.len()];
    reader.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, first);

    // The child waits for our acknowledgement, so this read must return Pending.
    poll_fn(|cx| {
        let mut bytes = [0; 1];
        let mut buf = ReadBuf::new(&mut bytes);
        assert!(Pin::new(&mut *reader).poll_read(cx, &mut buf).is_pending());
        Poll::Ready(())
    })
    .await;

    stdin.write_all(b"continue\n").await.unwrap();
    buf.resize(second.len(), 0);
    reader.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, second);

    // Keep the child alive until we have received the second chunk. Otherwise,
    // a hangup event could wake the reader even if readable interest was not rearmed.
    stdin.write_all(b"exit\n").await.unwrap();
    assert_eq!(reader.read(&mut buf).await.unwrap(), 0);
}

#[tokio::test]
async fn child_stdout_receives_second_chunk_before_exit() {
    let mut child = Command::new("sh")
        .args([
            "-c",
            "printf 'chunk1\\n'; read gate; printf 'chunk2\\n'; read gate",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let result = timeout(
        Duration::from_secs(5),
        read_chunks(&mut stdout, &mut stdin, b"chunk1\n", b"chunk2\n"),
    )
    .await;

    // The current SourceFd implementation misses rearming on poll selectors.
    if cfg!(any(
        mio_unsupported_force_poll_poll,
        target_os = "cygwin",
        target_os = "solaris",
    )) {
        assert!(result.is_err(), "expected the existing stdout poll stall");
        return;
    }
    result.expect("stdout stalled while the child was still alive");

    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("child wait timed out")
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn child_stderr_receives_second_chunk_before_exit() {
    let mut child = Command::new("sh")
        .args([
            "-c",
            "printf 'err1\\n' >&2; read gate; printf 'err2\\n' >&2; read gate",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let result = timeout(
        Duration::from_secs(5),
        read_chunks(&mut stderr, &mut stdin, b"err1\n", b"err2\n"),
    )
    .await;

    // The current SourceFd implementation misses rearming on poll selectors.
    if cfg!(any(
        mio_unsupported_force_poll_poll,
        target_os = "cygwin",
        target_os = "solaris",
    )) {
        assert!(result.is_err(), "expected the existing stderr poll stall");
        return;
    }
    result.expect("stderr stalled while the child was still alive");

    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("child wait timed out")
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn child_stdin_rearms_after_pipe_is_full() {
    let (gate, child_gate) = std::os::unix::net::UnixStream::pair().unwrap();
    gate.set_nonblocking(true).unwrap();
    let mut gate = tokio::net::UnixStream::from_std(gate).unwrap();
    let mut child = Command::new("sh")
        .args(["-c", "read gate <&2; wc -c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(OwnedFd::from(child_gate)))
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut written = 0;
    let result = timeout(Duration::from_secs(5), async {
        // The child waits on a separate socket while we fill its stdin pipe.
        // Disable cooperative yielding so Pending means the pipe is full.
        tokio::task::coop::unconstrained(poll_fn(|cx| loop {
            match Pin::new(&mut stdin).poll_write(cx, &[b'x'; 8192]) {
                Poll::Ready(Ok(n)) => {
                    assert!(n > 0);
                    written += n;
                }
                Poll::Ready(Err(err)) => panic!("writing stdin failed: {err}"),
                Poll::Pending if written == 0 => return Poll::Pending,
                Poll::Pending => return Poll::Ready(()),
            }
        }))
        .await;

        gate.write_all(b"continue\n").await.unwrap();
        stdin.write_all(b"x").await.unwrap();
        drop(stdin);

        let mut output = String::new();
        stdout.read_to_string(&mut output).await.unwrap();
        assert_eq!(output.trim().parse::<usize>().unwrap(), written + 1);
        assert!(child.wait().await.unwrap().success());
    })
    .await;

    // The current SourceFd implementation misses rearming on poll selectors.
    if cfg!(any(
        mio_unsupported_force_poll_poll,
        target_os = "cygwin",
        target_os = "solaris",
    )) {
        assert!(result.is_err(), "expected the existing stdin poll stall");
        return;
    }
    result.expect("stdin stalled after the child started reading");
}
