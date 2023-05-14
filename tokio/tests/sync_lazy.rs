#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", feature = "parking_lot"))]

use core::panic;
use std::ops::Drop;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::runtime;
use tokio::sync::Lazy;
use tokio::time;

use std::time::Duration;

#[test]
fn drop_lazy() {
    let rt = runtime::Builder::new_current_thread().build().unwrap();

    static NUM_DROPS: AtomicU32 = AtomicU32::new(0);

    rt.block_on(async {
        struct Foo {}

        let fooer = Foo {};

        impl Drop for Foo {
            fn drop(&mut self) {
                NUM_DROPS.fetch_add(1, Ordering::Release);
            }
        }

        {
            let lazy = Lazy::new(|| Box::pin(async { fooer }));
            lazy.force().await;
        }
        assert!(NUM_DROPS.load(Ordering::Acquire) == 1);
    });
}

#[test]
fn force() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();

    static LAZY: Lazy<u32> = Lazy::const_new(|| Box::pin(async { 5 }));

    rt.block_on(async {
        let handle1 = rt.spawn(async { *LAZY.force().await });
        let handle2 = rt.spawn(async {
            time::sleep(Duration::from_millis(1)).await;
            *LAZY.force().await
        });

        time::advance(Duration::from_millis(1)).await;
        time::resume();

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert_eq!(result1, 5);
        assert_eq!(result2, 5);
    });
}

#[test]
#[cfg_attr(not(panic = "unwind"), should_panic)]
fn force_panic() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();

    static LAZY: Lazy<u32> = Lazy::const_new(|| Box::pin(async { panic!() }));

    rt.block_on(async {
        let handle1 = rt.spawn(async { *LAZY.force().await });
        let handle2 = rt.spawn(async {
            time::sleep(Duration::from_millis(1)).await;
            *LAZY.force().await
        });

        time::advance(Duration::from_millis(1)).await;
        time::resume();

        handle1.await.unwrap_err();
        handle2.await.unwrap_err();

        assert_eq!(LAZY.get(), None);
    });
}

#[test]
fn force_and_get() {
    let rt = runtime::Builder::new_current_thread().build().unwrap();

    static LAZY: Lazy<u32> = Lazy::const_new(|| Box::pin(async { 5 }));

    rt.block_on(async {
        let _ = rt.spawn(async { LAZY.force().await }).await;
        let value = *LAZY.get().unwrap();
        assert_eq!(value, 5);
    });
}

#[test]
fn get_uninit() {
    static LAZY: Lazy<u32> = Lazy::const_new(|| Box::pin(async { panic!() }));
    let uninit = LAZY.get();
    assert!(uninit.is_none());
}
