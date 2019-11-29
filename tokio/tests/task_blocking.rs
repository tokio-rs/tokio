#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::task;
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
