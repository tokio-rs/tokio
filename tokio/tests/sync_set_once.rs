#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::mem;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::runtime;
use tokio::sync::{SetOnce, SetOnceError as SetError};
use tokio::time;

struct Foo {
    pub drops: Arc<AtomicU32>,
}

impl Foo {
    pub fn new(drops: Arc<AtomicU32>) -> Self {
        Foo { drops }
    }
}

impl Drop for Foo {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Release);
    }
}

#[test]
fn drop_cell() {
    let drops = Arc::new(AtomicU32::new(0));

    let fooer = Foo::new(Arc::clone(&drops));

    {
        let once_cell = SetOnce::new();
        let prev = once_cell.set(fooer);
        assert!(prev.is_ok())
    }
    assert!(drops.load(Ordering::Acquire) == 1);
}

#[test]
fn drop_cell_new_with() {
    let drops = Arc::new(AtomicU32::new(0));
    let fooer = Foo::new(Arc::clone(&drops));

    {
        let once_cell = SetOnce::new_with(Some(fooer));
        assert!(once_cell.initialized());
    }

    assert!(drops.load(Ordering::Acquire) == 1);
}

#[test]
fn drop_into_inner() {
    let drops = Arc::new(AtomicU32::new(0));
    let fooer = Foo::new(Arc::clone(&drops));

    let once_cell = SetOnce::new();
    assert!(once_cell.set(fooer).is_ok());
    let val = once_cell.into_inner();
    let count = drops.load(Ordering::Acquire);
    assert!(count == 0);
    drop(val);
    let count = drops.load(Ordering::Acquire);
    assert!(count == 1);
}

#[test]
fn drop_into_inner_new_with() {
    let drops = Arc::new(AtomicU32::new(0));
    let fooer = Foo::new(Arc::clone(&drops));

    let once_cell = SetOnce::new_with(Some(fooer));
    let fooer = once_cell.into_inner();
    let count = drops.load(Ordering::Acquire);
    assert!(count == 0);
    mem::drop(fooer);
    let count = drops.load(Ordering::Acquire);
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
fn set_and_wait() {
    let rt = runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();

    static ONCE: SetOnce<u32> = SetOnce::const_new();

    rt.block_on(async {
        let _ = rt.spawn(async { ONCE.set(5) }).await;
        let value = ONCE.wait().await;
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
    assert!(second.is_err());
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

        assert!(result2.is_err());
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
    let rt = runtime::Builder::new_current_thread().build().unwrap();

    static ONCE: SetOnce<u32> = SetOnce::const_new();

    rt.block_on(async {
        tokio::spawn(async { ONCE.set(20) });

        assert_eq!(*ONCE.wait().await, 20);
    });
}
