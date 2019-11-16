#![cfg(feature = "process")]
#![cfg(unix)]
#![warn(rust_2018_idioms)]

use tokio::process::Command;
use tokio::runtime;

use futures::future::FutureExt;
use futures::stream::FuturesOrdered;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn run_test() {
    let finished = Arc::new(AtomicBool::new(false));
    let finished_clone = finished.clone();

    thread::spawn(move || {
        let mut rt = runtime::Builder::new().current_thread().build().unwrap();

        let mut futures = FuturesOrdered::new();
        rt.block_on(async {
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

#[test]
fn issue_42() {
    let max = 10;
    for i in 0..max {
        println!("running {}/{}", i, max);
        run_test()
    }
}
