#![cfg(all(unix, feature = "process"))]
#![warn(rust_2018_idioms)]

use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::delay_for;
use tokio_test::assert_ok;

#[tokio::test]
async fn kill_on_drop() {
    let mut cmd = Command::new("sh");
    cmd.args(&[
        "-c",
        "
       # Fork another child that won't get killed
       sh -c 'sleep 1; echo child ran' &
       disown -a

       # Await our death
       sleep 5
       echo hello from beyond the grave
    ",
    ]);

    let mut child = cmd
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    delay_for(Duration::from_secs(2)).await;

    let mut out = child.stdout.take().unwrap();
    drop(child);

    let mut msg = String::new();
    assert_ok!(out.read_to_string(&mut msg).await);

    assert_eq!("child ran\n", msg);
}
