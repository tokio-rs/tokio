#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::mem;
use std::ops::Drop;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::runtime;
use tokio::sync::{OnceCell, SetError};
use tokio::time;

async fn func1() -> u32 {
    5
}

async fn func2() -> u32 {
    time::sleep(Duration::from_millis(1)).await;
    10
}

async fn func_err() -> Result<u32, ()> {
    Err(())
}

async fn func_ok() -> Result<u32, ()> {
    Ok(10)
}

async fn func_panic() -> u32 {
    time::sleep(Duration::from_millis(1)).await;
    panic!();
}

async fn sleep_and_set() -> u32 {
    // Simulate sleep by pausing time and waiting for another thread to
    // resume clock when calling `set`, then finding the cell being initialized
    // by this call
    time::sleep(Duration::from_millis(2)).await;
    5
}

async fn advance_time_and_set(cell: &'static OnceCell<u32>, v: u32) -> Result<(), SetError<u32>> {
    time::advance(Duration::from_millis(1)).await;
    cell.set(v)
}

#[test]
fn get_or_init() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();

    static ONCE: OnceCell<u32> = OnceCell::const_new();

    rt.block_on(async {
        let handle1 = rt.spawn(async { ONCE.get_or_init(func1).await });
        let handle2 = rt.spawn(async { ONCE.get_or_init(func2).await });

        time::advance(Duration::from_millis(1)).await;
        time::resume();

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert_eq!(*result1, 5);
        assert_eq!(*result2, 5);
    });
}

#[test]
fn get_or_init_panic() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: OnceCell<u32> = OnceCell::const_new();

    rt.block_on(async {
        time::pause();

        let handle1 = rt.spawn(async { ONCE.get_or_init(func1).await });
        let handle2 = rt.spawn(async { ONCE.get_or_init(func_panic).await });

        time::advance(Duration::from_millis(1)).await;

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert_eq!(*result1, 5);
        assert_eq!(*result2, 5);
    });
}

#[test]
fn set_and_get() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: OnceCell<u32> = OnceCell::const_new();

    rt.block_on(async {
        let _ = rt.spawn(async { ONCE.set(5) }).await;
        let value = ONCE.get().unwrap();
        assert_eq!(*value, 5);
    });
}

#[test]
fn get_uninit() {
    static ONCE: OnceCell<u32> = OnceCell::const_new();
    let uninit = ONCE.get();
    assert!(uninit.is_none());
}

#[test]
fn set_twice() {
    static ONCE: OnceCell<u32> = OnceCell::const_new();

    let first = ONCE.set(5);
    assert_eq!(first, Ok(()));
    let second = ONCE.set(6);
    assert!(second.err().unwrap().is_already_init_err());
}

#[test]
fn set_while_initializing() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: OnceCell<u32> = OnceCell::const_new();

    rt.block_on(async {
        time::pause();

        let handle1 = rt.spawn(async { ONCE.get_or_init(sleep_and_set).await });
        let handle2 = rt.spawn(async { advance_time_and_set(&ONCE, 10).await });

        time::advance(Duration::from_millis(2)).await;

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert_eq!(*result1, 5);
        assert!(result2.err().unwrap().is_initializing_err());
    });
}

#[test]
fn get_or_try_init() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();

    static ONCE: OnceCell<u32> = OnceCell::const_new();

    rt.block_on(async {
        let handle1 = rt.spawn(async { ONCE.get_or_try_init(func_err).await });
        let handle2 = rt.spawn(async { ONCE.get_or_try_init(func_ok).await });

        time::advance(Duration::from_millis(1)).await;
        time::resume();

        let result1 = handle1.await.unwrap();
        assert!(result1.is_err());

        let result2 = handle2.await.unwrap();
        assert_eq!(*result2.unwrap(), 10);
    });
}

#[test]
fn drop_cell() {
    static NUM_DROPS: AtomicU32 = AtomicU32::new(0);

    struct Foo {}

    let fooer = Foo {};

    impl Drop for Foo {
        fn drop(&mut self) {
            NUM_DROPS.fetch_add(1, Ordering::Release);
        }
    }

    {
        let once_cell = OnceCell::new();
        let prev = once_cell.set(fooer);
        assert!(prev.is_ok())
    }
    assert!(NUM_DROPS.load(Ordering::Acquire) == 1);
}

#[test]
fn drop_cell_new_with() {
    static NUM_DROPS: AtomicU32 = AtomicU32::new(0);

    struct Foo {}

    let fooer = Foo {};

    impl Drop for Foo {
        fn drop(&mut self) {
            NUM_DROPS.fetch_add(1, Ordering::Release);
        }
    }

    {
        let once_cell = OnceCell::new_with(Some(fooer));
        assert!(once_cell.initialized());
    }
    assert!(NUM_DROPS.load(Ordering::Acquire) == 1);
}

#[test]
fn drop_into_inner() {
    static NUM_DROPS: AtomicU32 = AtomicU32::new(0);

    struct Foo {}

    let fooer = Foo {};

    impl Drop for Foo {
        fn drop(&mut self) {
            NUM_DROPS.fetch_add(1, Ordering::Release);
        }
    }

    let once_cell = OnceCell::new();
    assert!(once_cell.set(fooer).is_ok());
    let fooer = once_cell.into_inner();
    let count = NUM_DROPS.load(Ordering::Acquire);
    assert!(count == 0);
    drop(fooer);
    let count = NUM_DROPS.load(Ordering::Acquire);
    assert!(count == 1);
}

#[test]
fn drop_into_inner_new_with() {
    static NUM_DROPS: AtomicU32 = AtomicU32::new(0);

    struct Foo {}

    let fooer = Foo {};

    impl Drop for Foo {
        fn drop(&mut self) {
            NUM_DROPS.fetch_add(1, Ordering::Release);
        }
    }

    let once_cell = OnceCell::new_with(Some(fooer));
    let fooer = once_cell.into_inner();
    let count = NUM_DROPS.load(Ordering::Acquire);
    assert!(count == 0);
    mem::drop(fooer);
    let count = NUM_DROPS.load(Ordering::Acquire);
    assert!(count == 1);
}
