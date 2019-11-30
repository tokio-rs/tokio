#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]
#![cfg(unix)]

use futures::future::join_all;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::task;
use tokio::time::timeout;

#[tokio::test]
async fn issue_42() {
    // We spawn a many batches of processes which should exit at roughly the
    // same time (modulo OS scheduling delays), to make sure that consuming
    // a readiness event for one process doesn't inadvertently starve another.
    // We then do this many times (in parallel) in an effort to stress test the
    // implementation to ensure there are no race conditions.
    // See alexcrichton/tokio-process#42 for background
    let join_handles = (0..10usize).into_iter().map(|_| {
        task::spawn(async {
            let processes = (0..10usize).into_iter().map(|i| {
                Command::new("echo")
                    .arg(format!("I am spawned process #{}", i))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .unwrap()
            });

            join_all(processes).await;
        })
    });

    timeout(Duration::from_secs(1), join_all(join_handles))
        .await
        .expect("timed out, did we deadlock?");
}
