//! Benchmark spawning tasks of different sizes.
//! This measures the time to start the task and wait the result.

use std::future::Future;

use bencher::black_box;
use tokio::runtime::Runtime;

const NITER: usize = 50000;

struct SizedFuture<const SIZE: usize> {
    _data: [u8; SIZE],
}

impl<const SIZE: usize> SizedFuture<SIZE> {
    fn new() -> Self {
        Self {
            _data: unsafe { std::mem::MaybeUninit::uninit().assume_init() },
        }
    }
}

impl<const SIZE: usize> Future for SizedFuture<SIZE> {
    type Output = u32;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(0)
    }
}

async fn bench_spawn<const SIZE: usize, const BATCH: usize>() -> u128 {
    let begin = std::time::Instant::now();
    for _ in 0..NITER {
        if BATCH == 1 {
            tokio::task::spawn(SizedFuture::<SIZE>::new())
                .await
                .unwrap();
        } else {
            let mut vec = Vec::with_capacity(BATCH);
            for _ in 0..BATCH {
                vec.push(tokio::task::spawn(SizedFuture::<SIZE>::new()));
            }

            for handle in vec {
                black_box(handle.await.unwrap());
            }
        }
    }
    let dur = begin.elapsed();

    dur.as_nanos() / (NITER as u128)
}

fn bench_block_on<const SIZE: usize>(rt: &Runtime) -> u128 {
    let begin = std::time::Instant::now();
    for _ in 0..NITER {
        let ans = rt.block_on(SizedFuture::<SIZE>::new());
        black_box(ans);
    }
    let dur = begin.elapsed();

    dur.as_nanos() / (NITER as u128)
}

fn bench<const SIZE: usize>() {
    let mut data = vec![];
    {
        let curr = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        data.push(curr.block_on(bench_spawn::<SIZE, 1>()));
        data.push(curr.block_on(bench_spawn::<SIZE, 10>()));
        data.push(curr.block_on(bench_spawn::<SIZE, 50>()));
        data.push(bench_block_on::<SIZE>(&curr));
    }

    {
        let multi = tokio::runtime::Builder::new_multi_thread().build().unwrap();
        data.push(multi.block_on(bench_spawn::<SIZE, 1>()));
        data.push(multi.block_on(bench_spawn::<SIZE, 10>()));
        data.push(multi.block_on(bench_spawn::<SIZE, 50>()));
        data.push(bench_block_on::<SIZE>(&multi));
    }

    print!("|{:>7} |", SIZE);
    for v in data {
        print!("{:>6} ns |", v);
    }
    println!();
}

fn print_sep() {
    println!(
        "+{0:-^8}+{0:-^10}+{0:-^10}+{0:-^10}+{0:-^10}+{0:-^10}+{0:-^10}+{0:-^10}+{0:-^10}+",
        ""
    )
}

fn main() {
    if let Some(filter) = std::env::args().skip(1).find(|arg| *arg != "--bench") {
        if !"spawn_size".contains(&filter) {
            return;
        }
    }

    println!();
    println!("Benchmark performance of spawn/block_on tasks of different sizes.");
    println!();
    println!("+{0:-<8}+{0:-^43}+{0:-^43}+", "");
    println!(
        "|{:>8}|{:^43}|{:^43}|",
        "", "current_thread", "multi_thread"
    );
    print_sep();
    println!(
        "|{:^8}|{:^10}|{:^10}|{:^10}|{:^10}|{:^10}|{:^10}|{:^10}|{:^10}|",
        "size",
        "spawn 1",
        "spawn 10",
        "spawn 50",
        "block_on",
        "spawn 1",
        "spawn 10",
        "spawn 50",
        "block_on"
    );
    print_sep();

    bench::<16>();
    bench::<32>();
    bench::<64>();
    bench::<128>();
    bench::<256>();
    bench::<512>();
    bench::<1024>();
    bench::<2048>();
    bench::<4096>();
    bench::<8192>();

    print_sep();
}
