#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::runtime;
use tokio::sync::SetError;
use tokio::sync::SetOnce;
use tokio::time;

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
        let once_cell = SetOnce::new();
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
        let once_cell = SetOnce::new_with(Some(fooer));
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

    let once_cell = SetOnce::new();
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

    let once_cell = SetOnce::new_with(Some(fooer));
    let fooer = once_cell.into_inner();
    let count = NUM_DROPS.load(Ordering::Acquire);
    assert!(count == 0);
    mem::drop(fooer);
    let count = NUM_DROPS.load(Ordering::Acquire);
    assert!(count == 1);
}

#[test]
fn from() {
    let cell = SetOnce::from(2);
    assert_eq!(*cell.get().unwrap(), 2);
}

#[test]
fn set_and_get() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: SetOnce<u32> = SetOnce::const_new();

    rt.block_on(async {
        let _ = rt.spawn(async { ONCE.set(5) }).await;
        let value = ONCE.get().unwrap();
        assert_eq!(*value, 5);
    });
}

#[test]
fn get_uninit() {
    static ONCE: SetOnce<u32> = SetOnce::const_new();
    let uninit = ONCE.get();
    assert!(uninit.is_none());
}

#[test]
fn set_twice() {
    static ONCE: SetOnce<u32> = SetOnce::const_new();

    let first = ONCE.set(5);
    assert_eq!(first, Ok(()));
    let second = ONCE.set(6);
    assert!(second.err().unwrap().is_already_init_err());
}

#[test]
fn set_while_initializing_or_already_init() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: SetOnce<u32> = SetOnce::const_new();

    rt.block_on(async {
        let handle1 = rt.spawn(async {
            time::sleep(Duration::from_millis(1)).await;
            let set_val = 5;
            ONCE.set(set_val)?;

            Ok::<u32, SetError<u32>>(set_val)
        });

        let handle2 = rt.spawn(async {
            time::sleep(Duration::from_millis(1)).await;
            let set_val = 10;
            ONCE.set(set_val)?;

            Ok::<u32, SetError<u32>>(set_val)
        });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert_eq!(result1, Ok(5));
        let err = result2.err().unwrap();

        assert!(err.is_initializing_err() || err.is_already_init_err());
    });
}

#[test]
fn is_none_initializing() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: SetOnce<u32> = SetOnce::const_new();

    rt.block_on(async {
        time::pause();

        tokio::spawn(async { ONCE.set(20) });

        tokio::spawn(async { ONCE.set(10) });

        let res = ONCE.get();

        assert_eq!(res, None);
    });
}

#[test]
fn is_some_initializing() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: SetOnce<u32> = SetOnce::const_new();

    rt.block_on(async {
        time::pause();

        tokio::spawn(async { ONCE.set(20) });

        ONCE.wait().await;

        let res = ONCE.get();

        assert_eq!(res, Some(20).as_ref());
    });
}
