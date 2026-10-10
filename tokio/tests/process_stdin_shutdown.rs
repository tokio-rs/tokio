#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", unix, not(target_os = "wasi"), not(miri)))]

use std::io::IoSlice;
use std::os::fd::AsFd;
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
async fn shutdown_sends_eof_without_dropping_stdin() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"hello\n").await.unwrap();
    stdin.shutdown().await.unwrap();

    let output = timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .expect("child did not receive EOF after stdin shutdown")
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "6");
    stdin.shutdown().await.unwrap();
    let error = stdin.write(b"x").await.unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EBADF));
    let error = stdin
        .write_vectored(&[IoSlice::new(b"x")])
        .await
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EBADF));
    let error = stdin.into_owned_fd().unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EBADF));
}

#[tokio::test]
async fn shutdown_stdin_cannot_be_converted_to_stdio() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.shutdown().await.unwrap();
    let result: std::io::Result<Stdio> = stdin.try_into();
    assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EBADF));
    assert!(timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap()
        .success());
}

#[tokio::test]
#[should_panic(expected = "child stdin has been shut down")]
async fn as_fd_panics_after_stdin_shutdown() {
    let mut child = child_with_stdin();
    let mut stdin = child.stdin.take().unwrap();
    stdin.shutdown().await.unwrap();
    let _ = stdin.as_fd();
}
