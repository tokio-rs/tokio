#![warn(rust_2018_idioms)]
#![cfg(all(
    feature = "full",
    any(unix, windows),
    not(target_os = "wasi"),
    not(miri)
))]

use std::io::IoSlice;
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(windows)]
use std::os::windows::io::AsHandle;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::time::timeout;

fn child_with_stdin() -> Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "stdin_reader", "--nocapture", "--quiet"]);
    // The abort test harness otherwise runs the reader in another subprocess.
    #[cfg(panic = "abort")]
    command.args(["-Z", "unstable-options", "--force-run-in-process"]);
    command.env("TOKIO_STDIN_SHUTDOWN_CHILD", "1");
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn shutdown_sends_eof_without_dropping_stdin() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    let data = vec![b'x'; 256 * 1024];
    timeout(Duration::from_secs(5), async {
        stdin.write_all(&data).await.unwrap();
        stdin.shutdown().await.unwrap();
    })
    .await
    .expect("stdin write or shutdown timed out");

    let output = timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .expect("child did not receive EOF after stdin shutdown")
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .last()
            .unwrap()
            .trim(),
        data.len().to_string()
    );
    stdin.shutdown().await.unwrap();
    stdin.flush().await.unwrap();
    let error = stdin.write(b"x").await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    let error = stdin
        .write_vectored(&[IoSlice::new(b"x")])
        .await
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    #[cfg(unix)]
    let error = stdin.into_owned_fd().unwrap_err();
    #[cfg(windows)]
    let error = stdin.into_owned_handle().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
}

#[tokio::test]
async fn shutdown_stdin_cannot_be_converted_to_stdio() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.shutdown().await.unwrap();
    let result: std::io::Result<Stdio> = stdin.try_into();
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
    assert!(timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("child wait timed out")
        .unwrap()
        .success());
}

#[cfg(unix)]
#[tokio::test]
#[should_panic(expected = "child stdin has been shut down")]
async fn as_fd_panics_after_stdin_shutdown() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.shutdown().await.unwrap();
    let _ = stdin.as_fd();
}

#[cfg(windows)]
#[tokio::test]
#[should_panic(expected = "child stdin has been shut down")]
async fn as_handle_panics_after_stdin_shutdown() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.shutdown().await.unwrap();
    let _ = stdin.as_handle();
}

#[cfg(windows)]
#[test]
fn shutdown_waits_for_pending_write() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let blocker = runtime.spawn_blocking(move || {
        started_tx.send(()).unwrap();
        let _ = release_rx.recv();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    runtime.block_on(async move {
        let mut child = child_with_stdin();
        let mut stdin = child.stdin.take().unwrap();

        // The only blocking thread is occupied, so the pipe write stays queued.
        assert_eq!(stdin.write(b"x").await.unwrap(), 1);
        let mut shutdown = tokio_test::task::spawn(stdin.shutdown());
        tokio_test::assert_pending!(shutdown.poll());
        drop(shutdown);

        release_tx.send(()).unwrap();
        timeout(Duration::from_secs(5), stdin.shutdown())
            .await
            .expect("stdin shutdown timed out after releasing the writer")
            .unwrap();
        let output = timeout(Duration::from_secs(5), child.wait_with_output())
            .await
            .expect("child did not receive EOF after stdin shutdown")
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().lines().last(),
            Some("1")
        );
        blocker.await.unwrap();
    });
}

#[test]
fn stdin_reader() {
    if std::env::var_os("TOKIO_STDIN_SHUTDOWN_CHILD").is_none() {
        return;
    }

    use std::io::Read;
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).unwrap();
    assert!(data.iter().all(|&byte| byte == b'x'));
    println!("{}", data.len());
    std::process::exit(0);
}
