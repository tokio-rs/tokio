#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;
use tokio::sync::{AlreadyInitializedError, NotInitializedError, OnceCell};
use tokio::time::{sleep, Duration};

async fn func1() -> u32 {
    5
}

async fn func2() -> u32 {
    sleep(Duration::from_millis(10)).await;
    10
}

async fn func_panic() -> u32 {
    sleep(Duration::from_secs(1)).await;
    panic!();
}

#[test]
fn get_or_init_with() {
    let rt = Runtime::new().unwrap();

    static ONCE: OnceCell<u32> = OnceCell::new();

    rt.block_on(async {
        let result1 = rt
            .spawn(async { ONCE.get_or_init_with(func1).await })
            .await
            .unwrap();

        let result2 = rt
            .spawn(async { ONCE.get_or_init_with(func2).await })
            .await
            .unwrap();

        assert_eq!(*result1, 5);
        assert_eq!(*result2, 5);
    });
}

#[test]
fn get_or_init_panic() {
    let rt = Runtime::new().unwrap();

    static ONCE: OnceCell<u32> = OnceCell::new();

    rt.block_on(async {
        let result1 = rt
            .spawn(async { ONCE.get_or_init_with(func1).await })
            .await
            .unwrap();

        let result2 = rt
            .spawn(async { ONCE.get_or_init_with(func_panic).await })
            .await
            .unwrap();

        assert_eq!(*result1, 5);
        assert_eq!(*result2, 5);
    });
}

#[test]
fn set_and_get() {
    let rt = Runtime::new().unwrap();

    static ONCE: OnceCell<u32> = OnceCell::new();

    rt.block_on(async {
        let _ = rt.spawn(async { ONCE.set(5) }).await;
        let value = ONCE.get().unwrap();
        assert_eq!(*value, 5);
    });
}

#[test]
fn get_uninit() {
    static ONCE: OnceCell<u32> = OnceCell::new();
    let uninit = ONCE.get();
    assert_eq!(uninit, Err(NotInitializedError));
}

#[test]
fn set_twice() {
    static ONCE: OnceCell<u32> = OnceCell::new();

    let first = ONCE.set(5);
    assert_eq!(first, Ok(()));
    let second = ONCE.set(6);
    assert_eq!(second, Err(AlreadyInitializedError));
}
