#![warn(rust_2018_idioms)]
#![cfg(all(
    feature = "full",
    tokio_unstable,
    any(target_os = "linux", target_os = "android")
))]

//! `EventLoop` / `LocalEventLoop` driven the way a host would: `poll(2)` on
//! the reactor's descriptor, `drive` when it is readable. Readiness is the
//! whole contract, so the tests pin when the descriptor is and is not
//! readable as much as what a drive does.

use std::cell::Cell;
use std::os::fd::{AsFd, AsRawFd};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, EventLoop, LocalEventLoop};
use tokio::sync::oneshot;

/// `poll(2)` the descriptor for readability, waiting at most `timeout`.
fn readable(fd: &impl AsFd, timeout: Duration) -> bool {
    let mut pfd = libc::pollfd {
        fd: fd.as_fd().as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = timeout.as_millis().try_into().unwrap();
    let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
    assert!(rc >= 0, "poll: {}", std::io::Error::last_os_error());
    rc == 1
}

fn ready_now(fd: &impl AsFd) -> bool {
    readable(fd, Duration::ZERO)
}

/// A host loop: drive whenever readable, until `done`.
fn pump<E: AsFd>(el: &E, mut drive: impl FnMut(), mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5), "pump stalled");
        if readable(el, Duration::from_millis(100)) {
            drive();
        }
    }
}

fn event_loop() -> EventLoop {
    Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_event_loop()
        .unwrap()
}

fn local_event_loop() -> LocalEventLoop {
    Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_local_event_loop(Default::default())
        .unwrap()
}

#[test]
fn idle_is_not_readable() {
    let el = event_loop();
    assert!(!readable(&el, Duration::from_millis(20)));
}

#[test]
fn spawn_makes_readable_and_queues_until_driven() {
    let el = event_loop();
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
    assert!(ready_now(&el));

    el.drive();
    assert_eq!(ran.load(SeqCst), 1);
    assert!(jh.is_finished());
    assert!(
        !readable(&el, Duration::from_millis(20)),
        "idle after the drive"
    );
}

#[test]
fn busy_batch_stays_readable() {
    let el = event_loop();
    let turns = Arc::new(AtomicUsize::new(0));

    let t = turns.clone();
    let jh = el.spawn(async move {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            t.fetch_add(1, SeqCst);
        }
    });

    el.drive();
    // event_interval(4): the task yielded back into the queue, so the drive
    // left the descriptor readable for the next host turn rather than
    // running to completion.
    assert!(turns.load(SeqCst) < 20);
    assert!(ready_now(&el));

    pump(&el, || el.drive(), || jh.is_finished());
    assert_eq!(turns.load(SeqCst), 20);
}

#[test]
fn timer_arms_deadline() {
    let el = event_loop();
    let start = Instant::now();
    let jh = el.spawn(async { tokio::time::sleep(Duration::from_millis(50)).await });

    // Each readiness is a drive; the wheel may cascade a higher-level slot
    // early and re-arm, so count drives rather than forbidding them, and
    // pin that the task completes no earlier than its deadline without a
    // spin in between.
    let mut drives = 0;
    pump(
        &el,
        || {
            el.drive();
            drives += 1;
        },
        || jh.is_finished(),
    );
    assert!(start.elapsed() >= Duration::from_millis(50));
    assert!(drives <= 4, "{drives} drives for one timer");
    assert!(!readable(&el, Duration::from_millis(20)), "idle afterwards");
}

#[test]
fn nearer_timer_rearms() {
    let el = event_loop();
    let jh_far = el.spawn(async { tokio::time::sleep(Duration::from_secs(10)).await });
    el.drive();

    // A nearer timer registered from outside a drive wakes the driver; the
    // drive re-arms for the nearer deadline.
    let jh_near = el.spawn(async { tokio::time::sleep(Duration::from_millis(10)).await });
    assert!(ready_now(&el));
    let start = Instant::now();
    pump(&el, || el.drive(), || jh_near.is_finished());
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!jh_far.is_finished());
}

#[test]
fn cancelled_timer_disarms() {
    let el = event_loop();
    let jh = el.spawn(async {
        tokio::time::timeout(Duration::from_millis(30), std::future::ready(1))
            .await
            .unwrap()
    });
    el.drive();
    assert!(jh.is_finished());
    // The 30 ms deadline was dropped with the timeout: nothing fires.
    assert!(!readable(&el, Duration::from_millis(60)));
}

#[test]
fn wake_from_another_thread() {
    let el = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.spawn(async move { rx.await.unwrap() });
    el.drive();
    assert!(!jh.is_finished());
    assert!(!ready_now(&el));

    std::thread::spawn(move || tx.send(7).unwrap())
        .join()
        .unwrap();
    assert!(ready_now(&el));
    el.drive();
    assert!(jh.is_finished());
    assert_eq!(el.handle().block_on(jh).unwrap(), 7);
}

#[test]
fn cross_loop_wake() {
    let a = event_loop();
    let b = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();

    let jh_b = b.spawn(async move { rx.await.unwrap() });
    let jh_a = a.spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        tx.send(9).unwrap();
    });
    let start = Instant::now();
    while !(jh_a.is_finished() && jh_b.is_finished()) {
        assert!(start.elapsed() < Duration::from_secs(5));
        if ready_now(&a) {
            a.drive();
        }
        if ready_now(&b) {
            b.drive();
        }
    }
}

#[test]
fn tcp_round_trip() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    let server = event_loop();
    let client = event_loop();
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
        if readable(&server, Duration::from_millis(10)) {
            server.drive();
        }
        if readable(&client, Duration::from_millis(10)) {
            client.drive();
        }
    }
}

#[test]
fn drop_cancels_tasks() {
    struct Flag(Arc<AtomicUsize>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.fetch_add(1, SeqCst);
        }
    }

    let el = event_loop();
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
fn local_event_loop_spawn_local() {
    let el = local_event_loop();
    let value = Rc::new(Cell::new(0));
    let v = value.clone();
    let jh = el.spawn_local(async move {
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        v.set(5);
    });
    pump(&el, || el.drive(), || jh.is_finished());
    assert_eq!(value.get(), 5);
}

#[test]
fn local_event_loop_rejects_foreign_thread_drive() {
    let el = local_event_loop();
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
    let el = Arc::new(event_loop());
    let el2 = el.clone();
    let jh = el.spawn(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el2.drive())).is_err()
    });
    el.drive();
    assert!(el.handle().block_on(jh).unwrap());
}

#[test]
fn io_driver_required() {
    let err = Builder::new_current_thread()
        .enable_time()
        .build_event_loop()
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
}

#[test]
fn without_time_driver() {
    let el = Builder::new_current_thread()
        .enable_io()
        .build_event_loop()
        .unwrap();
    let jh = el.spawn(async { 1 });
    el.drive();
    assert!(jh.is_finished());
}

fn _assert_bounds() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<EventLoop>();
}
