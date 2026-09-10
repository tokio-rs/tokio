#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), not(miri)))]

//! `Builder::io_shards`: I/O and timers stay live when the multi-thread
//! runtime polls several epoll instances.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn rt(workers: usize, shards: usize) -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .io_shards(shards)
        .enable_all()
        .build()
        .unwrap()
}

/// Spawns a TCP echo server and returns its address.
async fn echo_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0u8; 64];
                while let Ok(n) = s.read(&mut buf).await {
                    if n == 0 || s.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

/// A task that is always ready, so the worker running it never parks.
async fn spin() {
    std::future::poll_fn::<(), _>(|cx| {
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await
}

async fn echo_round_trips(conns: usize, rounds: usize) {
    let addr = echo_server().await;
    let mut tasks = Vec::new();
    for c in 0..conns {
        tasks.push(tokio::spawn(async move {
            let mut s = TcpStream::connect(addr).await.unwrap();
            let mut buf = [0u8; 8];
            for r in 0..rounds {
                let msg = ((c * rounds + r) as u64).to_le_bytes();
                s.write_all(&msg).await.unwrap();
                s.read_exact(&mut buf).await.unwrap();
                assert_eq!(buf, msg);
                if r % 16 == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        }));
    }
    for t in tasks {
        tokio::time::timeout(Duration::from_secs(30), t)
            .await
            .expect("stalled")
            .unwrap();
    }
}

#[test]
fn echo_across_shards() {
    for shards in [1, 2, 4] {
        rt(8, shards).block_on(echo_round_trips(32, 64));
    }
}

#[test]
fn more_shards_than_needed_is_clamped() {
    // 2 workers cannot host 4 shards; the builder clamps instead of failing.
    rt(2, 4).block_on(echo_round_trips(4, 16));
}

#[test]
fn timers_fire_with_all_workers_parked() {
    let rt = rt(8, 4);
    rt.block_on(async {
        let mut tasks = Vec::new();
        for i in 0..256u64 {
            tasks.push(tokio::spawn(tokio::time::sleep(Duration::from_millis(
                i % 50,
            ))));
        }
        for t in tasks {
            tokio::time::timeout(Duration::from_secs(10), t)
                .await
                .expect("timer lost")
                .unwrap();
        }
    });
}

#[test]
fn busy_runtime_still_polls_its_shards() {
    // Every worker permanently busy: I/O must still be driven from the
    // maintenance tick (regression: the help sweep once suppressed the
    // zero-timeout poll of the worker's own shard).
    for shards in [1, 2] {
        let rt = rt(2, shards);
        for _ in 0..64 {
            rt.spawn(spin());
        }
        rt.block_on(echo_round_trips(4, 16));
    }
}

/// Two workers, two shards. One worker is stuck in a non-yielding task for up
/// to `stuck` (it is released when the measurement ends); with `busy` the other
/// never parks. Returns how long one round trip on each of `conns` warmed-up
/// connections took meanwhile.
fn round_trips_with_a_wedged_group(conns: usize, busy: bool, stuck: Duration) -> Duration {
    rt(2, 2).block_on(async {
        let addr = echo_server().await;
        let mut socks = Vec::new();
        for _ in 0..conns {
            let mut s = TcpStream::connect(addr).await.unwrap();
            s.write_all(b"warmup!!").await.unwrap();
            s.read_exact(&mut [0u8; 8]).await.unwrap();
            socks.push(s);
        }
        if busy {
            tokio::spawn(spin());
        }
        std::thread::sleep(Duration::from_millis(50));
        let (release, released) = std::sync::mpsc::channel::<()>();
        tokio::spawn(async move {
            let _ = released.recv_timeout(stuck);
        });
        std::thread::sleep(Duration::from_millis(50));
        let t0 = std::time::Instant::now();
        for s in socks.iter_mut() {
            s.write_all(b"pingping").await.unwrap();
            s.read_exact(&mut [0u8; 8]).await.unwrap();
        }
        let took = t0.elapsed();
        drop(release);
        took
    })
}

#[test]
fn wedged_group_does_not_strand_its_shard() {
    // The other worker is idle. With one driver it serves every socket; with
    // two shards it must still reach the stuck group's shard (bounded by the
    // driver's sweep interval, not by the stuck task).
    let took = round_trips_with_a_wedged_group(8, false, Duration::from_secs(5));
    assert!(took < Duration::from_secs(2), "round trips took {took:?}");
}

#[test]
fn wedged_group_on_a_busy_runtime_is_polled_by_the_backstop() {
    // The other worker never parks. Only its maintenance tick can reach the
    // stuck group's shard, and it does so once the shard has gone unpolled for
    // the backstop (10 sweep intervals), long before the stuck task returns.
    let took = round_trips_with_a_wedged_group(4, true, Duration::from_secs(10));
    assert!(took < Duration::from_secs(4), "round trips took {took:?}");
}

#[cfg(unix)]
#[test]
fn signals_and_child_exit_with_shards() {
    // Signal and process handling live on shard 0. With a busy or parked
    // shard-0 group they must still be delivered, whichever worker polls.
    use tokio::signal::unix::{signal, SignalKind};
    let rt = rt(8, 4);
    rt.block_on(async {
        let mut sig = signal(SignalKind::user_defined1()).unwrap();
        for _ in 0..3 {
            tokio::spawn(async {
                unsafe { libc::raise(libc::SIGUSR1) };
            });
            tokio::time::timeout(Duration::from_secs(5), sig.recv())
                .await
                .expect("signal not delivered")
                .unwrap();
        }
        let status = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new("true").status(),
        )
        .await
        .expect("child exit not observed")
        .unwrap();
        assert!(status.success());
    });
}

#[test]
#[should_panic(expected = "io_shard_sweep_interval must be non-zero")]
fn zero_sweep_interval_panics() {
    tokio::runtime::Builder::new_multi_thread().io_shard_sweep_interval(Some(Duration::ZERO));
}

#[test]
fn pre_park_polls_take_the_busy_batch() {
    // Pre-park polls take the busy batch, except that two saturated sweeps
    // within half the sweep interval earn one full-batch poll of the worker's
    // own shard (`Parker::sweep`). Each shard has `PER_SHARD` ready sockets.
    // One worker is released while the other group stays wedged: its first
    // sweep wakes `BUSY` tasks of its own shard, its second `BUSY` and then the
    // rest through the full-batch poll. The wedged shard is reached only
    // through help, so it drains `BUSY` at a time. Every task must run.
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
    use std::sync::{mpsc, Arc};
    use std::task::{Context, Wake, Waker};

    const SHARDS: usize = 2;
    const PER_SHARD: usize = 40;
    const SOCKETS: usize = SHARDS * PER_SHARD;
    const BUSY: usize = 8;

    struct Flags {
        woken: Vec<AtomicBool>,
        ran: Vec<AtomicBool>,
        // Per shard: the most tasks of that shard woken and not yet run, as
        // seen by one of its tasks when it ran.
        max_pending: Vec<AtomicUsize>,
        registered: AtomicUsize,
        done: AtomicUsize,
    }

    struct MarkWoken {
        flags: Arc<Flags>,
        index: usize,
        inner: Waker,
    }

    impl Wake for MarkWoken {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.flags.woken[self.index].store(true, SeqCst);
            self.inner.wake_by_ref();
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(SHARDS)
        .io_shards(SHARDS)
        .max_io_events_per_tick(128)
        .max_io_events_per_busy_tick(BUSY)
        // No maintenance tick, so only the pre-park polls run during the test.
        .event_interval(u32::MAX)
        // A long sweep gives the two saturated sweeps a 100 ms window, so the
        // test does not depend on how fast eight no-op tasks run.
        .io_shard_sweep_interval(Some(Duration::from_millis(200)))
        .enable_all()
        .build()
        .unwrap();

    let flags = Arc::new(Flags {
        woken: (0..SOCKETS).map(|_| AtomicBool::new(false)).collect(),
        ran: (0..SOCKETS).map(|_| AtomicBool::new(false)).collect(),
        max_pending: (0..SHARDS).map(|_| AtomicUsize::new(0)).collect(),
        registered: AtomicUsize::new(0),
        done: AtomicUsize::new(0),
    });

    // Sources are placed round-robin, so socket `i` is on shard `i % SHARDS`.
    let mut addrs = Vec::new();
    {
        let _guard = rt.enter();
        for index in 0..SOCKETS {
            let std_sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            std_sock.set_nonblocking(true).unwrap();
            addrs.push(std_sock.local_addr().unwrap());
            let sock = tokio::net::UdpSocket::from_std(std_sock).unwrap();
            let flags = flags.clone();
            rt.spawn(async move {
                let mut first = true;
                std::future::poll_fn(|cx: &mut Context<'_>| {
                    let waker = Waker::from(Arc::new(MarkWoken {
                        flags: flags.clone(),
                        index,
                        inner: cx.waker().clone(),
                    }));
                    let poll = sock.poll_recv_ready(&mut Context::from_waker(&waker));
                    if first {
                        first = false;
                        flags.registered.fetch_add(1, SeqCst);
                    }
                    poll
                })
                .await
                .unwrap();
                let shard = index % SHARDS;
                let pending = (shard..SOCKETS)
                    .step_by(SHARDS)
                    .filter(|&j| flags.woken[j].load(SeqCst) && !flags.ran[j].load(SeqCst))
                    .count();
                flags.ran[index].store(true, SeqCst);
                flags.max_pending[shard].fetch_max(pending, SeqCst);
                flags.done.fetch_add(1, SeqCst);
            });
        }
    }

    let wait_for = |what: &str, cond: &dyn Fn() -> bool| {
        let t0 = std::time::Instant::now();
        while !cond() {
            assert!(t0.elapsed() < Duration::from_secs(5), "timed out: {what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    wait_for("registration", &|| flags.registered.load(SeqCst) == SOCKETS);
    // Let the owners drain the writable events from registration.
    std::thread::sleep(Duration::from_millis(50));

    // Wedge both workers, so that all the readiness below waits in the kernel.
    let (wedged_tx, wedged_rx) = mpsc::channel();
    let mut releases = Vec::new();
    for _ in 0..SHARDS {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        releases.push(release_tx);
        let wedged_tx = wedged_tx.clone();
        rt.spawn(async move {
            wedged_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
    }
    for _ in 0..SHARDS {
        wedged_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    for addr in &addrs {
        sender.send_to(b"x", addr).unwrap();
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(flags.done.load(SeqCst), 0);

    // The released worker parks with no tasks. It polls its own shard, runs
    // what that woke, parks again, and so on; once its shard is drained it
    // helps the other shard, whose group is still wedged.
    releases[0].send(()).unwrap();
    wait_for("all tasks", &|| flags.done.load(SeqCst) == SOCKETS);
    releases[1].send(()).unwrap();

    // Which shard the released worker owns depends on where the wedging tasks
    // landed, so identify the shards by behaviour.
    let mut maxes: Vec<usize> = (0..SHARDS)
        .map(|shard| flags.max_pending[shard].load(SeqCst))
        .collect();
    maxes.sort_unstable();
    assert!(
        (1..=BUSY).contains(&maxes[0]),
        "helped shard: one sweep woke {} tasks, busy batch is {BUSY}",
        maxes[0]
    );
    // Own shard: 8 means the full-batch poll never fired, 16 that it took only
    // a busy batch, 40 that it fired on the first saturated sweep. 32 is the
    // expected value; 24 is one missed window under load.
    assert!(
        2 * BUSY < maxes[1] && maxes[1] <= PER_SHARD - BUSY,
        "own shard: one sweep woke {} tasks; the second saturated sweep should take the full batch",
        maxes[1]
    );
}

#[cfg(unix)]
#[test]
fn shard_keeps_a_poller_when_its_blocker_runs_a_task() {
    // Four workers, two shards, and a long sweep, so that only a shard's own
    // group can serve it promptly. Socket 0's handler blocks its thread when
    // it reads a marker: the thread that ran it is the one that was in
    // `epoll_wait` on shard 0, so shard 0 loses its poller to a non-yielding
    // task while its other worker is idle. That idle sibling must take over.
    // Socket 2 is on the same shard (round-robin placement) and is fed every
    // 2 ms; its echo latency shows whether anyone polled shard 0 meanwhile.
    // Before the standby hand-off the sibling slept on its condvar and shard 0
    // waited for another group's stale-help, about half the sweep (100 ms).
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
    use std::sync::Arc;

    const MARKER: u64 = u64::MAX;
    const BLOCK: Duration = Duration::from_millis(300);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .io_shards(2)
        .io_shard_sweep_interval(Some(Duration::from_millis(200)))
        .enable_all()
        .build()
        .unwrap();
    let t0 = std::time::Instant::now();
    // Sockets 0..4 land on shards 0, 1, 0, 1.
    let maxes: Vec<Arc<AtomicU64>> = (0..4).map(|_| Arc::new(AtomicU64::new(0))).collect();
    let mut writers = Vec::new();
    for m in &maxes {
        let (w, r) = std::os::unix::net::UnixStream::pair().unwrap();
        r.set_nonblocking(true).unwrap();
        let m = m.clone();
        rt.block_on(async {
            let mut r = tokio::net::UnixStream::from_std(r).unwrap();
            tokio::spawn(async move {
                let mut buf = [0u8; 8];
                while r.read_exact(&mut buf).await.is_ok() {
                    let sent = u64::from_le_bytes(buf);
                    if sent == MARKER {
                        std::thread::sleep(BLOCK);
                        continue;
                    }
                    let now = t0.elapsed().as_micros() as u64;
                    m.fetch_max(now.saturating_sub(sent), Relaxed);
                }
            });
        });
        writers.push(w);
    }
    let mut marker = writers.remove(0);
    let stop = Arc::new(AtomicBool::new(false));
    let feeder = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Relaxed) {
                for w in writers.iter_mut() {
                    let now = t0.elapsed().as_micros() as u64;
                    let _ = w.write_all(&now.to_le_bytes());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };

    let mut worst = Duration::ZERO;
    for _ in 0..6 {
        std::thread::sleep(Duration::from_millis(100));
        maxes[2].store(0, Relaxed);
        marker.write_all(&MARKER.to_le_bytes()).unwrap();
        std::thread::sleep(BLOCK + Duration::from_millis(50));
        worst = worst.max(Duration::from_micros(maxes[2].load(Relaxed)));
    }
    stop.store(true, Relaxed);
    feeder.join().unwrap();
    assert!(
        worst < Duration::from_millis(40),
        "shard 0 went unpolled for {worst:?} while an idle sibling slept"
    );
}
