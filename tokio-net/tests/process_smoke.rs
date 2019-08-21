#![cfg(feature = "process")]
#![warn(rust_2018_idioms)]

use tokio_net::process::Command;

mod support;
use support::*;

#[tokio::test]
async fn simple() {
    let mut cmd;

    if cfg!(windows) {
        cmd = Command::new("cmd");
        cmd.arg("/c");
    } else {
        cmd = Command::new("sh");
        cmd.arg("-c");
    }

    let mut child = cmd.arg("exit 2").spawn().unwrap();

    let id = child.id();
    assert!(id > 0);

    let status = with_timeout(&mut child)
        .await
        .expect("failed to run future");
    assert_eq!(status.code(), Some(2));

    assert_eq!(child.id(), id);
    drop(child.kill());
}
