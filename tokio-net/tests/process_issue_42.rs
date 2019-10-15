#![cfg(feature = "process")]
#![cfg(unix)]
#![warn(rust_2018_idioms)]

use futures_util::future::FutureExt;
use futures_util::stream::FuturesOrdered;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::current_thread;
use tokio_net::process::Command;

mod support;
use support::*;

async fn run_test() {
    let finished = Arc::new(AtomicBool::new(false));
    let finished_clone = finished.clone();

    thread::spawn(move || {
        let mut rt = current_thread::Runtime::new().expect("failed to get runtime");
        let mut futures = FuturesOrdered::new();
        run_with_timeout(&mut rt, async {
            for i in 0..2 {
                futures.push(
                    Command::new("echo")
                        .arg(format!("I am spawned process #{}", i))
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .unwrap()
                        .boxed(),
                )
            }
        });
        drop(rt);
        finished_clone.store(true, Ordering::SeqCst);
    });

    thread::sleep(Duration::from_millis(1000));
    assert!(
        finished.load(Ordering::SeqCst),
        "FINISHED flag not set, maybe we deadlocked?"
    );
}

#[tokio::test]
async fn issue_42() {
    let max = 10;
    for i in 0..max {
        println!("running {}/{}", i, max);
        run_test().await
    }
}
