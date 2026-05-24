#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "time", feature = "rt-multi-thread"))]

use std::future::Future;
use std::task::Context;

use futures::task::noop_waker_ref;

use tokio::runtime::Runtime;
use tokio::time::*;
use tokio_test::assert_ready;

fn rt_combinations() -> Vec<Runtime> {
    let mut rts = vec![];

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    rts.push(rt);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    rts.push(rt);

    #[cfg(tokio_unstable)]
    {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_alt_timer()
            .enable_all()
            .build()
            .unwrap();
        rts.push(rt);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_alt_timer()
            .enable_all()
            .build()
            .unwrap();
        rts.push(rt);
    }

    rts
}

#[test]
fn sleep() {
    const N: u32 = 512;

    for rt in rt_combinations() {
        rt.block_on(async {
            let mut jhs = vec![];

            // sleep outside of the worker threads
            let now = Instant::now();
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert!(now.elapsed() >= Duration::from_millis(10));

            for _ in 0..N {
                let jh = tokio::spawn(async move {
                    // sleep inside of the worker threads
                    let now = Instant::now();
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    assert!(now.elapsed() >= Duration::from_millis(10));
                });
                jhs.push(jh);
            }

            for jh in jhs {
                jh.await.unwrap();
            }
        });
    }
}

#[test]
fn timeout() {
    const N: u32 = 512;

    for rt in rt_combinations() {
        rt.block_on(async {
            let mut jhs = vec![];

            // timeout outside of the worker threads
            let now = Instant::now();
            tokio::time::timeout(Duration::from_millis(10), std::future::pending::<()>())
                .await
                .expect_err("timeout should occur");
            assert!(now.elapsed() >= Duration::from_millis(10));

            for _ in 0..N {
                let jh = tokio::spawn(async move {
                    let now = Instant::now();
                    // timeout inside of the worker threads
                    tokio::time::timeout(Duration::from_millis(10), std::future::pending::<()>())
                        .await
                        .expect_err("timeout should occur");
                    assert!(now.elapsed() >= Duration::from_millis(10));
                });
                jhs.push(jh);
            }

            for jh in jhs {
                jh.await.unwrap();
            }
        });
    }
}

#[test]
fn poll_after_completion_is_ready() {
    // `unconstrained` removes coop so the assertion only depends on `Sleep`.
    for rt in rt_combinations() {
        rt.block_on(async {
            let timer =
                tokio::task::coop::unconstrained(tokio::time::sleep(Duration::from_millis(1)));
            tokio::pin!(timer);

            timer.as_mut().await;

            for _ in 0..1024 {
                assert_ready!(timer
                    .as_mut()
                    .poll(&mut Context::from_waker(noop_waker_ref())));
            }
        });
    }
}
