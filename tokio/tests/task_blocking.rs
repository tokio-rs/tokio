#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::{runtime, task};
use tokio_test::assert_ok;

use std::thread;
use std::time::Duration;

#[tokio::test]
async fn basic_blocking() {
    // Run a few times
    for _ in 0..100 {
        let out = assert_ok!(
            tokio::spawn(async {
                assert_ok!(
                    task::spawn_blocking(|| {
                        thread::sleep(Duration::from_millis(5));
                        "hello"
                    })
                    .await
                )
            })
            .await
        );

        assert_eq!(out, "hello");
    }
}

#[tokio::test(threaded_scheduler)]
async fn block_in_blocking() {
    // Run a few times
    for _ in 0..100 {
        let out = assert_ok!(
            tokio::spawn(async {
                assert_ok!(
                    task::spawn_blocking(|| {
                        task::block_in_place(|| {
                            thread::sleep(Duration::from_millis(5));
                        });
                        "hello"
                    })
                    .await
                )
            })
            .await
        );

        assert_eq!(out, "hello");
    }
}

#[tokio::test(threaded_scheduler)]
async fn block_in_block() {
    // Run a few times
    for _ in 0..100 {
        let out = assert_ok!(
            tokio::spawn(async {
                task::block_in_place(|| {
                    task::block_in_place(|| {
                        thread::sleep(Duration::from_millis(5));
                    });
                    "hello"
                })
            })
            .await
        );

        assert_eq!(out, "hello");
    }
}

#[tokio::test(basic_scheduler)]
#[should_panic]
async fn no_block_in_basic_scheduler() {
    task::block_in_place(|| {});
}

#[test]
fn yes_block_in_threaded_block_on() {
    let mut rt = runtime::Builder::new()
        .threaded_scheduler()
        .build()
        .unwrap();
    rt.block_on(async {
        task::block_in_place(|| {});
    });
}

#[test]
#[should_panic]
fn no_block_in_basic_block_on() {
    let mut rt = runtime::Builder::new().basic_scheduler().build().unwrap();
    rt.block_on(async {
        task::block_in_place(|| {});
    });
}
