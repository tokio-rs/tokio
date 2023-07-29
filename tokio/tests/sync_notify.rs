#![warn(rust_2018_idioms)]
#![cfg(feature = "sync")]

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
use wasm_bindgen_test::wasm_bindgen_test as test;

use tokio::sync::Notify;
use tokio_test::task::spawn;
use tokio_test::*;

trait AssertSend: Send + Sync {}
impl AssertSend for Notify {}

#[test]
fn notify_notified_one() {
    let notify = Notify::new();
    let mut notified = spawn(async { notify.notified().await });

    notify.notify_one();
    assert_ready!(notified.poll());
}

#[test]
fn notified_one_notify() {
    let notify = Notify::new();
    let mut notified = spawn(async { notify.notified().await });

    assert_pending!(notified.poll());

    notify.notify_one();
    assert!(notified.is_woken());
    assert_ready!(notified.poll());
}

#[test]
fn notified_multi_notify() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());
    assert_pending!(notified2.poll());

    notify.notify_one();
    assert!(notified1.is_woken());
    assert!(!notified2.is_woken());

    assert_ready!(notified1.poll());
    assert_pending!(notified2.poll());
}

#[test]
fn notify_notified_multi() {
    let notify = Notify::new();

    notify.notify_one();

    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_ready!(notified1.poll());
    assert_pending!(notified2.poll());

    notify.notify_one();

    assert!(notified2.is_woken());
    assert_ready!(notified2.poll());
}

#[test]
fn notified_drop_notified_notify() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());

    drop(notified1);

    assert_pending!(notified2.poll());

    notify.notify_one();
    assert!(notified2.is_woken());
    assert_ready!(notified2.poll());
}

#[test]
fn notified_multi_notify_drop_one() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });
    let mut notified2 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());
    assert_pending!(notified2.poll());

    notify.notify_one();

    assert!(notified1.is_woken());
    assert!(!notified2.is_woken());

    drop(notified1);

    assert!(notified2.is_woken());
    assert_ready!(notified2.poll());
}

#[test]
fn notify_in_drop_after_wake() {
    use futures::task::ArcWake;
    use std::future::Future;
    use std::sync::Arc;

    let notify = Arc::new(Notify::new());

    struct NotifyOnDrop(Arc<Notify>);

    impl ArcWake for NotifyOnDrop {
        fn wake_by_ref(_arc_self: &Arc<Self>) {}
    }

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            self.0.notify_waiters();
        }
    }

    let mut fut = Box::pin(async {
        notify.notified().await;
    });

    {
        let waker = futures::task::waker(Arc::new(NotifyOnDrop(notify.clone())));
        let mut cx = std::task::Context::from_waker(&waker);
        assert!(fut.as_mut().poll(&mut cx).is_pending());
    }

    // Now, notifying **should not** deadlock
    notify.notify_waiters();
}

#[test]
fn notify_one_after_dropped_all() {
    let notify = Notify::new();
    let mut notified1 = spawn(async { notify.notified().await });

    assert_pending!(notified1.poll());

    notify.notify_waiters();
    notify.notify_one();

    drop(notified1);

    let mut notified2 = spawn(async { notify.notified().await });

    assert_ready!(notified2.poll());
}

#[test]
fn test_notify_one_not_enabled() {
    let notify = Notify::new();
    let mut future = spawn(notify.notified());

    notify.notify_one();
    assert_ready!(future.poll());
}

#[test]
fn test_notify_one_after_enable() {
    let notify = Notify::new();
    let mut future = spawn(notify.notified());

    future.enter(|_, fut| assert!(!fut.enable()));

    notify.notify_one();
    assert_ready!(future.poll());
    future.enter(|_, fut| assert!(fut.enable()));
}

#[test]
fn test_poll_after_enable() {
    let notify = Notify::new();
    let mut future = spawn(notify.notified());

    future.enter(|_, fut| assert!(!fut.enable()));
    assert_pending!(future.poll());
}

#[test]
fn test_enable_after_poll() {
    let notify = Notify::new();
    let mut future = spawn(notify.notified());

    assert_pending!(future.poll());
    future.enter(|_, fut| assert!(!fut.enable()));
}

#[test]
fn test_enable_consumes_permit() {
    let notify = Notify::new();

    // Add a permit.
    notify.notify_one();

    let mut future1 = spawn(notify.notified());
    future1.enter(|_, fut| assert!(fut.enable()));

    let mut future2 = spawn(notify.notified());
    future2.enter(|_, fut| assert!(!fut.enable()));
}

#[test]
fn test_waker_update() {
    use futures::task::noop_waker;
    use std::future::Future;
    use std::task::Context;

    let notify = Notify::new();
    let mut future = spawn(notify.notified());

    let noop = noop_waker();
    future.enter(|_, fut| assert_pending!(fut.poll(&mut Context::from_waker(&noop))));

    assert_pending!(future.poll());
    notify.notify_one();

    assert!(future.is_woken());
}
