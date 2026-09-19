//! Benchmark the path from `accept` to the first read and write on the
//! accepted socket. The peer's request is already queued when the socket is
//! accepted, as it is for a client racing a busy server, so the first read
//! never needs to wait; this measures whether the runtime makes it wait for
//! the I/O driver anyway. These benches are a form of regression testing and
//! not a general purpose benchmark.
//!
//! Idle shapes, time per connection:
//! - `block_on`: accept + read + write inside `block_on`, one connection per
//!   iteration.
//! - `serial_task`: one spawned task accepts and serves a batch of connections
//!   in order, never yielding except on I/O.
//! - `task_per_conn`: an accept loop spawns one task per connection (the usual
//!   server shape).
//!
//! `live/*`: a current-thread server runs on its own thread for the whole
//! bench and also drains `busy` other connections that a writer thread keeps
//! readable; the timed quantity is a blocking client's connect + request +
//! 2-byte reply. `inline_first_read` reads the request in the accept loop,
//! `task_per_conn` in a spawned task, `cap8` sets `max_io_events_per_tick(8)`.

use std::io::Write;
use std::net::{SocketAddr, TcpStream as StdStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::{self, Runtime};

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

const REQUEST: &[u8] = &[1; 64];
const BATCH: u64 = 64;

/// A client that has connected and sent its request before the server accepts.
fn client(addr: SocketAddr) -> StdStream {
    let mut c = StdStream::connect(addr).unwrap();
    c.set_nodelay(true).unwrap();
    c.write_all(REQUEST).unwrap();
    c
}

async fn serve(mut s: TcpStream) {
    let mut buf = [0u8; REQUEST.len()];
    s.read_exact(&mut buf).await.unwrap();
    s.write_all(b"ok").await.unwrap();
}

fn rt_current_thread() -> Runtime {
    runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap()
}

fn rt_multi_thread(workers: usize) -> Runtime {
    runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_io()
        .build()
        .unwrap()
}

fn block_on(c: &mut Criterion, name: &str, rt: Runtime) {
    let listener = rt.block_on(TcpListener::bind("127.0.0.1:0")).unwrap();
    let addr = listener.local_addr().unwrap();

    c.bench_function(name, |b| {
        b.iter_batched(
            || client(addr),
            |_client| {
                rt.block_on(async {
                    let (s, _) = listener.accept().await.unwrap();
                    serve(s).await;
                })
            },
            BatchSize::PerIteration,
        )
    });
}

/// Runs `iters` connections in batches of `BATCH`: the clients for a batch are
/// connected and have written before the timed section, which runs `server`
/// for that batch as one spawned task.
fn spawned<F, Fut>(c: &mut Criterion, name: &str, rt: Runtime, server: F)
where
    F: Fn(Arc<TcpListener>, u64) -> Fut + Copy + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = Arc::new(rt.block_on(TcpListener::bind("127.0.0.1:0")).unwrap());
    let addr = listener.local_addr().unwrap();

    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            let mut left = iters;
            while left > 0 {
                let n = left.min(BATCH);
                left -= n;
                let clients: Vec<StdStream> = (0..n).map(|_| client(addr)).collect();
                let l = listener.clone();
                let start = Instant::now();
                rt.block_on(async move { tokio::spawn(server(l, n)).await.unwrap() });
                total += start.elapsed();
                drop(clients);
            }
            total
        })
    });
}

async fn serial_task(listener: Arc<TcpListener>, n: u64) {
    for _ in 0..n {
        let (s, _) = listener.accept().await.unwrap();
        serve(s).await;
    }
}

async fn task_per_conn(listener: Arc<TcpListener>, n: u64) {
    let mut handlers = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let (s, _) = listener.accept().await.unwrap();
        handlers.push(tokio::spawn(serve(s)));
    }
    for h in handlers {
        h.await.unwrap();
    }
}

fn block_on_current_thread(c: &mut Criterion) {
    block_on(c, "block_on/current_thread", rt_current_thread());
}

fn block_on_multi_thread_1(c: &mut Criterion) {
    block_on(c, "block_on/multi_thread_1", rt_multi_thread(1));
}

fn serial_task_multi_thread_1(c: &mut Criterion) {
    spawned(
        c,
        "serial_task/multi_thread_1",
        rt_multi_thread(1),
        serial_task,
    );
}

fn serial_task_multi_thread_4(c: &mut Criterion) {
    spawned(
        c,
        "serial_task/multi_thread_4",
        rt_multi_thread(4),
        serial_task,
    );
}

fn task_per_conn_multi_thread_1(c: &mut Criterion) {
    spawned(
        c,
        "task_per_conn/multi_thread_1",
        rt_multi_thread(1),
        task_per_conn,
    );
}

fn task_per_conn_multi_thread_4(c: &mut Criterion) {
    spawned(
        c,
        "task_per_conn/multi_thread_4",
        rt_multi_thread(4),
        task_per_conn,
    );
}

criterion_group!(
    net_accept,
    block_on_current_thread,
    block_on_multi_thread_1,
    serial_task_multi_thread_1,
    serial_task_multi_thread_4,
    task_per_conn_multi_thread_1,
    task_per_conn_multi_thread_4
);

// ---------------------------------------------------------------------------
// Live server: the runtime runs for the whole bench on its own thread and
// keeps draining the background connections, so both sides do the same
// background work; the client measures connect -> request -> reply.

use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

struct Live {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    drained: Arc<AtomicU64>,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
        // The server thread is left running; it holds no resources the next
        // bench needs and exits with the process.
        eprintln!(
            "  (background bytes drained by the server: {})",
            self.drained.load(Relaxed)
        );
    }
}

fn live(busy: usize, inline: bool, events_per_tick: Option<usize>) -> Live {
    let std_l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let std_bg = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std_l.set_nonblocking(true).unwrap();
    std_bg.set_nonblocking(true).unwrap();
    let (addr, bg_addr) = (std_l.local_addr().unwrap(), std_bg.local_addr().unwrap());
    let accepted = Arc::new(AtomicU64::new(0));
    let drained = Arc::new(AtomicU64::new(0));
    let (accepted2, drained2) = (accepted.clone(), drained.clone());

    std::thread::spawn(move || {
        let mut b = runtime::Builder::new_current_thread();
        b.enable_io();
        if let Some(n) = events_per_tick {
            b.max_io_events_per_tick(n);
        }
        let rt = b.build().unwrap();
        rt.block_on(async move {
            let bg = TcpListener::from_std(std_bg).unwrap();
            tokio::spawn(async move {
                loop {
                    let (mut s, _) = bg.accept().await.unwrap();
                    accepted2.fetch_add(1, Relaxed);
                    let drained = drained2.clone();
                    tokio::spawn(async move {
                        let mut buf = [0u8; 256];
                        loop {
                            match s.read(&mut buf).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    drained.fetch_add(n as u64, Relaxed);
                                }
                            }
                        }
                    });
                }
            });
            let l = TcpListener::from_std(std_l).unwrap();
            loop {
                let (s, _) = l.accept().await.unwrap();
                if inline {
                    serve(s).await;
                } else {
                    tokio::spawn(serve(s));
                }
            }
        });
    });

    let mut clients = Vec::with_capacity(busy);
    for _ in 0..busy {
        let c = StdStream::connect(bg_addr).unwrap();
        c.set_nonblocking(true).unwrap();
        c.set_nodelay(true).unwrap();
        clients.push(c);
    }
    while accepted.load(Relaxed) < busy as u64 {
        std::thread::sleep(Duration::from_millis(1));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let writer = std::thread::spawn(move || {
        let chunk = [2u8; 32];
        while !stop2.load(Relaxed) {
            for c in &mut clients {
                let _ = c.write(&chunk);
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    });
    Live {
        addr,
        stop,
        drained,
        writer: Some(writer),
    }
}

fn live_bench(c: &mut Criterion, name: &str, busy: usize, inline: bool, cap: Option<usize>) {
    let srv = live(busy, inline, cap);
    c.bench_function(name, |b| {
        b.iter(|| {
            let mut c = StdStream::connect(srv.addr).unwrap();
            c.set_nodelay(true).unwrap();
            c.write_all(REQUEST).unwrap();
            let mut reply = [0u8; 2];
            c.read_exact(&mut reply).unwrap();
        })
    });
}

fn live_task_per_conn_0(c: &mut Criterion) {
    live_bench(c, "live/task_per_conn/busy_0", 0, false, None);
}
fn live_task_per_conn_1024(c: &mut Criterion) {
    live_bench(c, "live/task_per_conn/busy_1024", 1024, false, None);
}
fn live_inline_0(c: &mut Criterion) {
    live_bench(c, "live/inline_first_read/busy_0", 0, true, None);
}
fn live_inline_1024(c: &mut Criterion) {
    live_bench(c, "live/inline_first_read/busy_1024", 1024, true, None);
}
fn live_task_per_conn_cap8_1024(c: &mut Criterion) {
    live_bench(c, "live/task_per_conn_cap8/busy_1024", 1024, false, Some(8));
}

criterion_group!(
    net_accept_live,
    live_task_per_conn_0,
    live_task_per_conn_1024,
    live_inline_0,
    live_inline_1024,
    live_task_per_conn_cap8_1024
);
criterion_main!(net_accept, net_accept_live);
