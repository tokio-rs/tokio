#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::join;
use tokio::process::Command;

use std::convert::TryInto;
use std::env;
use std::process::Stdio;

fn cat() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_test-cat"));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    cmd
}

// NB: this test is placed in its own test-suite to avoid hitting CI
// resource limits with too many opened processes.
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
