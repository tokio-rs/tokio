#![allow(unknown_lints, unexpected_cfgs)]
#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), target_has_atomic = "64"))]

use tokio::runtime::Runtime;

#[test]
fn num_workers() {
    let rt = current_thread();
    assert_eq!(1, rt.metrics().num_workers());

    let rt = threaded();
    assert_eq!(2, rt.metrics().num_workers());
}

#[test]
fn num_blocking_threads() {
    let rt = current_thread();
    assert_eq!(0, rt.metrics().num_blocking_threads());
    let _ = rt.block_on(rt.spawn_blocking(move || {}));
    assert_eq!(1, rt.metrics().num_blocking_threads());

    let rt = threaded();
    assert_eq!(0, rt.metrics().num_blocking_threads());
    let _ = rt.block_on(rt.spawn_blocking(move || {}));
    assert_eq!(1, rt.metrics().num_blocking_threads());
}

#[test]
fn num_idle_blocking_threads() {
    let rt = current_thread();
    assert_eq!(0, rt.metrics().num_idle_blocking_threads());
    let _ = rt.block_on(rt.spawn_blocking(move || {}));
    rt.block_on(async {
        time::sleep(Duration::from_millis(5)).await;
    });

    // We need to wait until the blocking thread has become idle. Usually 5ms is
    // enough for this to happen, but not always. When it isn't enough, sleep
    // for another second. We don't always wait for a whole second since we want
    // the test suite to finish quickly.
    //
    // Note that the timeout for idle threads to be killed is 10 seconds.
    if 0 == rt.metrics().num_idle_blocking_threads() {
        rt.block_on(async {
            time::sleep(Duration::from_secs(1)).await;
        });
    }

    assert_eq!(1, rt.metrics().num_idle_blocking_threads());
}

#[test]
fn blocking_queue_depth() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();

    assert_eq!(0, rt.metrics().blocking_queue_depth());

    let ready = Arc::new(Mutex::new(()));
    let guard = ready.lock().unwrap();

    let ready_cloned = ready.clone();
    let wait_until_ready = move || {
        let _unused = ready_cloned.lock().unwrap();
    };

    let h1 = rt.spawn_blocking(wait_until_ready.clone());
    let h2 = rt.spawn_blocking(wait_until_ready);
    assert!(rt.metrics().blocking_queue_depth() > 0);

    drop(guard);

    let _ = rt.block_on(h1);
    let _ = rt.block_on(h2);

    assert_eq!(0, rt.metrics().blocking_queue_depth());
}

#[test]
fn num_alive_tasks() {
    let rt = current_thread();
    let metrics = rt.metrics();
    assert_eq!(0, metrics.num_alive_tasks());
    rt.block_on(rt.spawn(async move {
        assert_eq!(1, metrics.num_alive_tasks());
    }))
    .unwrap();

    assert_eq!(0, rt.metrics().num_alive_tasks());

    let rt = threaded();
    let metrics = rt.metrics();
    assert_eq!(0, metrics.num_alive_tasks());
    rt.block_on(rt.spawn(async move {
        assert_eq!(1, metrics.num_alive_tasks());
    }))
    .unwrap();

    // try for 10 seconds to see if this eventually succeeds.
    // wake_join() is called before the task is released, so in multithreaded
    // code, this means we sometimes exit the block_on before the counter decrements.
    for _ in 0..100 {
        if rt.metrics().num_alive_tasks() == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(0, rt.metrics().num_alive_tasks());
}

fn current_thread() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn threaded() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
