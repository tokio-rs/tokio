//! I/O registration churn on the multi-thread runtime with one I/O driver
//! and with several (`Builder::io_shards`). Registering or dropping a socket
//! is an `epoll_ctl` on the driver's epoll instance, and those calls, the
//! driver's `epoll_wait`, and a read of `/proc/<pid>/fdinfo/<epfd>` all take
//! that instance's mutex. With `n` shards each instance sees `1/n` of them.
//!
//! Like the other runtime benches, this is a regression check for the
//! scheduler and driver, not a real-world benchmark.

#![cfg(unix)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::net::UdpSocket;
use tokio::runtime::{self, Runtime};

/// Sockets each task registers and drops per iteration.
const OPS_PER_TASK: usize = 200;

const SHARDS: [usize; 2] = [1, 8];

fn workers() -> usize {
    std::thread::available_parallelism().map_or(8, |n| n.get().min(64))
}

fn rt(shards: usize) -> Runtime {
    runtime::Builder::new_multi_thread()
        .worker_threads(workers())
        .io_shards(shards)
        .enable_all()
        .build()
        .unwrap()
}

/// One task per worker; each registers and drops `OPS_PER_TASK` UDP sockets.
/// Returns the longest single registration.
fn churn(rt: &Runtime) -> Duration {
    let worst = Arc::new(AtomicU64::new(0));
    rt.block_on(async {
        let tasks: Vec<_> = (0..workers())
            .map(|_| {
                let worst = Arc::clone(&worst);
                tokio::spawn(async move {
                    for _ in 0..OPS_PER_TASK {
                        let std = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
                        std.set_nonblocking(true).unwrap();
                        let t = Instant::now();
                        let s = UdpSocket::from_std(std).unwrap();
                        worst.fetch_max(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        drop(s);
                    }
                })
            })
            .collect();
        for t in tasks {
            t.await.unwrap();
        }
    });
    Duration::from_nanos(worst.load(Ordering::Relaxed))
}

/// Throughput of concurrent register/deregister across all workers.
fn register_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("io_register_churn");
    group.sample_size(20);
    for shards in SHARDS {
        let rt = rt(shards);
        group.bench_with_input(BenchmarkId::new("shards", shards), &rt, |b, rt| {
            b.iter(|| churn(rt));
        });
    }
    group.finish();
}

/// The same churn while another thread reads each epoll instance's fdinfo in
/// a loop, as a host agent that inventories file descriptors does. Each read
/// walks every registration under the instance's mutex and stalls `epoll_ctl`
/// for the length of the walk; the held idle sockets make the walk long.
/// `throughput` times the batch; `worst_register` reports the longest single
/// registration instead, which is the stall one connecting client sees.
#[cfg(target_os = "linux")]
mod fdinfo {
    use super::*;
    use std::sync::atomic::AtomicBool;

    const HELD: usize = 20_000;

    /// A runtime with `shards` drivers, `HELD` idle sockets registered on it,
    /// and a thread reading its epoll fdinfo until the guard is dropped.
    struct Fixture {
        rt: Runtime,
        _held: Vec<UdpSocket>,
        stop: Arc<AtomicBool>,
        reader: Option<std::thread::JoinHandle<()>>,
    }

    impl Fixture {
        fn new(shards: usize) -> Fixture {
            let rt = rt(shards);
            let held = HELD.min(fd_limit() / 2);
            let _held = rt.block_on(async {
                let mut v = Vec::with_capacity(held);
                for _ in 0..held {
                    v.push(UdpSocket::bind("127.0.0.1:0").await.unwrap());
                }
                v
            });
            let paths = epoll_fdinfo_paths();
            let stop = Arc::new(AtomicBool::new(false));
            let reader = {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        for p in &paths {
                            let _ = std::fs::read(p);
                        }
                    }
                })
            };
            Fixture {
                rt,
                _held,
                stop,
                reader: Some(reader),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(r) = self.reader.take() {
                r.join().unwrap();
            }
        }
    }

    fn epoll_fdinfo_paths() -> Vec<std::path::PathBuf> {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                std::fs::read_link(e.path())
                    .map(|t| t.to_string_lossy().contains("eventpoll"))
                    .unwrap_or(false)
            })
            .map(|e| std::path::PathBuf::from("/proc/self/fdinfo").join(e.file_name()))
            .collect()
    }

    fn fd_limit() -> usize {
        let mut lim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `lim` is a valid out-pointer for `getrlimit`.
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } == 0 {
            lim.rlim_cur as usize
        } else {
            1024
        }
    }

    pub(super) fn throughput(c: &mut Criterion) {
        let mut group = c.benchmark_group("io_register_churn_during_fdinfo_reads");
        group.sample_size(10);
        for shards in SHARDS {
            let f = Fixture::new(shards);
            group.bench_with_input(BenchmarkId::new("shards", shards), &f.rt, |b, rt| {
                b.iter(|| churn(rt));
            });
        }
        group.finish();
    }

    pub(super) fn worst_register(c: &mut Criterion) {
        let mut group = c.benchmark_group("io_worst_register_during_fdinfo_reads");
        group.sample_size(10);
        for shards in SHARDS {
            let f = Fixture::new(shards);
            group.bench_with_input(BenchmarkId::new("shards", shards), &f.rt, |b, rt| {
                b.iter_custom(|iters| (0..iters).map(|_| churn(rt)).sum());
            });
        }
        group.finish();
    }
}

#[cfg(target_os = "linux")]
criterion_group!(
    io_shards,
    register_churn,
    fdinfo::throughput,
    fdinfo::worst_register
);
#[cfg(not(target_os = "linux"))]
criterion_group!(io_shards, register_churn);
criterion_main!(io_shards);
