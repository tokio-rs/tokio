#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable))]

use std::panic;
use tokio::runtime::LocalOptions;
use tokio::task::spawn_local;
use tokio::task::LocalSet;

#[test]
fn test_spawn_local_in_runtime() {
    let rt = rt(LocalOptions::default());

    let res = rt.block_on(async move {
        let (tx, rx) = tokio::sync::oneshot::channel();

        spawn_local(async {
            tokio::task::yield_now().await;
            tx.send(5).unwrap();
        });

        rx.await.unwrap()
    });

    assert_eq!(res, 5);
}

#[test]
fn test_on_thread_park_unpark_in_runtime() {
    let mut opts = LocalOptions::default();

    // the refcell makes the below callbacks `!Send + !Sync`
    let on_park_called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let on_park_cc = on_park_called.clone();
    opts.on_thread_park(move || {
        *on_park_cc.borrow_mut() = true;
    });

    let on_unpark_called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let on_unpark_cc = on_unpark_called.clone();
    opts.on_thread_unpark(move || {
        *on_unpark_cc.borrow_mut() = true;
    });
    let rt = rt(opts);

    rt.block_on(async move {
        let (tx, rx) = tokio::sync::oneshot::channel();

        spawn_local(async {
            tokio::task::yield_now().await;
            tx.send(5).unwrap();
        });

        // this ensures on_thread_park is called
        rx.await.unwrap()
    });

    assert!(*on_park_called.borrow());
    assert!(*on_unpark_called.borrow());
}

#[test]
fn test_on_thread_park_unpark_in_handle() {
    let mut opts = LocalOptions::default();

    // the refcell makes the below callbacks `!Send + !Sync`
    let on_park_called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let on_park_cc = on_park_called.clone();
    opts.on_thread_park(move || {
        *on_park_cc.borrow_mut() = true;
    });

    let on_unpark_called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let on_unpark_cc = on_unpark_called.clone();
    opts.on_thread_unpark(move || {
        *on_unpark_cc.borrow_mut() = true;
    });
    let rt = rt(opts);

    rt.block_on(async move {
        tokio::task::yield_now().await;
    });

    // assert that the callbacks were not called - `Handle::block_on` can not drive IO or timer
    // drivers on a current-thread runtime, so the park/unpark callbacks should not be invoked.
    assert!(!*on_park_called.borrow());
    assert!(!*on_unpark_called.borrow());
}

#[test]
fn test_spawn_from_handle() {
    let rt = rt(LocalOptions::default());

    let (tx, rx) = tokio::sync::oneshot::channel();

    rt.handle().spawn(async {
        tokio::task::yield_now().await;
        tx.send(5).unwrap();
    });

    let res = rt.block_on(async move { rx.await.unwrap() });

    assert_eq!(res, 5);
}

#[test]
fn test_spawn_local_on_runtime_object() {
    let rt = rt(LocalOptions::default());

    let (tx, rx) = tokio::sync::oneshot::channel();

    rt.spawn_local(async {
        tokio::task::yield_now().await;
        tx.send(5).unwrap();
    });

    let res = rt.block_on(async move { rx.await.unwrap() });

    assert_eq!(res, 5);
}

#[test]
fn test_spawn_local_from_guard() {
    let rt = rt(LocalOptions::default());

    let (tx, rx) = tokio::sync::oneshot::channel();

    let _guard = rt.enter();

    spawn_local(async {
        tokio::task::yield_now().await;
        tx.send(5).unwrap();
    });

    let res = rt.block_on(async move { rx.await.unwrap() });

    assert_eq!(res, 5);
}

#[test]
#[cfg_attr(target_family = "wasm", ignore)] // threads not supported
fn test_spawn_from_guard_other_thread() {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let rt = rt(LocalOptions::default());
        let handle = rt.handle().clone();

        tx.send(handle).unwrap();
    });

    let handle = rx.recv().unwrap();

    let _guard = handle.enter();

    tokio::spawn(async {});
}

#[test]
#[should_panic = "Local tasks can only be spawned on a LocalRuntime from the thread the runtime was created on"]
#[cfg_attr(target_family = "wasm", ignore)] // threads not supported
fn test_spawn_local_from_guard_other_thread() {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let rt = rt(LocalOptions::default());
        let handle = rt.handle().clone();

        tx.send(handle).unwrap();
    });

    let handle = rx.recv().unwrap();

    let _guard = handle.enter();

    spawn_local(async {});
}

// This test guarantees that **`tokio::task::spawn_local` panics** when it is invoked
// from a thread that is *not* running the `LocalRuntime` / `LocalSet` to which
// the task would belong.
// The test creates a `LocalRuntime` and `LocalSet`, drives the `LocalSet` on the `LocalRuntime`'s thread,
// then spawns a **separate OS thread** and tries to call
// `tokio::task::spawn_local` there. `std::panic::catch_unwind` is then used
// to capture the panic and to assert that it indeed occurs.
#[test]
#[cfg_attr(target_family = "wasm", ignore)] // threads not supported
fn test_spawn_local_panic() {
    let rt = rt(LocalOptions::default());
    let local = LocalSet::new();

    rt.block_on(local.run_until(async {
        let thread_result = std::thread::spawn(|| {
            let panic_result = panic::catch_unwind(|| {
                let _jh = tokio::task::spawn_local(async {
                    println!("you will never see this line");
                });
            });
            assert!(panic_result.is_err(), "Expected panic, but none occurred");
        })
        .join();
        assert!(thread_result.is_ok(), "Thread itself panicked unexpectedly");
    }));
}

#[test]
#[should_panic = "`spawn_local` called from outside of a `task::LocalSet` or `runtime::LocalRuntime`"]
fn test_spawn_local_in_current_thread_runtime() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async move {
        spawn_local(async {});
    })
}

#[test]
#[should_panic = "`spawn_local` called from outside of a `task::LocalSet` or `runtime::LocalRuntime`"]
fn test_spawn_local_in_multi_thread_runtime() {
    let rt = tokio::runtime::Builder::new_multi_thread().build().unwrap();

    rt.block_on(async move {
        spawn_local(async {});
    })
}

fn rt(opts: LocalOptions) -> tokio::runtime::LocalRuntime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build_local(opts)
        .unwrap()
}
