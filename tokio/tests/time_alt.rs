#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "time", feature = "rt-multi-thread"))]

use tokio::runtime::Runtime;
use tokio::time::*;

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
fn cancelled_timer_waker_drop_can_register_timer() {
    use futures::FutureExt;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Wake, Waker};
    use tokio_test::assert_pending;

    struct ReenterOnDrop;

    #[allow(unknown_lints, clippy::manual_noop_waker)]
    impl Wake for ReenterOnDrop {
        fn wake(self: Arc<Self>) {}
    }

    impl Drop for ReenterOnDrop {
        fn drop(&mut self) {
            let _ = tokio::time::sleep(Duration::from_secs(5)).now_or_never();
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_alt_timer()
        .enable_time()
        .build()
        .unwrap();

    rt.block_on(async {
        tokio::spawn(async {
            let mut timer = Box::pin(tokio::time::sleep(Duration::from_secs(10)));
            let waker = Waker::from(Arc::new(ReenterOnDrop));
            assert_pending!(timer.as_mut().poll(&mut Context::from_waker(&waker)));
            drop(waker);

            tokio::task::yield_now().await;
            drop(timer);
            tokio::task::yield_now().await;
        })
        .await
        .unwrap();
    });
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
