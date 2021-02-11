#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;
use tokio::sync::{AlreadyInitializedError, Lazy, NotInitializedError, OnceCell};
use tokio::time::{pause, sleep, Duration, Instant};

use std::future::Future;
use std::pin::Pin;

async fn func1() -> u32 {
    5
}

async fn func2() -> u32 {
    sleep(Duration::from_secs(1)).await;
    10
}

fn func_lazy() -> Pin<Box<dyn Future<Output = u32> + Send>> {
    async fn f() -> u32 {
        sleep(Duration::from_secs(1)).await;
        10
    }
    Box::pin(f())
}

async fn func_panic() -> u32 {
    sleep(Duration::from_secs(1)).await;
    panic!();
}

#[test]
fn get_or_init() {
    let rt = Runtime::new().unwrap();

    static ONCE: OnceCell<u32> = OnceCell::new();

    rt.block_on(async {
        let result1 = rt
            .spawn(async { ONCE.get_or_init(func1).await })
            .await
            .unwrap();

        let result2 = rt
            .spawn(async { ONCE.get_or_init(func2).await })
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
            .spawn(async { ONCE.get_or_init(func1).await })
            .await
            .unwrap();

        let result2 = rt
            .spawn(async { ONCE.get_or_init(func_panic).await })
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

#[test]
fn lazy() {
    // func2 sleeps for 1 second
    static LAZY: Lazy<u32> = Lazy::new(func_lazy);
    let rt = Runtime::new().unwrap();
    pause();

    rt.block_on(async {
        let start_time1 = Instant::now();
        let _result1 = rt.spawn(async { LAZY.get().await }).await;
        let execution_time1 = Instant::now().duration_since(start_time1).as_secs();

        let start_time2 = Instant::now();
        let _result2 = rt.spawn(async { LAZY.get().await }).await;
        let execution_time2 = Instant::now().duration_since(start_time2).as_secs();

        assert!(execution_time1 >= 1);
        assert!(execution_time2 < 1);
    });
}
