#![cfg(unix)]

extern crate tokio_process;

use futures_util::future::FutureExt;
use futures_util::stream::FuturesOrdered;
use futures_util::stream::StreamExt;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio_process::CommandExt;

mod support;

fn run_test() {
    let finished = Arc::new(AtomicBool::new(false));
    let finished_clone = finished.clone();

    thread::spawn(move || {
        let mut futures: FuturesOrdered<Pin<Box<dyn Future<Output = io::Result<ExitStatus>>>>> =
            FuturesOrdered::new();
        for i in 0..2 {
            futures.push(
                Command::new("echo")
                    .arg(format!("I am spawned process #{}", i))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn_async()
                    .unwrap()
                    .boxed(),
            )
        }
        support::run_with_timeout(futures.collect::<Vec<io::Result<ExitStatus>>>());

        finished_clone.store(true, Ordering::SeqCst);
    });

    thread::sleep(Duration::from_millis(100));
    assert!(
        finished.load(Ordering::SeqCst),
        "FINISHED flag not set, maybe we deadlocked?"
    );
}

#[test]
fn issue_42() {
    let max = 10;
    for i in 0..max {
        println!("running {}/{}", i, max);
        run_test()
    }
}
