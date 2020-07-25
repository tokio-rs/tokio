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

#[test]
fn can_enter_basic_rt_from_within_block_in_place() {
    let mut outer = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .build()
        .unwrap();

    outer.block_on(async {
        tokio::task::block_in_place(|| {
            let mut inner = tokio::runtime::Builder::new()
                .basic_scheduler()
                .build()
                .unwrap();

            inner.block_on(async {})
        })
    });
}

#[test]
fn useful_panic_message_when_dropping_rt_in_rt() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let mut outer = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .build()
        .unwrap();

    let result = catch_unwind(AssertUnwindSafe(|| {
        outer.block_on(async {
            let _ = tokio::runtime::Builder::new()
                .basic_scheduler()
                .build()
                .unwrap();
        });
    }));

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err: &'static str = err.downcast_ref::<&'static str>().unwrap();

    assert!(
        err.find("Cannot drop a runtime").is_some(),
        "Wrong panic message: {:?}",
        err
    );
}

#[test]
fn can_shutdown_with_zero_timeout_in_runtime() {
    let mut outer = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .build()
        .unwrap();

    outer.block_on(async {
        let rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .build()
            .unwrap();
        rt.shutdown_timeout(Duration::from_nanos(0));
    });
}

#[test]
fn can_shutdown_now_in_runtime() {
    let mut outer = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .build()
        .unwrap();

    outer.block_on(async {
        let rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .build()
            .unwrap();
        rt.shutdown_background();
    });
}
