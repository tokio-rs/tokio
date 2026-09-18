#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, not(target_family = "wasm")))]

//! `LocalEventLoop` driven the way a host would: a waker that
//! flags "drive me", and a host loop that drives when flagged. The waker is
//! the whole contract, so the tests pin when it is and is not woken as much
//! as what a drive does.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Wake, Waker};
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, LocalEventLoop, WouldBlock};
use tokio::sync::oneshot;

/// The host's side of the contract: a flag the runtime raises.
#[derive(Default)]
struct Host {
    woken: Mutex<bool>,
    cv: Condvar,
    wakes: AtomicUsize,
}

impl Wake for Host {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, SeqCst);
        *self.woken.lock().unwrap() = true;
        self.cv.notify_all();
    }
}

impl Host {
    fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }

    /// Waits up to `timeout` for a wake, consuming it.
    fn wait(&self, timeout: Duration) -> bool {
        let guard = self.woken.lock().unwrap();
        let (mut woken, _) = self.cv.wait_timeout_while(guard, timeout, |w| !*w).unwrap();
        std::mem::replace(&mut *woken, false)
    }

    fn woken_now(&self) -> bool {
        self.wait(Duration::ZERO)
    }

    /// Peeks without consuming.
    fn is_woken(&self) -> bool {
        *self.woken.lock().unwrap()
    }

    /// A host loop: drive whenever woken, until `done`.
    fn pump(&self, mut drive: impl FnMut(), mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "pump stalled");
            if self.wait(Duration::from_millis(100)) {
                drive();
            }
        }
    }
}

fn event_loop() -> (Arc<Host>, LocalEventLoop) {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_local_event_loop(Default::default(), host.waker())
        .unwrap();
    (host, el)
}

#[test]
fn idle_does_not_wake() {
    let (host, _el) = event_loop();
    assert!(!host.wait(Duration::from_millis(20)));
}

#[test]
fn spawn_wakes_and_queues_until_driven() {
    let (host, el) = event_loop();
    let ran = Arc::new(AtomicUsize::new(0));

    let ran2 = ran.clone();
    let jh = el.spawn_local(async move {
        ran2.fetch_add(1, SeqCst);
        3
    });
    assert_eq!(
        ran.load(SeqCst),
        0,
        "spawn must not run on the caller's stack"
    );
    assert!(host.woken_now());

    el.drive();
    assert_eq!(ran.load(SeqCst), 1);
    assert!(jh.is_finished());
    assert!(
        !host.wait(Duration::from_millis(20)),
        "idle after the drive"
    );
}

#[test]
fn busy_batch_wakes_again() {
    let (host, el) = event_loop();
    let turns = Arc::new(AtomicUsize::new(0));

    let t = turns.clone();
    let jh = el.spawn_local(async move {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            t.fetch_add(1, SeqCst);
        }
    });
    assert!(host.woken_now());

    el.drive();
    // event_interval(4): the task yielded back into the queue, so the drive
    // woke the host for the next turn rather than running to completion.
    assert!(turns.load(SeqCst) < 20);
    assert!(host.is_woken());

    host.pump(|| el.drive(), || jh.is_finished());
    assert_eq!(turns.load(SeqCst), 20);
}

#[test]
fn inject_leftovers_wake_again() {
    // Spawns from host context land in the inject queue. A batch that ends
    // at `event_interval` with more of them queued must wake the host: the
    // wake those spawns caused was consumed by this drive.
    let (host, el) = event_loop();
    let ran = Arc::new(AtomicUsize::new(0));
    for _ in 0..6 {
        let r = ran.clone();
        el.spawn_local(async move {
            r.fetch_add(1, SeqCst);
        });
    }
    assert!(host.woken_now());
    el.drive();
    assert_eq!(ran.load(SeqCst), 4, "one event_interval(4) batch");
    assert!(
        host.is_woken(),
        "the remaining two are queued and need a drive"
    );
    host.pump(|| el.drive(), || ran.load(SeqCst) == 6);
}

#[test]
fn block_on_leaving_inject_tasks_wakes() {
    let (host, el) = event_loop();
    let ran = Arc::new(AtomicUsize::new(0));
    for _ in 0..6 {
        let r = ran.clone();
        el.spawn_local(async move {
            r.fetch_add(1, SeqCst);
        });
    }
    // The future is ready first, before any batch runs.
    assert_eq!(el.block_on(async { 1 }), Ok(1));
    assert!(ran.load(SeqCst) < 6);
    assert!(host.is_woken());
    host.pump(|| el.drive(), || ran.load(SeqCst) == 6);
}

#[test]
fn timer_wakes_at_deadline() {
    let (host, el) = event_loop();
    let start = Instant::now();
    let jh = el.spawn_local(async { tokio::time::sleep(Duration::from_millis(50)).await });

    host.pump(|| el.drive(), || jh.is_finished());
    assert!(start.elapsed() >= Duration::from_millis(50));
    // The spawn, the deadline, and possibly the driver's self-unpark.
    assert!(
        host.wakes.load(SeqCst) <= 4,
        "{} wakes",
        host.wakes.load(SeqCst)
    );
    assert!(!host.wait(Duration::from_millis(20)), "idle afterwards");
}

#[test]
fn nearer_timer_rearms() {
    let (host, el) = event_loop();
    let jh_far = el.spawn_local(async { tokio::time::sleep(Duration::from_secs(10)).await });
    assert!(host.woken_now());
    el.drive();

    // The driver is parked for the far deadline; a nearer timer must
    // re-arm it rather than wait it out.
    let jh_near = el.spawn_local(async { tokio::time::sleep(Duration::from_millis(10)).await });
    let start = Instant::now();
    host.pump(|| el.drive(), || jh_near.is_finished());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!jh_far.is_finished());
}

#[test]
fn cancelled_timer_does_not_wake() {
    let (host, el) = event_loop();
    let jh = el.spawn_local(async {
        tokio::time::timeout(Duration::from_millis(30), std::future::ready(1))
            .await
            .unwrap()
    });
    assert!(host.woken_now());
    el.drive();
    assert!(jh.is_finished());
    // The 30 ms deadline was dropped with the timeout: nothing fires.
    assert!(!host.wait(Duration::from_millis(60)));
}

#[test]
fn wake_from_another_thread() {
    let (host, el) = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.spawn_local(async move { rx.await.unwrap() });
    assert!(host.woken_now());
    el.drive();
    assert!(!jh.is_finished());
    assert!(!host.woken_now());

    std::thread::spawn(move || tx.send(7).unwrap())
        .join()
        .unwrap();
    assert!(host.woken_now());
    el.drive();
    assert!(jh.is_finished());
    assert_eq!(el.block_on(jh).unwrap().unwrap(), 7);
}

#[test]
fn cross_loop_wake() {
    let (host_a, a) = event_loop();
    let (host_b, b) = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();

    let jh_b = b.spawn_local(async move { rx.await.unwrap() });
    let jh_a = a.spawn_local(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        tx.send(9).unwrap();
    });
    let start = Instant::now();
    while !(jh_a.is_finished() && jh_b.is_finished()) {
        assert!(start.elapsed() < Duration::from_secs(5));
        if host_a.wait(Duration::from_millis(10)) {
            a.drive();
        }
        if host_b.wait(Duration::from_millis(10)) {
            b.drive();
        }
    }
}

#[test]
fn tcp_round_trip() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    let (host_s, server) = event_loop();
    let (host_c, client) = event_loop();
    let (addr_tx, addr_rx) = oneshot::channel();

    let jh_s = server.spawn_local(async move {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
        sock.write_all(b"world").await.unwrap();
    });
    let jh_c = client.spawn_local(async move {
        let addr = addr_rx.await.unwrap();
        let mut sock = TcpStream::connect(addr).await.unwrap();
        sock.write_all(b"hello").await.unwrap();
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");
    });

    let start = Instant::now();
    while !(jh_s.is_finished() && jh_c.is_finished()) {
        assert!(start.elapsed() < Duration::from_secs(5));
        if host_s.wait(Duration::from_millis(10)) {
            server.drive();
        }
        if host_c.wait(Duration::from_millis(10)) {
            client.drive();
        }
    }
}

#[test]
fn block_on_ready_future() {
    let (_host, el) = event_loop();
    assert_eq!(el.block_on(async { 1 + 2 }), Ok(3));
}

#[test]
fn block_on_drives_ready_tasks() {
    // Spawned tasks that complete synchronously, and the block_on future
    // awaiting them, all run within the one call: nothing waits.
    let (host, el) = event_loop();
    let out = el.block_on(async {
        let a = tokio::spawn(async { 20 });
        let b = tokio::spawn(async {
            tokio::task::yield_now().await;
            22
        });
        a.await.unwrap() + b.await.unwrap()
    });
    assert_eq!(out, Ok(42));
    // The spawns woke the host; a drive now finds nothing.
    el.drive();
    assert!(!host.wait(Duration::from_millis(20)));
}

#[test]
fn block_on_leaving_ready_tasks_wakes() {
    // The future completes while a spawned task is still queued: the host
    // must be woken to run it, as after a busy drive.
    let (host, el) = event_loop();
    let turns = Arc::new(AtomicUsize::new(0));
    let t = turns.clone();
    let out = el.block_on(async move {
        tokio::spawn(async move {
            for _ in 0..10 {
                tokio::task::yield_now().await;
                t.fetch_add(1, SeqCst);
            }
        });
        1
    });
    assert_eq!(out, Ok(1));
    assert!(host.is_woken());
    host.pump(|| el.drive(), || turns.load(SeqCst) == 10);
}

#[test]
fn block_on_would_block_on_timer() {
    let (host, el) = event_loop();
    let start = Instant::now();
    let res = el.block_on(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        1
    });
    let err: WouldBlock = res.unwrap_err();
    assert_eq!(
        err.to_string(),
        "the future did not complete without blocking"
    );
    assert!(start.elapsed() < Duration::from_millis(50), "must not wait");

    // The runtime is intact: spawned work still runs.
    let jh = el.spawn_local(async { 2 });
    host.pump(|| el.drive(), || jh.is_finished());
}

#[test]
fn block_on_would_block_leaves_spawned_tasks() {
    let (host, el) = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let res = el.block_on(async {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            tx.send(5).unwrap();
        });
        std::future::pending::<()>().await;
    });
    assert!(res.is_err());

    // The task the future spawned outlives it and completes from drives.
    let jh = el.spawn_local(async move { rx.await.unwrap() });
    host.pump(|| el.drive(), || jh.is_finished());
    assert_eq!(el.block_on(jh).unwrap().unwrap(), 5);
}

#[test]
fn block_on_inside_runtime_panics() {
    let (_host, el) = event_loop();
    let el = Rc::new(el);
    let el2 = el.clone();
    let res = el.block_on(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el2.block_on(async {}))).is_err()
    });
    assert_eq!(res, Ok(true));
}

#[test]
fn unhandled_panic_shutdown_panics_drive() {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .unhandled_panic(tokio::runtime::UnhandledPanic::ShutdownRuntime)
        .build_local_event_loop(Default::default(), host.waker())
        .unwrap();
    el.spawn_local(async { panic!("task panicked") });
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el.drive()));
    assert!(res.is_err(), "drive reports the shutdown as a panic");
}

#[test]
fn drop_cancels_tasks() {
    struct Flag(Arc<AtomicUsize>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.fetch_add(1, SeqCst);
        }
    }

    let (_host, el) = event_loop();
    let dropped = Arc::new(AtomicUsize::new(0));
    let flag = Flag(dropped.clone());
    el.spawn_local(async move {
        let _flag = flag;
        std::future::pending::<()>().await;
    });
    el.drive();
    assert_eq!(dropped.load(SeqCst), 0);
    drop(el);
    assert_eq!(dropped.load(SeqCst), 1, "drop shuts the runtime down");
}

#[test]
fn drop_while_parked_on_io_and_timer() {
    // The driver thread is parked in the reactor with a timer armed and a
    // listener registered; drop must unpark and join it.
    let (host, el) = event_loop();
    let jh = el.spawn_local(async {
        let _listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    assert!(host.woken_now());
    el.drive();
    assert!(!jh.is_finished());
    let start = Instant::now();
    drop(el);
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn spawn_from_another_thread_via_handle() {
    let (host, el) = event_loop();
    let handle = el.handle().clone();
    std::thread::spawn(move || {
        handle.spawn(async { 4 });
    })
    .join()
    .unwrap();
    assert!(host.woken_now());
    el.drive();
    assert_eq!(el.handle().metrics().num_alive_tasks(), 0);
}

#[test]
fn spawn_local_with_rc() {
    let (host, el) = event_loop();
    let value = Rc::new(Cell::new(0));
    let v = value.clone();
    let jh = el.spawn_local(async move {
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        v.set(5);
    });
    host.pump(|| el.drive(), || jh.is_finished());
    assert_eq!(value.get(), 5);
    assert_eq!(el.block_on(async { value.get() }), Ok(5));
}

#[test]
fn rejects_foreign_thread_drive() {
    let (_host, el) = event_loop();
    struct SendPtr(*const LocalEventLoop);
    unsafe impl Send for SendPtr {}
    let ptr = SendPtr(&el);
    let res = std::thread::spawn(move || {
        let ptr = ptr;
        // SAFETY: `el` outlives the join below; the drive is expected to
        // panic on the thread check before touching the runtime.
        unsafe { (*ptr.0).drive() }
    })
    .join();
    assert!(res.is_err());
}

#[test]
fn drive_inside_runtime_panics() {
    let (_host, el) = event_loop();
    let el = Rc::new(el);
    let el2 = el.clone();
    let jh = el.spawn_local(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el2.drive())).is_err()
    });
    el.drive();
    assert!(el.block_on(jh).unwrap().unwrap());
}

#[test]
fn without_io_driver() {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .enable_time()
        .build_local_event_loop(Default::default(), host.waker())
        .unwrap();
    let jh = el.spawn_local(async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        1
    });
    host.pump(|| el.drive(), || jh.is_finished());
}

#[test]
fn without_any_driver() {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .build_local_event_loop(Default::default(), host.waker())
        .unwrap();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.spawn_local(async move { rx.await.unwrap() });
    el.drive();
    std::thread::spawn(move || tx.send(3).unwrap())
        .join()
        .unwrap();
    host.pump(|| el.drive(), || jh.is_finished());
    assert_eq!(el.block_on(jh).unwrap().unwrap(), 3);
}

// `LocalEventLoop` is `!Send` and `!Sync`, like `LocalRuntime`: the method
// resolves to the blanket impl only when the auto trait is absent.
#[allow(dead_code)]
fn assert_not_send_sync() {
    trait AmbiguousIfSend<A> {
        fn some_item(&self) {}
    }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
    trait AmbiguousIfSync<A> {
        fn some_item(&self) {}
    }
    impl<T: ?Sized> AmbiguousIfSync<()> for T {}
    impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
    fn check(el: &LocalEventLoop) {
        AmbiguousIfSend::some_item(el);
        AmbiguousIfSync::some_item(el);
    }
    let _ = check;
}
