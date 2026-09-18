#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, not(target_family = "wasm")))]

//! `EventLoop` / `LocalEventLoop` driven the way a host would: a waker that
//! flags "drive me", and a host loop that drives when flagged. The waker is
//! the whole contract, so the tests pin when it is and is not woken as much
//! as what a drive does.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Wake, Waker};
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, EventLoop, LocalEventLoop, WouldBlock};
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

fn event_loop() -> (Arc<Host>, EventLoop) {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_event_loop(host.waker())
        .unwrap();
    (host, el)
}

fn local_event_loop() -> (Arc<Host>, LocalEventLoop) {
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
    let jh = el.spawn(async move {
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
    let jh = el.spawn(async move {
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
fn timer_wakes_at_deadline() {
    let (host, el) = event_loop();
    let start = Instant::now();
    let jh = el.spawn(async { tokio::time::sleep(Duration::from_millis(50)).await });

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
    let jh_far = el.spawn(async { tokio::time::sleep(Duration::from_secs(10)).await });
    assert!(host.woken_now());
    el.drive();

    // The driver is parked for the far deadline; a nearer timer must
    // re-arm it rather than wait it out.
    let jh_near = el.spawn(async { tokio::time::sleep(Duration::from_millis(10)).await });
    let start = Instant::now();
    host.pump(|| el.drive(), || jh_near.is_finished());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!jh_far.is_finished());
}

#[test]
fn cancelled_timer_does_not_wake() {
    let (host, el) = event_loop();
    let jh = el.spawn(async {
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
    let jh = el.spawn(async move { rx.await.unwrap() });
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

    let jh_b = b.spawn(async move { rx.await.unwrap() });
    let jh_a = a.spawn(async move {
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

    let jh_s = server.spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
        sock.write_all(b"world").await.unwrap();
    });
    let jh_c = client.spawn(async move {
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
    let jh = el.spawn(async { 2 });
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
    let jh = el.spawn(async move { rx.await.unwrap() });
    host.pump(|| el.drive(), || jh.is_finished());
    assert_eq!(el.block_on(jh).unwrap().unwrap(), 5);
}

#[test]
fn block_on_inside_runtime_panics() {
    let (_host, el) = event_loop();
    let el = Arc::new(el);
    let el2 = el.clone();
    let res = el.block_on(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el2.block_on(async {}))).is_err()
    });
    assert_eq!(res, Ok(true));
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
    el.spawn(async move {
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
    let jh = el.spawn(async {
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
fn local_event_loop_spawn_local() {
    let (host, el) = local_event_loop();
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
fn local_event_loop_rejects_foreign_thread_drive() {
    let (_host, el) = local_event_loop();
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
    let el = Arc::new(el);
    let el2 = el.clone();
    let jh = el.spawn(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el2.drive())).is_err()
    });
    el.drive();
    assert_eq!(el.block_on(jh).unwrap().unwrap(), true);
}

#[test]
fn without_io_driver() {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .enable_time()
        .build_event_loop(host.waker())
        .unwrap();
    let jh = el.spawn(async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        1
    });
    host.pump(|| el.drive(), || jh.is_finished());
}

#[test]
fn without_any_driver() {
    let host = Arc::new(Host::default());
    let el = Builder::new_current_thread()
        .build_event_loop(host.waker())
        .unwrap();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.spawn(async move { rx.await.unwrap() });
    el.drive();
    std::thread::spawn(move || tx.send(3).unwrap())
        .join()
        .unwrap();
    host.pump(|| el.drive(), || jh.is_finished());
    assert_eq!(el.block_on(jh).unwrap().unwrap(), 3);
}

fn _assert_bounds() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<EventLoop>();
}
