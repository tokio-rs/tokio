use criterion::{criterion_group, criterion_main, Criterion};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use tokio::io::{copy, repeat, AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::time::{interval, Interval, MissedTickBehavior};

use std::task::Poll;
use std::time::Duration;

const KILO: usize = 1024;

// Tunable parameters if you want to change this benchmark. If reader and writer
// are matched in kilobytes per second, then this only exposes buffering to the
// benchmark.
const RNG_SEED: u64 = 0;
// How much data to copy in a single benchmark run
const SOURCE_SIZE: u64 = 256 * KILO as u64;
// Read side provides CHUNK_SIZE every READ_SERVICE_PERIOD. If it's not called
// frequently, it'll burst to catch up (representing OS buffers draining)
const CHUNK_SIZE: usize = 2 * KILO;
const READ_SERVICE_PERIOD: Duration = Duration::from_millis(1);
// Write side buffers up to WRITE_BUFFER, and flushes to disk every
// WRITE_SERVICE_PERIOD.
const WRITE_BUFFER: usize = 40 * KILO;
const WRITE_SERVICE_PERIOD: Duration = Duration::from_millis(20);
// How likely you are to have to wait for previously written data to be flushed
// because another writer claimed the buffer space
const PROBABILITY_FLUSH_WAIT: f64 = 0.1;

/// A slow writer that aims to simulate HDD behavior under heavy load.
///
/// There is a limited buffer, which is fully drained on the next write after
/// a time limit is reached. Flush waits for the time limit to be reached
/// and then drains the buffer.
///
/// At random, the HDD will stall writers while it flushes out all buffers. If
/// this happens to you, you will be unable to write until the next time the
/// buffer is drained.
struct SlowHddWriter {
    service_intervals: Interval,
    blocking_rng: ChaCha20Rng,
    buffer_size: usize,
    buffer_used: usize,
}

impl SlowHddWriter {
    fn new(service_interval: Duration, buffer_size: usize) -> Self {
        let blocking_rng = ChaCha20Rng::seed_from_u64(RNG_SEED);
        let mut service_intervals = interval(service_interval);
        service_intervals.set_missed_tick_behavior(MissedTickBehavior::Delay);
        Self {
            service_intervals,
            blocking_rng,
            buffer_size,
            buffer_used: 0,
        }
    }

    fn service_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        // If we hit a service interval, the buffer can be cleared
        let res = self.service_intervals.poll_tick(cx).map(|_| Ok(()));
        if res.is_ready() {
            self.buffer_used = 0;
        }
        res
    }

    fn write_bytes(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        writeable: usize,
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let service_res = self.as_mut().service_write(cx);

        if service_res.is_pending() && self.blocking_rng.random_bool(PROBABILITY_FLUSH_WAIT) {
            return Poll::Pending;
        }
        let available = self.buffer_size - self.buffer_used;

        if available == 0 {
            assert!(service_res.is_pending());
            Poll::Pending
        } else {
            let written = available.min(writeable);
            self.buffer_used += written;
            Poll::Ready(Ok(written))
        }
    }
}

impl Unpin for SlowHddWriter {}

impl AsyncWrite for SlowHddWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        self.write_bytes(cx, buf.len())
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.service_write(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        self.service_write(cx)
    }

    fn poll_write_vectored(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let writeable = bufs.iter().fold(0, |acc, buf| acc + buf.len());
        self.write_bytes(cx, writeable)
    }

    fn is_write_vectored(&self) -> bool {
        true
    }
}

/// A reader that limits the maximum chunk it'll give you back
///
/// Simulates something reading from a slow link - you get one chunk per call,
/// and you are offered chunks on a schedule
struct ChunkReader {
    data: Vec<u8>,
    service_intervals: Interval,
}

impl ChunkReader {
    fn new(chunk_size: usize, service_interval: Duration) -> Self {
        let mut service_intervals = interval(service_interval);
        service_intervals.set_missed_tick_behavior(MissedTickBehavior::Burst);
        let data: Vec<u8> = std::iter::repeat_n(0, chunk_size).collect();
        Self {
            data,
            service_intervals,
        }
    }
}

impl AsyncRead for ChunkReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.service_intervals.poll_tick(cx).is_pending() {
            return Poll::Pending;
        }
        buf.put_slice(&self.data[..buf.remaining().min(self.data.len())]);
        Poll::Ready(Ok(()))
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}

fn copy_mem_to_mem(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("copy_mem_to_mem", |b| {
        b.iter(|| {
            let task = || async {
                let mut source = repeat(0).take(SOURCE_SIZE);
                let mut dest = Vec::new();
                copy(&mut source, &mut dest).await.unwrap();
            };

            rt.block_on(task());
        })
    });
}

fn copy_mem_to_slow_hdd(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("copy_mem_to_slow_hdd", |b| {
        b.iter(|| {
            let task = || async {
                let mut source = repeat(0).take(SOURCE_SIZE);
                let mut dest = SlowHddWriter::new(WRITE_SERVICE_PERIOD, WRITE_BUFFER);
                copy(&mut source, &mut dest).await.unwrap();
            };

            rt.block_on(task());
        })
    });
}

fn copy_chunk_to_mem(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("copy_chunk_to_mem", |b| {
        b.iter(|| {
            let task = || async {
                let mut source =
                    ChunkReader::new(CHUNK_SIZE, READ_SERVICE_PERIOD).take(SOURCE_SIZE);
                let mut dest = Vec::new();
                copy(&mut source, &mut dest).await.unwrap();
            };

            rt.block_on(task());
        })
    });
}

fn copy_chunk_to_slow_hdd(c: &mut Criterion) {
    let rt = rt();

    c.bench_function("copy_chunk_to_slow_hdd", |b| {
        b.iter(|| {
            let task = || async {
                let mut source =
                    ChunkReader::new(CHUNK_SIZE, READ_SERVICE_PERIOD).take(SOURCE_SIZE);
                let mut dest = SlowHddWriter::new(WRITE_SERVICE_PERIOD, WRITE_BUFFER);
                copy(&mut source, &mut dest).await.unwrap();
            };

            rt.block_on(task());
        })
    });
}

criterion_group!(
    copy_bench,
    copy_mem_to_mem,
    copy_mem_to_slow_hdd,
    copy_chunk_to_mem,
    copy_chunk_to_slow_hdd,
);
criterion_main!(copy_bench);
