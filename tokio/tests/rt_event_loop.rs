#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", tokio_unstable, not(target_family = "wasm")))]

//! `EventLoop` and `LocalEventLoop` driven the way a host would: a wait on
//! `event_fd` (`poll(2)`) or in `park`, bounded by `next_sleep`, then `tick`
//! until `next_sleep` is no longer zero. The descriptor, the callback and
//! the timeout are the whole contract, so the tests pin when the host is and
//! is not notified as much as what a tick does.

use std::cell::Cell;
use std::future::Future;
#[cfg(unix)]
use std::os::fd::{AsRawFd, BorrowedFd};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::runtime::{Builder, EventLoop, LocalEventLoop, LocalOptions, Unparker};
use tokio::sync::oneshot;

const ZERO: Option<Duration> = Some(Duration::ZERO);

/// The two event loop types, for tests that apply to both.
trait Loop {
    fn tick(&mut self, max_tasks: usize) -> usize;
    fn next_sleep(&self) -> Option<Duration>;
    fn park(&mut self, timeout: Option<Duration>);
    #[cfg(unix)]
    fn event_fd(&self) -> Option<BorrowedFd<'_>>;
}

macro_rules! impl_loop {
    ($t:ty) => {
        impl Loop for $t {
            fn tick(&mut self, max_tasks: usize) -> usize {
                <$t>::tick(self, max_tasks)
            }
            fn next_sleep(&self) -> Option<Duration> {
                <$t>::next_sleep(self)
            }
            fn park(&mut self, timeout: Option<Duration>) {
                <$t>::park(self, timeout)
            }
            #[cfg(unix)]
            fn event_fd(&self) -> Option<BorrowedFd<'_>> {
                <$t>::event_fd(self)
            }
        }
    };
}
impl_loop!(EventLoop);
impl_loop!(LocalEventLoop);

/// The host's side of the contract.
trait Host {
    /// Ticks until no work is ready. Returns the number of tasks polled.
    fn drain(&mut self) -> usize;

    /// Waits up to `timeout` for `event_fd` to be readable.
    #[cfg(unix)]
    fn readable(&self, timeout: Duration) -> bool;

    #[cfg(unix)]
    fn readable_now(&self) -> bool {
        self.readable(Duration::ZERO)
    }

    /// A host loop over `event_fd` with `next_sleep` as the timeout.
    #[cfg(unix)]
    fn pump_fd(&mut self, done: impl FnMut() -> bool);

    /// A host loop that blocks in `park`.
    fn pump_park(&mut self, done: impl FnMut() -> bool);
}

/// Waits on several loops' descriptors at once, as one host does. Returns
/// which are readable.
#[cfg(unix)]
fn wait_any(fds: &[BorrowedFd<'_>], timeout: Duration) -> Vec<bool> {
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .map(|fd| libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    let timeout = timeout.as_millis().try_into().unwrap_or(i32::MAX);
    // SAFETY: a valid pollfd array.
    let n = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as _, timeout) };
    assert!(n >= 0, "{}", std::io::Error::last_os_error());
    pfds.iter().map(|fd| fd.revents != 0).collect()
}

/// `poll` has millisecond resolution; round up so the deadline has passed.
#[cfg(unix)]
fn bound(next: Option<Duration>, limit: Duration) -> Duration {
    next.map_or(limit, |t| t.min(limit)) + Duration::from_millis(1)
}

impl<L: Loop> Host for L {
    fn drain(&mut self) -> usize {
        let mut ran = 0;
        loop {
            ran += self.tick(4);
            if self.next_sleep() != ZERO {
                return ran;
            }
        }
    }

    #[cfg(unix)]
    fn readable(&self, timeout: Duration) -> bool {
        wait_any(&[self.event_fd().expect("event_fd")], timeout)[0]
    }

    #[cfg(unix)]
    fn pump_fd(&mut self, mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "pump stalled");
            self.readable(bound(self.next_sleep(), Duration::from_millis(100)));
            self.drain();
        }
    }

    fn pump_park(&mut self, mut done: impl FnMut() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(5), "pump stalled");
            self.park(Some(Duration::from_millis(100)));
            self.drain();
        }
    }
}

fn event_loop() -> EventLoop {
    Builder::new_current_thread()
        .enable_all()
        .build_event_loop()
        .unwrap()
}

fn local_event_loop() -> LocalEventLoop {
    Builder::new_current_thread()
        .enable_all()
        .build_local_event_loop(LocalOptions::default())
        .unwrap()
}

fn counter() -> (Arc<AtomicUsize>, impl Fn() + Send + Sync + 'static) {
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = n.clone();
    (n, move || {
        n2.fetch_add(1, SeqCst);
    })
}

// ===== event_fd =====

#[cfg(unix)]
#[test]
fn idle_is_not_readable() {
    let mut el = event_loop();
    assert!(!el.readable(Duration::from_millis(20)));
    assert_eq!(el.next_sleep(), None);
    assert_eq!(el.tick(64), 0, "a tick without a wakeup is harmless");
    assert_eq!(el.next_sleep(), None);
}

#[cfg(unix)]
#[test]
fn spawn_is_readable_and_queues_until_ticked() {
    let mut el = local_event_loop();
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
    assert_eq!(el.next_sleep(), ZERO);

    assert_eq!(el.tick(64), 1);
    assert_eq!(ran.load(SeqCst), 1);
    assert!(jh.is_finished());
    assert_eq!(el.next_sleep(), None);
    assert!(
        !el.readable(Duration::from_millis(20)),
        "idle after the tick"
    );
}

#[cfg(unix)]
#[test]
fn timer_is_a_timeout_not_a_readiness() {
    let mut el = event_loop();
    let start = Instant::now();
    let jh = el.handle().spawn(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    assert_eq!(el.next_sleep(), ZERO, "the spawn is ready work");
    el.drain();
    let timeout = el.next_sleep().expect("timer registered");
    // Deadlines round up to the millisecond.
    assert!(timeout <= Duration::from_millis(51), "{timeout:?}");
    assert!(
        !el.readable(Duration::from_millis(10)),
        "a pending timer does not make the descriptor readable"
    );

    el.pump_fd(|| jh.is_finished());
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(50), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    assert_eq!(el.next_sleep(), None);
}

#[cfg(unix)]
#[test]
fn nearer_timer_from_another_thread_is_readable() {
    let mut el = event_loop();
    el.handle().spawn(async {
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    el.drain();
    // The wheel reports its next cascade point, not the deadline itself.
    let far = el.next_sleep().unwrap();
    assert!(
        far > Duration::from_secs(1) && far <= Duration::from_secs(10),
        "{far:?}"
    );
    assert!(
        !el.readable(Duration::from_millis(20)),
        "a timer registered from within a tick does not wake the driver"
    );

    // A timer registered by `Handle::block_on` on another thread, nearer
    // than the one the host last read: the host must get to re-read.
    let handle = el.handle().clone();
    let t = std::thread::spawn(move || {
        handle.block_on(async { tokio::time::sleep(Duration::from_millis(30)).await });
    });
    assert!(
        el.readable(Duration::from_secs(1)),
        "a nearer timer wakes the host"
    );
    el.drain();
    assert!(el.next_sleep().unwrap() <= Duration::from_millis(31));
    el.pump_fd(|| t.is_finished());
    t.join().unwrap();
}

/// The driver only announces a remote timer nearer than the wheel's next
/// expiration as of the last park. A timer cancelled since then moves the
/// host's `next_sleep` later than that, and a remote timer in between must
/// still wake the host.
#[cfg(unix)]
#[test]
fn nearer_timer_after_a_cancel_is_readable() {
    let mut el = event_loop();
    let (release, released) = oneshot::channel::<()>();
    el.handle().spawn(async move {
        let sleep = tokio::time::sleep(Duration::from_millis(50));
        tokio::pin!(sleep);
        // Registers the timer, then holds it until released.
        std::future::poll_fn(|cx| {
            let _ = sleep.as_mut().poll(cx);
            std::task::Poll::Ready(())
        })
        .await;
        released.await.unwrap();
    });
    el.drain();
    // A park with the timer registered: the driver records it as its next
    // expiration.
    el.tick(0);
    assert!(el.next_sleep().unwrap() <= Duration::from_millis(60));

    // The task drops the timer; the host now sees nothing to wait for.
    release.send(()).unwrap();
    el.drain();
    assert_eq!(el.next_sleep(), None);

    let handle = el.handle().clone();
    let t = std::thread::spawn(move || {
        handle.block_on(async { tokio::time::sleep(Duration::from_millis(75)).await });
    });
    assert!(
        el.readable(Duration::from_secs(1)),
        "a timer nearer than the host's wait wakes the host"
    );
    el.pump_fd(|| t.is_finished());
    t.join().unwrap();
}

#[cfg(unix)]
#[test]
fn cancelled_timer_disarms() {
    let mut el = event_loop();
    let (tx, rx) = oneshot::channel::<()>();
    let jh = el.handle().spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(30)) => false,
            _ = rx => true,
        }
    });
    el.drain();
    assert!(el.next_sleep().is_some());
    tx.send(()).unwrap();
    assert!(el.readable_now(), "the oneshot wake");
    el.pump_fd(|| jh.is_finished());
    assert_eq!(el.next_sleep(), None, "the cancelled timer is gone");
}

#[cfg(unix)]
#[test]
fn wake_from_another_thread_is_readable() {
    let mut el = event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.handle().spawn(async move { rx.await.unwrap() });
    el.drain();
    assert!(!el.readable(Duration::from_millis(20)));

    std::thread::spawn(move || tx.send(7).unwrap())
        .join()
        .unwrap();
    assert!(el.readable_now());
    el.pump_fd(|| jh.is_finished());
}

#[cfg(unix)]
#[test]
fn cross_loop_wake() {
    let mut a = local_event_loop();
    let mut b = local_event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh_a = a.spawn_local(async move { rx.await.unwrap() });
    let jh_b = b.spawn_local(async move { tx.send(5).unwrap() });
    a.drain();
    assert!(!a.readable(Duration::from_millis(20)));
    b.drain();
    assert!(jh_b.is_finished());
    assert!(a.readable_now(), "woken by a task on the other loop");
    a.pump_fd(|| jh_a.is_finished());
}

#[cfg(unix)]
#[test]
fn tcp_round_trip() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    let mut server = event_loop();
    let mut client = local_event_loop();
    let (addr_tx, addr_rx) = oneshot::channel();

    let jh_s = server.handle().spawn(async move {
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

    // One host, two descriptors.
    let start = Instant::now();
    while !(jh_s.is_finished() && jh_c.is_finished()) {
        assert!(start.elapsed() < Duration::from_secs(5));
        let timeout = bound(
            server
                .next_sleep()
                .into_iter()
                .chain(client.next_sleep())
                .min(),
            Duration::from_millis(100),
        );
        let ready = wait_any(
            &[server.event_fd().unwrap(), client.event_fd().unwrap()],
            timeout,
        );
        if ready[0] || server.next_sleep() == ZERO {
            server.drain();
        }
        if ready[1] || client.next_sleep() == ZERO {
            client.drain();
        }
    }
}

#[cfg(unix)]
#[test]
fn io_readiness_is_readable() {
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    let mut el = event_loop();
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let jh = el.handle().spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 5];
        sock.read_exact(&mut buf).await.unwrap();
        buf
    });
    el.drain();
    let addr = addr_rx.recv().unwrap();
    assert!(!el.readable(Duration::from_millis(20)), "nothing pending");

    let mut sock = std::net::TcpStream::connect(addr).unwrap();
    assert!(
        el.readable(Duration::from_secs(1)),
        "accept readiness, with no thread involved"
    );
    el.drain();
    // The accepted socket's own registration reports it writable.
    while el.readable_now() {
        el.drain();
    }
    assert!(!jh.is_finished());

    std::io::Write::write_all(&mut sock, b"hello").unwrap();
    assert!(el.readable(Duration::from_secs(1)));
    el.pump_fd(|| jh.is_finished());
}

#[cfg(unix)]
#[test]
fn without_io_driver_event_fd_is_none() {
    let el = Builder::new_current_thread()
        .enable_time()
        .build_event_loop()
        .unwrap();
    assert!(el.event_fd().is_none());
}

#[cfg(unix)]
#[test]
fn enable_event_fd_without_io_driver() {
    let mut el = Builder::new_current_thread()
        .enable_time()
        .enable_event_fd()
        .build_event_loop()
        .unwrap();
    assert!(el.event_fd().is_some());
    assert!(!el.readable(Duration::from_millis(20)));

    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.handle().spawn(async move { rx.await.unwrap() });
    assert!(el.readable_now(), "a spawn is a wakeup");
    el.handle().spawn(async {});
    el.drain();
    assert!(
        !el.readable(Duration::from_millis(20)),
        "drained by the tick"
    );

    std::thread::spawn(move || tx.send(7).unwrap())
        .join()
        .unwrap();
    assert!(el.readable_now(), "readable again after the tick");
    el.pump_fd(|| jh.is_finished());
    assert!(!el.readable(Duration::from_millis(20)));

    // A wakeup during a tick leaves it readable, as the reactor's would be:
    // a host that only watches the descriptor still wakes.
    let handle = el.handle().clone();
    el.handle().spawn(async move {
        std::thread::spawn(move || handle.spawn(async {}))
            .join()
            .unwrap();
    });
    assert_eq!(el.tick(1), 1);
    assert!(el.readable_now(), "the in-tick spawn");
    assert_eq!(el.next_sleep(), ZERO);
    el.drain();
    assert!(!el.readable(Duration::from_millis(20)));

    // A timer is still only a timeout.
    let jh = el.handle().spawn(async {
        tokio::time::sleep(Duration::from_millis(30)).await;
    });
    el.drain();
    assert!(el.next_sleep().unwrap() <= Duration::from_millis(31));
    assert!(!el.readable(Duration::from_millis(10)));
    el.pump_fd(|| jh.is_finished());
}

#[cfg(unix)]
#[test]
fn enable_event_fd_with_io_driver_uses_the_reactor() {
    // The reactor's descriptor covers wakeups too, so I/O readiness must
    // still arrive on whatever `event_fd` returns.
    let mut el = Builder::new_current_thread()
        .enable_all()
        .enable_event_fd()
        .build_event_loop()
        .unwrap();
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let jh = el.handle().spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        listener.accept().await.unwrap();
    });
    el.drain();
    let _sock = std::net::TcpStream::connect(addr_rx.recv().unwrap()).unwrap();
    assert!(el.readable(Duration::from_secs(1)));
    el.pump_fd(|| jh.is_finished());
}

// ===== tick =====

#[test]
fn max_tasks_bounds_the_batch() {
    let mut el = event_loop();
    let ran = Arc::new(AtomicUsize::new(0));
    for _ in 0..6 {
        let r = ran.clone();
        el.handle().spawn(async move {
            r.fetch_add(1, SeqCst);
        });
    }
    assert_eq!(el.tick(0), 0, "drains the driver without running tasks");
    assert_eq!(ran.load(SeqCst), 0);
    assert_eq!(el.next_sleep(), ZERO);
    assert_eq!(el.tick(4), 4);
    assert_eq!(ran.load(SeqCst), 4);
    assert_eq!(el.next_sleep(), ZERO, "two remain");
    assert_eq!(el.tick(4), 2);
    assert_eq!(ran.load(SeqCst), 6);
    assert_eq!(el.next_sleep(), None);
}

#[test]
fn yields_report_ready_work() {
    let mut el = local_event_loop();
    let turns = Rc::new(Cell::new(0));
    let t = turns.clone();
    let jh = el.spawn_local(async move {
        for _ in 0..10 {
            t.set(t.get() + 1);
            tokio::task::yield_now().await;
        }
    });
    let mut ticks = 0;
    while !jh.is_finished() {
        assert_eq!(el.next_sleep(), ZERO, "ready while the task is yielding");
        assert_eq!(el.tick(64), 1, "one poll per yield");
        ticks += 1;
        assert!(ticks <= 11);
    }
    assert_eq!(turns.get(), 10);
    assert_eq!(el.next_sleep(), None);
}

#[test]
fn nearer_timer_within_a_tick_is_reread() {
    let mut el = event_loop();
    el.handle().spawn(async {
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    el.drain();
    assert!(el.next_sleep().unwrap() > Duration::from_secs(1));

    let jh = el.handle().spawn(async {
        tokio::time::sleep(Duration::from_millis(20)).await;
    });
    el.drain();
    assert!(el.next_sleep().unwrap() <= Duration::from_millis(21));
    el.pump_park(|| jh.is_finished());
}

#[test]
fn spawn_from_another_thread_via_handle() {
    let mut el = event_loop();
    let handle = el.handle().clone();
    let jh = std::thread::spawn(move || handle.spawn(async { 11 }))
        .join()
        .unwrap();
    assert_eq!(el.next_sleep(), ZERO);
    el.pump_park(|| jh.is_finished());
}

#[test]
fn enter_binds_tokio_spawn() {
    let mut el = event_loop();
    let jh = {
        let _guard = el.enter();
        tokio::spawn(async { 2 })
    };
    assert_eq!(el.tick(64), 1);
    assert!(jh.is_finished());
}

#[test]
fn unhandled_panic_shutdown_panics_tick() {
    let mut el = Builder::new_current_thread()
        .unhandled_panic(tokio::runtime::UnhandledPanic::ShutdownRuntime)
        .build_event_loop()
        .unwrap();
    el.handle().spawn(async { panic!("task panicked") });
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| el.tick(64)));
    assert!(res.is_err(), "tick reports the shutdown as a panic");
}

#[test]
fn tick_inside_a_runtime_panics() {
    let mut el = event_loop();
    let rt = Builder::new_current_thread().build().unwrap();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { el.tick(1) })
    }));
    assert!(res.is_err());
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rt.block_on(async { el.park(ZERO) })
    }));
    assert!(res.is_err());
    assert_eq!(el.tick(1), 0, "the loop is intact");
}

// ===== park =====

#[test]
fn park_timeout_elapses() {
    let mut el = event_loop();
    let start = Instant::now();
    let deadline = start + Duration::from_millis(30);
    // The OS wait may return a little early; a host parks for what remains.
    let mut parks = 0;
    while Instant::now() < deadline {
        el.park(Some(deadline - Instant::now()));
        parks += 1;
        assert!(parks < 10, "park returns at once");
    }
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn park_returns_on_unpark_from_another_thread() {
    let mut el = event_loop();
    let unparker = el.unparker();
    let start = Instant::now();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        unparker.unpark();
    });
    // Nothing is armed, so only the unpark (or a spurious return) ends a park.
    while start.elapsed() < Duration::from_millis(50) {
        el.park(None);
    }
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
    t.join().unwrap();

    // An unpark before the park makes it return at once.
    el.unparker().unpark();
    let start = Instant::now();
    el.park(None);
    assert!(start.elapsed() < Duration::from_millis(100));
}

/// An unpark carries no task or timer, but it is work for the host: the
/// contract that `next_sleep` is `Some(ZERO)` whenever the host should tick
/// holds for it too, until a tick or park consumes it.
#[test]
fn unpark_is_reported_by_next_sleep() {
    let mut el = event_loop();
    el.drain();
    assert_eq!(el.next_sleep(), None);

    el.unparker().unpark();
    assert_eq!(el.next_sleep(), ZERO);
    el.tick(0);
    assert_eq!(el.next_sleep(), None);

    el.unparker().unpark();
    assert_eq!(el.next_sleep(), ZERO);
    el.park(None);
    assert_eq!(el.next_sleep(), None);
}

#[test]
fn park_returns_on_timer_without_running_tasks() {
    let mut el = event_loop();
    let ran = Arc::new(AtomicUsize::new(0));
    let r = ran.clone();
    // The deadline is set when the tick first polls the sleep.
    let start = Instant::now();
    let jh = el.handle().spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        r.fetch_add(1, SeqCst);
    });
    el.drain();
    // A park may return spuriously (the OS wait a little early); the timer
    // fires only once due.
    while el.next_sleep() != ZERO {
        el.park(None);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(30), "{elapsed:?}");
    assert_eq!(ran.load(SeqCst), 0, "park runs no tasks");
    assert_eq!(el.tick(64), 1);
    assert!(jh.is_finished());
}

#[test]
fn park_returns_at_once_with_ready_work() {
    let mut el = event_loop();
    el.handle().spawn(async {});
    let start = Instant::now();
    el.park(None);
    assert!(start.elapsed() < Duration::from_millis(100), "queued spawn");

    // The driver's wakeup for the spawn was consumed by a tick that did not
    // run the task: the task alone must keep park from blocking.
    assert_eq!(el.tick(0), 0);
    assert_eq!(el.next_sleep(), ZERO);
    let start = Instant::now();
    el.park(None);
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "injected task"
    );
    assert_eq!(el.tick(64), 1);

    // Likewise a task left in the local queue by `max_tasks`.
    let mut el = local_event_loop();
    let yielder = el.spawn_local(async {
        tokio::task::yield_now().await;
    });
    el.spawn_local(async {});
    assert_eq!(el.tick(1), 1);
    assert_eq!(el.next_sleep(), ZERO);
    let start = Instant::now();
    el.park(None);
    assert!(start.elapsed() < Duration::from_millis(100), "local task");
    el.drain();
    assert!(yielder.is_finished());
}

#[test]
fn park_returns_on_io() {
    let mut el = event_loop();
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let jh = el.handle().spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        listener.accept().await.unwrap();
    });
    el.drain();
    let addr = addr_rx.recv().unwrap();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        std::net::TcpStream::connect(addr).unwrap()
    });
    let start = Instant::now();
    el.park(None);
    assert!(start.elapsed() < Duration::from_secs(1));
    el.pump_park(|| jh.is_finished());
    drop(t.join().unwrap());
}

#[test]
fn handle_block_on_from_another_thread() {
    let mut el = local_event_loop();
    let (tx, rx) = oneshot::channel::<u32>();
    let handle = el.handle().clone();
    let t = std::thread::spawn(move || {
        handle.block_on(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            rx.await.unwrap()
        })
    });
    el.spawn_local(async move { tx.send(9).unwrap() });
    // Its timer is served by the host's ticks.
    el.pump_park(|| t.is_finished());
    assert_eq!(t.join().unwrap(), 9);
}

// ===== on_work_available =====

#[test]
fn on_work_available_once_per_tick() {
    let (n, f) = counter();
    let mut el = Builder::new_current_thread()
        .enable_all()
        .on_work_available(f)
        .build_event_loop()
        .unwrap();
    assert_eq!(n.load(SeqCst), 0);

    el.handle().spawn(async {});
    assert_eq!(n.load(SeqCst), 1, "a spawn is a wakeup");
    el.handle().spawn(async {});
    assert_eq!(n.load(SeqCst), 1, "coalesced until the tick");
    assert_eq!(el.tick(64), 2);
    assert_eq!(n.load(SeqCst), 1);

    let handle = el.handle().clone();
    std::thread::spawn(move || handle.spawn(async {}))
        .join()
        .unwrap();
    assert_eq!(n.load(SeqCst), 2, "from another thread");
    el.drain();

    el.unparker().unpark();
    assert_eq!(n.load(SeqCst), 3, "an unpark is a wakeup");
    el.drain();

    // A nearer timer registered elsewhere while the host is idle.
    let handle = el.handle().clone();
    let (registered_tx, registered_rx) = std::sync::mpsc::channel();
    let t = std::thread::spawn(move || {
        handle.block_on(async {
            let sleep = tokio::time::sleep(Duration::from_millis(10));
            tokio::pin!(sleep);
            std::future::poll_fn(|cx| {
                assert!(sleep.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            registered_tx.send(()).unwrap();
            sleep.await
        })
    });
    registered_rx.recv().unwrap();
    assert_eq!(n.load(SeqCst), 4, "a nearer timer from elsewhere");
    el.pump_park(|| t.is_finished());
    t.join().unwrap();
    assert_eq!(n.load(SeqCst), 4, "nothing more while the host drives");

    // Timers and wakes from within a tick do not notify the host.
    let before = n.load(SeqCst);
    let jh = el.handle().spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        tokio::spawn(async {}).await.unwrap();
    });
    assert_eq!(n.load(SeqCst), before + 1);
    el.pump_park(|| jh.is_finished());
    assert_eq!(n.load(SeqCst), before + 1);

    // Nor does a remote wake during a tick: `next_sleep` reports its work.
    // It does not use up the idle period's notification either.
    let before = n.load(SeqCst);
    let handle = el.handle().clone();
    el.handle().spawn(async move {
        std::thread::spawn(move || handle.spawn(async {}))
            .join()
            .unwrap();
    });
    assert_eq!(n.load(SeqCst), before + 1);
    assert_eq!(el.tick(1), 1);
    assert_eq!(n.load(SeqCst), before + 1, "suppressed during the tick");
    assert_eq!(el.next_sleep(), ZERO, "reported here instead");
    el.handle().spawn(async {});
    assert_eq!(n.load(SeqCst), before + 2, "the next idle wakeup notifies");
    el.drain();

    // An explicit unpark carries no work, so it reaches the host even from
    // within a tick.
    let before = n.load(SeqCst);
    let unparker = el.unparker();
    el.handle().spawn(async move { unparker.unpark() });
    assert_eq!(n.load(SeqCst), before + 1);
    assert_eq!(el.tick(1), 1);
    assert_eq!(n.load(SeqCst), before + 2, "the in-tick unpark");
    el.drain();
}

#[test]
fn on_work_available_not_for_io() {
    let (n, f) = counter();
    let mut el = Builder::new_current_thread()
        .enable_all()
        .on_work_available(f)
        .build_event_loop()
        .unwrap();
    let (addr_tx, addr_rx) = std::sync::mpsc::channel();
    let jh = el.handle().spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        addr_tx.send(listener.local_addr().unwrap()).unwrap();
        listener.accept().await.unwrap();
    });
    el.drain();
    let before = n.load(SeqCst);
    let _sock = std::net::TcpStream::connect(addr_rx.recv().unwrap()).unwrap();
    el.pump_park(|| jh.is_finished());
    assert_eq!(
        n.load(SeqCst),
        before,
        "I/O readiness is the reactor's to report"
    );
}

#[test]
fn on_work_available_without_io_driver() {
    let (n, f) = counter();
    let mut el = Builder::new_current_thread()
        .enable_time()
        .on_work_available(f)
        .build_event_loop()
        .unwrap();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.handle().spawn(async move { rx.await.unwrap() });
    assert_eq!(n.load(SeqCst), 1);
    el.drain();
    std::thread::spawn(move || tx.send(3).unwrap())
        .join()
        .unwrap();
    assert_eq!(n.load(SeqCst), 2);
    el.pump_park(|| jh.is_finished());
}

// ===== lifecycle =====

#[test]
fn builder_rejects_multi_thread() {
    assert!(Builder::new_multi_thread().build_event_loop().is_err());
    assert!(Builder::new_multi_thread()
        .build_local_event_loop(LocalOptions::default())
        .is_err());
}

#[test]
fn without_time_driver() {
    let mut el = Builder::new_current_thread()
        .enable_io()
        .build_event_loop()
        .unwrap();
    assert_eq!(el.next_sleep(), None);
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.handle().spawn(async move { rx.await.unwrap() });
    el.drain();
    assert_eq!(el.next_sleep(), None);
    std::thread::spawn(move || tx.send(3).unwrap())
        .join()
        .unwrap();
    el.pump_park(|| jh.is_finished());
}

#[test]
fn without_any_driver() {
    let mut el = Builder::new_current_thread().build_event_loop().unwrap();
    let (tx, rx) = oneshot::channel::<u32>();
    let jh = el.handle().spawn(async move { rx.await.unwrap() });
    el.drain();
    std::thread::spawn(move || tx.send(3).unwrap())
        .join()
        .unwrap();
    el.pump_park(|| jh.is_finished());
}

#[test]
fn drop_cancels_tasks() {
    struct Flag(Arc<AtomicUsize>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.fetch_add(1, SeqCst);
        }
    }

    let mut el = event_loop();
    let dropped = Arc::new(AtomicUsize::new(0));
    let flag = Flag(dropped.clone());
    el.handle().spawn(async move {
        let _flag = flag;
        let _listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    el.drain();
    assert_eq!(dropped.load(SeqCst), 0);
    let start = Instant::now();
    drop(el);
    assert_eq!(dropped.load(SeqCst), 1, "drop shuts the runtime down");
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn shutdown_methods() {
    let el = event_loop();
    el.handle().spawn(std::future::pending::<()>());
    el.shutdown_background();

    let el = local_event_loop();
    el.spawn_local(std::future::pending::<()>());
    el.shutdown_timeout(Duration::from_millis(10));
}

#[test]
fn spawn_local_with_rc() {
    let mut el = local_event_loop();
    let rc = Rc::new(5);
    let rc2 = rc.clone();
    let jh = el.spawn_local(async move { *rc2 + 1 });
    el.pump_park(|| jh.is_finished());
    assert_eq!(Rc::strong_count(&rc), 1);
}

#[test]
fn event_loop_ticks_on_another_thread() {
    let mut el = event_loop();
    let jh = el.handle().spawn(async { 4 });
    let t = std::thread::spawn(move || {
        el.pump_park(|| jh.is_finished());
        el.handle().block_on(jh).unwrap()
    });
    assert_eq!(t.join().unwrap(), 4);
}

// `EventLoop` is `Send + Sync` like `Runtime`; `LocalEventLoop` is neither,
// like `LocalRuntime`. The method resolves to the blanket impl only when the
// auto trait is absent.
#[allow(dead_code)]
fn assert_auto_traits() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<EventLoop>();
    send_sync::<Unparker>();

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
