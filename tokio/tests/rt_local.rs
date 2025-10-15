#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable))]

use tokio::runtime::LocalOptions;
use tokio::task::spawn_local;

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

fn rt(opts: LocalOptions) -> tokio::runtime::LocalRuntime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build_local(opts)
        .unwrap()
}
