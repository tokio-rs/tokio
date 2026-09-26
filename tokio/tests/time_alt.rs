#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "time", feature = "rt-multi-thread"))]

use tokio::runtime::Runtime;
use tokio::time::*;

use std::future::Future;

use futures::FutureExt;
use futures_test::task::noop_context;
use tokio_test::assert_pending;

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
/// It is possible that a timer is created in one runtime,
/// but `.reset()` is called in a different runtime.
/// In this case, the timer should be registered in the original runtime.
fn reset_should_stay_on_same_runtime() {
    let rt1 = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_alt_timer()
        .build()
        .unwrap();

    let rt2 = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_alt_timer()
        .build()
        .unwrap();

    // Register the timer into the local timer wheel of `rt1`.
    //
    // We cannot use bare `rt1.block_on` as the local timer wheel of `rt1`
    // is only accessible from the worker threads of `rt1`,
    // but `rt1.block_on` runs the future on the current thread.
    // So we need to use `rt1.spawn` to run the future on the worker thread.
    let sleep = rt1
        .block_on(
            #[allow(clippy::async_yields_async)]
            rt1.spawn(async {
                // 1 hour is long enough to make sure the timer is not fired before we call `reset()`.
                tokio::time::sleep(Duration::from_secs(3600))
            }),
        )
        .unwrap();
    let mut sleep = Box::pin(sleep);
    assert_pending!(sleep.as_mut().poll(&mut noop_context()));

    // reset the timer created from `rt1` in `rt2`,
    // which should register the timer into the local timer wheel of `rt1`.
    let sleep = rt2
        .block_on({
            #[allow(clippy::async_yields_async)]
            rt2.spawn(async move {
                sleep
                    .as_mut()
                    .reset(Instant::now() + Duration::from_secs(3600));
                sleep
            })
        })
        .unwrap();

    // drop `rt1` to fire all timers registered in `rt1`,
    // including the timer we just reset.
    drop(rt1);

    // If this assertion fails, it means the timer is not registered in `rt1`.
    // This can happen if the timer is registered in `rt2` instead of `rt1`,
    assert!(sleep.now_or_never().is_some());
}

#[test]
/// It is possible that a timer is created in one runtime,
/// but `.reset()` is called in a different runtime.
/// In this case, the timer should be registered in the original runtime.
fn reset_should_stay_on_same_runtime2() {
    let rt1 = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_alt_timer()
        .build()
        .unwrap();

    let rt2 = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    // Register the timer into the local timer wheel of `rt1`.
    //
    // We cannot use bare `rt1.block_on` as the local timer wheel of `rt1`
    // is only accessible from the worker threads of `rt1`,
    // but `rt1.block_on` runs the future on the current thread.
    // So we need to use `rt1.spawn` to run the future on the worker thread.
    let sleep = rt1
        .block_on(
            #[allow(clippy::async_yields_async)]
            rt1.spawn(async {
                // 1 hour is long enough to make sure the timer is not fired before we call `reset()`.
                tokio::time::sleep(Duration::from_secs(3600))
            }),
        )
        .unwrap();
    let mut sleep = Box::pin(sleep);
    assert_pending!(sleep.as_mut().poll(&mut noop_context()));

    // reset the timer created from `rt1` in `rt2`,
    // which should register the timer into the local timer wheel of `rt1`.
    let sleep = rt2
        .block_on({
            #[allow(clippy::async_yields_async)]
            rt2.spawn(async move {
                sleep
                    .as_mut()
                    .reset(Instant::now() + Duration::from_secs(3600));
                sleep
            })
        })
        .unwrap();

    // drop `rt1` to fire all timers registered in `rt1`,
    // including the timer we just reset.
    drop(rt1);

    // If this assertion fails, it means the timer is not registered in `rt1`.
    // This can happen if the timer is registered in `rt2` instead of `rt1`,
    assert!(sleep.now_or_never().is_some());
}
