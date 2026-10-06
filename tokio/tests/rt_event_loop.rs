#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, any(unix, windows)))]

//! `LocalEventLoop` driven the way a host would: a wait on the event loop's
//! reactor handle (`poll(2)` on the descriptor, `WaitForSingleObject` on the
//! completion port) with `next_timeout` as the timeout, then `drive`. The
//! handle and the timeout are the whole contract, so the tests pin when the
//! handle is and is not signaled as much as what a drive does.

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, LocalEventLoop};
use tokio::sync::oneshot;

/// The host's side of the contract.
trait Host {
    /// Waits up to `timeout` for the reactor handle to be signaled.
    fn readable(&self, timeout: Duration) -> bool;

    fn readable_now(&self) -> bool {
        self.readable(Duration::ZERO)
    }

    /// Waits for the handle or the next timer, whichever is first.
    fn wait(&self, limit: Duration) -> bool;

    /// A host loop: wait, drive, until `done`.
    fn pump(&self, done: impl FnMut() -> bool);
}

/// Waits on several event loops at once, as one host does. Returns which are
/// signaled.
fn wait_any(els: &[&LocalEventLoop], timeout: Duration) -> Vec<bool> {
    #[cfg(unix)]
    {
        let mut fds: Vec<libc::pollfd> = els
            .iter()
            .map(|el| libc::pollfd {
                fd: el.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        let timeout = timeout.as_millis().try_into().unwrap_or(i32::MAX);
        // SAFETY: a valid pollfd array.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout) };
        assert!(n >= 0, "{}", std::io::Error::last_os_error());
        fds.iter().map(|fd| fd.revents != 0).collect()
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::WaitForMultipleObjects;
        let handles: Vec<_> = els.iter().map(|el| el.as_raw_handle()).collect();
        let timeout = timeout.as_millis().try_into().unwrap_or(u32::MAX);
        // SAFETY: valid handles owned by the event loops.
        let rc =
            unsafe { WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, timeout) };
        // `WaitForMultipleObjects` reports the lowest signaled index; the
        // completion ports are level-triggered, so probe the rest.
        let mut out = vec![false; els.len()];
        if rc == WAIT_TIMEOUT {
            return out;
        }
        let first = (rc - WAIT_OBJECT_0) as usize;
        assert!(first < els.len(), "{}", std::io::Error::last_os_error());
        out[first] = true;
        for (i, el) in els.iter().enumerate().skip(first + 1) {
            out[i] = el.readable_now();
        }
        out
    }
}

impl Host for LocalEventLoop {
    fn readable(&self, timeout: Duration) -> bool {
        wait_any(&[self], timeout)[0]
    }

    fn wait(&self, limit: Duration) -> bool {
        let timeout = self.next_timeout().map_or(limit, |t| t.min(limit));
        // Round up: `poll` has millisecond resolution and the deadline must
        // have passed when the drive runs.
        let timeout = timeout + Duration::from_millis(1);
        self.readable(timeout)
    }

    fn pump(&self, mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "pump stalled");
            self.wait(Duration::from_millis(100));
            while self.drive() {}
        }
    }
}

fn event_loop() -> LocalEventLoop {
    Builder::new_current_thread()
        .enable_all()
        .event_interval(4)
        .build_local_event_loop(Default::default())
        .unwrap()
}

#[test]
fn idle_is_not_signaled() {
    let el = event_loop();
    assert!(!el.readable(Duration::from_millis(20)));
    assert_eq!(el.next_timeout(), None);
    assert!(!el.drive(), "a drive without a wake is harmless");
}

#[test]
fn spawn_signals_and_queues_until_driven() {
    let el = event_loop();
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
    assert!(el.readable_now());

    assert!(!el.drive());
    assert_eq!(ran.load(SeqCst), 1);
    assert!(jh.is_finished());
    assert!(
        !el.readable(Duration::from_millis(20)),
        "idle after the drive"
    );
}

#[test]
fn busy_batch_reports_more() {
    let el = event_loop();
    let turns = Arc::new(AtomicUsize::new(0));

    let t = turns.clone();
    let jh = el.spawn_local(async move {
        for _ in 0..10 {
            t.fetch_add(1, SeqCst);
            tokio::task::yield_now().await;
        }
    });
    assert!(el.readable_now());
    let mut drives = 0;
    while !jh.is_finished() {
        let busy = el.drive();
        drives += 1;
        assert!(drives <= 12, "a drive per yield at most");
        assert_eq!(busy, !jh.is_finished(), "busy iff work remains");
    }
    assert_eq!(turns.load(SeqCst), 10);
    assert!(!el.readable(Duration::from_millis(20)));
}

#[test]
fn inject_leftovers_report_more() {
    // Spawns from host context land in the inject queue. A batch that ends
    // at `event_interval` with more of them queued must report them: the
    // wake those spawns caused was consumed by this drive.
    let el = event_loop();
    let ran = Arc::new(AtomicUsize::new(0));
    for _ in 0..6 {
        let r = ran.clone();
        el.spawn_local(async move {
            r.fetch_add(1, SeqCst);
        });
    }
    assert!(el.readable_now());
    assert!(el.drive(), "two remain after one event_interval(4) batch");
    assert_eq!(ran.load(SeqCst), 4);
    assert!(!el.drive());
    assert_eq!(ran.load(SeqCst), 6);
}

#[test]
fn timer_is_a_timeout() {
    let el = event_loop();
    let start = Instant::now();
    let jh = el.spawn_local(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    assert_eq!(el.next_timeout(), None, "not registered until polled");
    el.drive();
    // The registration itself woke the reactor (see
    // `nearer_timer_from_another_thread_signals`).
    el.drive();
    let timeout = el.next_timeout().expect("timer registered");
    // Deadlines round up to the millisecond.
    assert!(timeout <= Duration::from_millis(51), "{timeout:?}");
    assert!(
        !el.readable(Duration::from_millis(10)),
        "a pending timer does not signal the reactor"
    );

    el.pump(|| jh.is_finished());
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(50), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    assert_eq!(el.next_timeout(), None);
}

#[test]
fn nearer_timer_within_a_drive_is_reread() {
    let el = event_loop();
    el.spawn_local(async {
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    el.drive();
    // The wheel reports its next cascade point, not the deadline itself.
    let far = el.next_timeout().unwrap();
    assert!(
        far > Duration::from_secs(1) && far <= Duration::from_secs(10),
        "{far:?}"
    );

    let jh = el.spawn_local(async {
        tokio::time::sleep(Duration::from_millis(20)).await;
    });
    el.drive();
    assert!(el.next_timeout().unwrap() <= Duration::from_millis(21));
    el.pump(|| jh.is_finished());
}

#[test]
fn nearer_timer_from_another_thread_signals() {
    let el = event_loop();
    el.spawn_local(async {
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    el.drive();
    // Registering a timer nearer than the one the driver last armed wakes
    // the reactor, from inside a drive too. On Unix that wake is left for
    // the host to see and the next drive consumes it; on Windows the
    // drive's trailing driver turn already has.
    #[cfg(unix)]
    {
        assert!(el.readable_now());
        el.drive();
    }
    assert!(!el.readable(Duration::from_millis(20)));

    // A timer registered by `Handle::block_on` on another thread, nearer
    // than the one the host last read: the host must get to re-read.
    let handle = el.handle().clone();
    let t = std::thread::spawn(move || {
        handle.block_on(async { tokio::time::sleep(Duration::from_millis(30)).await });
    });
    assert!(
        el.readable(Duration::from_secs(1)),
        "a nearer timer signals the reactor"
    );
    el.drive();
    assert!(el.next_timeout().unwrap() <= Duration::from_millis(31));
    el.pump(|| t.is_finished());
    t.join().unwrap();
}

#[test]
fn cancelled_timer_disarms() {
    let el = event_loop();
    let (tx, rx) = oneshot::channel::<()>();
    let jh = el.spawn_local(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(30)) => false,
            _ = rx => true,
        }
    });
    el.drive();
    assert!(el.next_timeout().is_some());
    tx.send(()).unwrap();
    assert!(el.readable_now(), "the oneshot wake");
    el.pump(|| jh.is_finished());
    assert_eq!(el.next_timeout(), None, "the cancelled timer is gone");
}

#[test]
fn wake_from_another_thread_signals() {
    let el = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.spawn_local(async move { rx.await.unwrap() });
    el.drive();
    assert!(!el.readable(Duration::from_millis(20)));

    std::thread::spawn(move || tx.send(7).unwrap())
        .join()
        .unwrap();
    assert!(el.readable_now());
    el.pump(|| jh.is_finished());
}

#[test]
fn spawn_from_another_thread_via_handle() {
    let el = event_loop();
    let handle = el.handle().clone();
    let jh = std::thread::spawn(move || handle.spawn(async { 11 }))
        .join()
        .unwrap();
    assert!(el.readable_now());
    el.pump(|| jh.is_finished());
}

#[test]
fn cross_loop_wake() {
    let a = event_loop();
    let b = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh_a = a.spawn_local(async move { rx.await.unwrap() });
    let jh_b = b.spawn_local(async move { tx.send(5).unwrap() });
    a.drive();
    assert!(!a.readable(Duration::from_millis(20)));
    b.drive();
    assert!(jh_b.is_finished());
    assert!(a.readable_now(), "woken by a task on the other loop");
    a.pump(|| jh_a.is_finished());
}

#[test]
fn tcp_round_trip() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    let server = event_loop();
    let client = event_loop();
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

    // One host, two reactor handles.
    let start = Instant::now();
    while !(jh_s.is_finished() && jh_c.is_finished()) {
        assert!(start.elapsed() < Duration::from_secs(5));
        let ready = wait_any(&[&server, &client], Duration::from_millis(100));
        if ready[0] {
            while server.drive() {}
        }
        if ready[1] {
            while client.drive() {}
        }
    }
}

#[test]
fn io_readiness_signals() {
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    let el = event_loop();
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let jh = el.spawn_local(async move {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf).await.unwrap();
        buf
    });
    el.drive();
    let addr = addr_rx.recv().unwrap();
    assert!(!el.readable(Duration::from_millis(20)), "nothing pending");

    let mut sock = std::net::TcpStream::connect(addr).unwrap();
    assert!(
        el.readable(Duration::from_secs(1)),
        "accept readiness, with no thread involved"
    );
    while el.drive() {}
    // The accepted socket's own registration reports it writable.
    while el.readable_now() {
        while el.drive() {}
    }
    assert!(!jh.is_finished());

    std::io::Write::write_all(&mut sock, b"hello").unwrap();
    assert!(el.readable(Duration::from_secs(1)));
    el.pump(|| jh.is_finished());
}

#[test]
fn unhandled_panic_shutdown_panics_drive() {
    let el = Builder::new_current_thread()
        .enable_io()
        .unhandled_panic(tokio::runtime::UnhandledPanic::ShutdownRuntime)
        .build_local_event_loop(Default::default())
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

    let el = event_loop();
    let dropped = Arc::new(AtomicUsize::new(0));
    let flag = Flag(dropped.clone());
    el.spawn_local(async move {
        let _flag = flag;
        let _listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    el.drive();
    assert_eq!(dropped.load(SeqCst), 0);
    let start = Instant::now();
    drop(el);
    assert_eq!(dropped.load(SeqCst), 1, "drop shuts the runtime down");
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn handle_block_on_from_another_thread() {
    let el = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let handle = el.handle().clone();
    let t = std::thread::spawn(move || {
        handle.block_on(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            rx.await.unwrap()
        })
    });
    el.spawn_local(async move { tx.send(9).unwrap() });
    // Its timer is served by the host's drives.
    el.pump(|| t.is_finished());
    assert_eq!(t.join().unwrap(), 9);
}

#[test]
fn spawn_local_with_rc() {
    let el = event_loop();
    let rc = Rc::new(5);
    let rc2 = rc.clone();
    let jh = el.spawn_local(async move { *rc2 + 1 });
    el.pump(|| jh.is_finished());
    assert_eq!(Rc::strong_count(&rc), 1);
}

#[test]
fn rejects_foreign_thread_drive() {
    let el = event_loop();
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
    let el = Rc::new(event_loop());
    let el2 = el.clone();
    let jh = el.spawn_local(async move {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el2.drive())).is_err()
    });
    el.pump(|| jh.is_finished());
    let el = Rc::try_unwrap(el).expect("the task's clone is dropped with it");
    assert!(el.handle().block_on(jh).unwrap());
}

#[test]
fn without_io_driver_is_an_error() {
    let res = Builder::new_current_thread()
        .enable_time()
        .build_local_event_loop(Default::default());
    assert!(res.is_err());
}

#[test]
fn without_time_driver() {
    let el = Builder::new_current_thread()
        .enable_io()
        .build_local_event_loop(Default::default())
        .unwrap();
    assert_eq!(el.next_timeout(), None);
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.spawn_local(async move { rx.await.unwrap() });
    el.drive();
    std::thread::spawn(move || tx.send(3).unwrap())
        .join()
        .unwrap();
    el.pump(|| jh.is_finished());
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
