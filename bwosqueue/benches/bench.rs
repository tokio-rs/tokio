//! Microbenchmarks to benchmark the BWoS queue and compare to the original queue in tokio.
//!
//! Please note that the tokio queue stores `task::Notified<T>` items, which boils down to a
//! `NonNull` pointer, so we benchmark with a u64 as the queue item.

use core::sync::atomic::Ordering::{Acquire, Relaxed, Release, SeqCst};
use std::arch::asm;
use std::sync::atomic::fence;
use std::time::{Duration, Instant};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc,
    },
    thread::{self},
};

#[path = "support/original_tokio_queue.rs"]
mod original_tokio_queue;

#[path = "support/original_bwos.rs"]
mod original_bwos;

use bwosqueue::{Owner, Stealer};
use criterion::{
    black_box, criterion_group, criterion_main, measurement::WallTime, BenchmarkGroup, BenchmarkId,
    Criterion, Throughput,
};

fn bench_stealing(c: &mut Criterion) {
    let mut stealer_group = c.benchmark_group("Stealing");
    let bwos_single = QueueType::BwosStealSingleItems;
    let bwos_block = QueueType::BwosStealBlocks;
    let tokio_q_single = QueueType::TokioStealSingleItems;
    let tokio_q_batch = QueueType::TokioStealHalf;

    bench_steal::<8, 32>(&mut stealer_group, tokio_q_single, 0);
    bench_steal::<8, 32>(&mut stealer_group, tokio_q_single, 1);
    bench_steal::<8, 32>(&mut stealer_group, tokio_q_single, 2);
    bench_steal::<8, 32>(&mut stealer_group, tokio_q_batch, 1);
    bench_steal::<8, 32>(&mut stealer_group, tokio_q_batch, 2);

    bench_steal::<8, 32>(&mut stealer_group, bwos_single, 0);
    bench_steal::<8, 32>(&mut stealer_group, bwos_single, 1);
    bench_steal::<8, 32>(&mut stealer_group, bwos_block, 0);
    bench_steal::<8, 32>(&mut stealer_group, bwos_block, 1);
}

fn simple_enqueue_dequeue(c: &mut Criterion) {
    let mut group = c.benchmark_group("Simple Enqueue Dequeue");
    simple_enqueue_dequeue_original_queue_inner::<{ 8 * 32 }>(&mut group);
    simple_enqueue_dequeue_original_queue_inner::<{ 8 * 128 }>(&mut group);
    simple_enqueue_dequeue_original_queue_inner::<{ 8 * 512 }>(&mut group);
    simple_enqueue_dequeue_original_queue_inner::<{ 8 * 1024 }>(&mut group);
    simple_enqueue_dequeue_inner::<8, 32>(&mut group);
    simple_enqueue_dequeue_inner::<8, 128>(&mut group);
    simple_enqueue_dequeue_inner::<8, 512>(&mut group);
    simple_enqueue_dequeue_inner::<8, 1024>(&mut group);
    simple_enqueue_dequeue_inner::<32, 8>(&mut group);
    simple_enqueue_dequeue_inner::<128, 2>(&mut group);
    simple_enqueue_dequeue_inner::<256, 1>(&mut group);
    simple_enqueue_dequeue_original_bwos_queue(&mut group);
}

#[inline(never)]
fn bwos_enq_deq<const NB: usize, const NE: usize>(owner: &mut bwosqueue::Owner<u64, NB, NE>) {
    while owner.enqueue(black_box(5)).is_ok() {}
    loop {
        if let Some(val) = owner.dequeue() {
            assert_eq!(black_box(val), 5_u64);
        } else {
            break;
        };
    }
}

fn simple_enqueue_dequeue_inner<const NB: usize, const NE: usize>(
    group: &mut BenchmarkGroup<WallTime>,
) {
    let (mut owner, _) = bwosqueue::new::<u64, NB, NE>();
    let num_elements = NE;

    group.throughput(Throughput::Elements((NB * NE * 2) as u64));
    group.bench_with_input(
        BenchmarkId::new(
            format!("BWoS {NE} Elems per Block"),
            format!("{} Total size", NB * NE),
        ),
        &num_elements,
        |b, _num_elements| {
            b.iter(|| {
                bwos_enq_deq(&mut owner);
            });
            #[cfg(feature = "stats")]
            assert!(!owner.can_consume())
        },
    );
}

fn simple_enqueue_dequeue_original_queue_inner<const SIZE: usize>(
    group: &mut BenchmarkGroup<WallTime>,
) {
    let (_, mut owner) = original_tokio_queue::local::<u64, SIZE>();
    group.throughput(Throughput::Elements((SIZE * 2) as u64));
    // todo: we could do a binary search here by doing dry runs to determine how much
    // idle time we need to reach a certain stealing percentage
    group.bench_with_input(
        BenchmarkId::new("Original tokio queue", format!("{SIZE} Total size")),
        &SIZE,
        |b, _num_elements| {
            b.iter(|| {
                while owner.push_back(black_box(5)).is_ok() {}
                loop {
                    if let Some(val) = owner.pop() {
                        assert_eq!(black_box(val), 5_u64);
                    } else {
                        break;
                    };
                }
            });
            assert!(!owner.has_tasks())
        },
    );

    // one full enqueue + one full dequeue
}

#[inline(never)]
fn simple_enqueue_dequeue_original_bwos_queue(group: &mut BenchmarkGroup<WallTime>) {
    #[inline(never)]
    fn enq_deq(prod: &mut original_bwos::Producer<u64>, cons: &mut original_bwos::Consumer<u64>) {
        while prod.enqueue(black_box(5)) {}
        loop {
            if let Some(val) = cons.dequeue() {
                assert_eq!(black_box(val), 5_u64);
            } else {
                break;
            };
        }
    }
    let (mut prod, mut cons, _stealer) = original_bwos::new();

    const SIZE: u64 = (1024 * 8) as u64;
    group.throughput(Throughput::Elements(SIZE * 2));
    group.bench_with_input(
        BenchmarkId::new("Unsafe Rust: enq-deq", format!("{SIZE} Total size")),
        &SIZE,
        |b, _num_elements| {
            b.iter(|| {
                enq_deq(&mut prod, &mut cons);
            });
            // use owner outside of iter to control drop
            assert!(cons.dequeue().is_none())
        },
    );

    // one full enqueue + one full dequeue
}

enum StealKind<const NB: usize, const NE: usize> {
    BwosSingleSteal(Stealer<u64, NB, NE>),
    BwosBlockSteal(Stealer<u64, NB, NE>),
    // Just use 8K for the tokio queue, since the size doesn't really matter for this queue and we can't
    // use generic const expressions here.
    TokioSingleSteal(original_tokio_queue::Steal<u64, 8192>),
    TokioBatchSteal(original_tokio_queue::Steal<u64, 8192>),

    // This version also currently has 8K hardcoded, and it's not really worth it to modify it, since it is
    // only used as a baseline for benchmarks.
    BwosUnsafeSingleSteal(original_bwos::Stealer<u64>),
}

struct StealTest<const NB: usize, const NE: usize> {
    owner: QueueOwner<NB, NE>,
    stealer: StealKind<NB, NE>,
    params: StealTestParams,
}

#[derive(Clone)]
struct StealTestParams {
    num_stealers: usize,
    num_ready_stealers: Arc<AtomicUsize>,
    start: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    stealer_idle_ops: usize,
}

fn bwos_steal_block_thread<const NB: usize, const NE: usize>(
    stealer: Stealer<u64, NB, NE>,
    params: StealTestParams,
) {
    params.num_ready_stealers.fetch_add(1, Release);
    let mut idle_iterations: u64 = 0;
    loop {
        if let Some(stolen_iter) = stealer.steal_block() {
            for element in stolen_iter {
                assert_eq!(element, 5);
            }
            // Stealing should be a "rare" operation, so configure the stealing frequency by waiting for
            // a while after a successfully stolen item.
            for _ in 0..params.stealer_idle_ops {
                unsafe { asm!("nop") };
            }
        } else {
            // Only check the atomic variable once in a while to reduce the overhead.
            idle_iterations = idle_iterations.wrapping_add(1);
            if (idle_iterations % 1024) == 0 {
                if params.stop.load(Relaxed) {
                    params.num_ready_stealers.fetch_sub(1, SeqCst);
                    return ();
                }
            }
        }
    }
}

fn bwos_steal_single_item_thread<const NB: usize, const NE: usize>(
    stealer: Stealer<u64, NB, NE>,
    params: StealTestParams,
) {
    params.num_ready_stealers.fetch_add(1, Release);
    let mut iterations: u64 = 0;
    loop {
        if let Some(val) = stealer.steal() {
            assert_eq!(val, 5);
            for _ in 0..params.stealer_idle_ops {
                unsafe { asm!("nop") };
            }
        } else {
            iterations = iterations.wrapping_add(1);
            if (iterations % 1024) == 0 {
                if params.stop.load(Relaxed) {
                    params.num_ready_stealers.fetch_sub(1, SeqCst);
                    return ();
                }
            }
        }
    }
}

fn bwos_unsafe_steal_single_item_thread(
    mut stealer: original_bwos::Stealer<u64>,
    params: StealTestParams,
) {
    params.num_ready_stealers.fetch_add(1, Release);
    let mut iterations: u64 = 0;
    loop {
        if let Some(val) = stealer.steal() {
            assert_eq!(val, 5);
            for _ in 0..params.stealer_idle_ops {
                unsafe { asm!("nop") };
            }
        } else {
            iterations = iterations.wrapping_add(1);
            if (iterations % 1024) == 0 {
                if params.stop.load(Relaxed) {
                    params.num_ready_stealers.fetch_sub(1, SeqCst);
                    return ();
                }
            }
        }
    }
}

fn tokio_q_steal_block_thread(
    stealer: original_tokio_queue::Steal<u64, 8192>,
    params: StealTestParams,
) {
    params.num_ready_stealers.fetch_add(1, Release);
    let mut iterations: u64 = 0;
    loop {
        if stealer.bench_tokio_q_steal(1024) != 0 {
            for _ in 0..params.stealer_idle_ops {
                unsafe { asm!("nop") };
            }
        } else {
            iterations = iterations.wrapping_add(1);
            if (iterations % 1024) == 0 {
                if params.stop.load(Relaxed) {
                    params.num_ready_stealers.fetch_sub(1, SeqCst);
                    return ();
                }
            }
        }
    }
}

fn tokio_q_steal_single_item_thread(
    stealer: original_tokio_queue::Steal<u64, 8192>,
    params: StealTestParams,
) {
    params.num_ready_stealers.fetch_add(1, Release);
    let mut iterations: u64 = 0;
    loop {
        if let Some(val) = stealer.bench_tokio_steal_single() {
            assert_eq!(val, 5);
            for _ in 0..params.stealer_idle_ops {
                unsafe { asm!("nop") };
            }
        } else {
            iterations = iterations.wrapping_add(1);
            if (iterations % 1024) == 0 {
                if params.stop.load(Relaxed) {
                    params.num_ready_stealers.fetch_sub(1, SeqCst);
                    return ();
                }
            }
        }
    }
}

/// Sets up stealers to only steals items, without enqueuing them into a different queue.
/// This allows us to measure only the overhead of the stealing operation, without
/// any side effects from an enqueue into a different queue.
fn setup_stealers<const NB: usize, const NE: usize>(steal_test: &StealTest<NB, NE>) {
    let params = &steal_test.params;
    // ensure any remaining stealer threads from previous run have shutdown.
    while params.num_ready_stealers.load(SeqCst) != 0 {}
    params.stop.store(false, SeqCst);
    params.start.store(0, SeqCst);

    assert_eq!(params.num_ready_stealers.load(SeqCst), 0);
    for _ in 0..params.num_stealers {
        let l_params = params.clone();

        match &steal_test.stealer {
            StealKind::BwosSingleSteal(stealer) => {
                let l_stealer = stealer.clone();
                thread::spawn(|| bwos_steal_single_item_thread(l_stealer, l_params));
            }
            StealKind::BwosBlockSteal(stealer) => {
                let l_stealer = stealer.clone();
                thread::spawn(|| bwos_steal_block_thread(l_stealer, l_params));
            }
            StealKind::TokioSingleSteal(stealer) => {
                let l_stealer = stealer.clone();
                thread::spawn(|| tokio_q_steal_single_item_thread(l_stealer, l_params));
            }
            StealKind::TokioBatchSteal(stealer) => {
                let l_stealer = stealer.clone();
                thread::spawn(|| tokio_q_steal_block_thread(l_stealer, l_params));
            }
            StealKind::BwosUnsafeSingleSteal(stealer) => {
                let l_stealer = stealer.clone();
                thread::spawn(|| bwos_unsafe_steal_single_item_thread(l_stealer, l_params));
            }
        }
    }
    while params.num_ready_stealers.load(Acquire) != params.num_stealers {}
}

// Owner thread implementation which enqueues for a configurable amount of items
// as fast as possible, dequeuing until empty once the queue is full.
fn bwos_owner_thread<const NB: usize, const NE: usize>(
    owner: &mut Owner<u64, NB, NE>,
    num_enqueues: u64,
    total_enqueues: &mut u64,
    total_dequeues: &mut u64,
) -> Duration {
    let mut enq_count: u64 = 0;
    let mut deq_count: u64 = 0;
    let start = Instant::now();

    while enq_count < num_enqueues {
        while owner.enqueue(black_box(5)).is_ok() {
            enq_count += 1;
            if enq_count >= num_enqueues {
                break;
            }
        }
        loop {
            if let Some(val) = owner.dequeue() {
                assert_eq!(black_box(val), 5);
                deq_count += 1;
            } else {
                break;
            };
        }
    }
    // This adds some additional overhead even with 0 stealers compared to the simple enq/deq benchmark.
    while owner.has_stealers() {}
    let duration = start.elapsed();

    debug_assert!(
        enq_count >= deq_count,
        "enq: {}, deq: {}",
        enq_count,
        deq_count
    );
    *total_enqueues += enq_count;
    *total_dequeues += deq_count;
    duration
}

fn original_bwos_owner_thread(
    producer: &mut original_bwos::Producer<u64>,
    consumer: &mut original_bwos::Consumer<u64>,
    num_enqueues: u64,
    total_enqueues: &mut u64,
    total_dequeues: &mut u64,
) -> Duration {
    let mut enq_count: u64 = 0;
    let mut deq_count: u64 = 0;
    let start = Instant::now();

    while enq_count < num_enqueues {
        while producer.enqueue(black_box(5)) {
            enq_count += 1;
            if enq_count >= num_enqueues {
                break;
            }
        }
        loop {
            if let Some(val) = consumer.dequeue() {
                assert_eq!(black_box(val), 5);
                deq_count += 1;
            } else {
                break;
            };
        }
    }
    // No implementation to check for stealers, so just skip this here.
    //while owner.has_stealers() {}
    let duration = start.elapsed();

    debug_assert!(
        enq_count >= deq_count,
        "enq: {}, deq: {}",
        enq_count,
        deq_count
    );
    *total_enqueues += enq_count;
    *total_dequeues += deq_count;
    duration
}

fn tokio_q_owner_thread(
    owner: &mut original_tokio_queue::Local<u64, 8192>,
    num_enqueues: u64,
    total_enqueues: &mut u64,
    total_dequeues: &mut u64,
) -> Duration {
    let mut enq_count: u64 = 0;
    let mut deq_count: u64 = 0;
    let start = Instant::now();

    while enq_count < num_enqueues {
        while owner.push_back(black_box(5)).is_ok() {
            enq_count += 1;
            if enq_count >= num_enqueues {
                break;
            }
        }
        loop {
            if let Some(val) = owner.pop() {
                assert_eq!(black_box(val), 5_u64);
                deq_count += 1;
            } else {
                break;
            };
        }
    }
    while owner.has_stealers() {}

    let duration = start.elapsed();

    debug_assert!(
        enq_count >= deq_count,
        "enq: {}, deq: {}",
        enq_count,
        deq_count
    );
    *total_enqueues += enq_count;
    *total_dequeues += deq_count;
    duration
}

#[derive(Copy, Clone, Debug)]
enum QueueType {
    BwosStealSingleItems,
    BwosStealBlocks,
    TokioStealSingleItems,
    // Default tokio configuration
    TokioStealHalf,
    BwosUnsafe,
}

enum QueueOwner<const NB: usize, const NE: usize> {
    Bwos(bwosqueue::Owner<u64, NB, NE>),
    Tokio(original_tokio_queue::Local<u64, 8192>),
    BwosUnsafe((original_bwos::Producer<u64>, original_bwos::Consumer<u64>)),
}

fn bench_steal<const NB: usize, const NE: usize>(
    group: &mut BenchmarkGroup<WallTime>,
    queue_type: QueueType,
    num_stealers: usize,
) {
    let setup_params = StealTestParams {
        num_stealers,
        num_ready_stealers: Arc::new(AtomicUsize::new(0)),
        stealer_idle_ops: 5000,
        start: Arc::new(AtomicUsize::new(0)),
        stop: Arc::new(AtomicBool::new(false)),
    };
    let mut test_configuration = match queue_type {
        QueueType::BwosStealBlocks => {
            let (owner, stealer) = bwosqueue::new::<u64, NB, NE>();
            StealTest {
                owner: QueueOwner::Bwos(owner),
                stealer: StealKind::BwosBlockSteal(stealer),
                params: setup_params,
            }
        }
        QueueType::BwosStealSingleItems => {
            let (owner, stealer) = bwosqueue::new::<u64, NB, NE>();
            StealTest {
                owner: QueueOwner::Bwos(owner),
                stealer: StealKind::BwosSingleSteal(stealer),
                params: setup_params,
            }
        }
        QueueType::TokioStealSingleItems => {
            let (stealer, owner) = original_tokio_queue::local();
            StealTest {
                owner: QueueOwner::Tokio(owner),
                stealer: StealKind::TokioSingleSteal(stealer),
                params: setup_params,
            }
        }
        QueueType::TokioStealHalf => {
            let (stealer, owner) = original_tokio_queue::local();
            StealTest {
                owner: QueueOwner::Tokio(owner),
                stealer: StealKind::TokioBatchSteal(stealer),
                params: setup_params,
            }
        }
        QueueType::BwosUnsafe => {
            let (producer, consumer, stealer) = original_bwos::new();
            StealTest {
                owner: QueueOwner::BwosUnsafe((producer, consumer)),
                stealer: StealKind::BwosUnsafeSingleSteal(stealer),
                params: setup_params,
            }
        }
    };

    let enqueue_iterations = 1;
    // One enqueue + One dequeue is the base throughput. This is scaled up by the number of iterations
    // criterion determines and additional enqueue_iterations, since otherwise the time span is too short
    group.throughput(Throughput::Elements(enqueue_iterations * 2));
    let mut total_enqueues: u64 = 0;
    let mut total_dequeues: u64 = 0;
    group.bench_with_input(
        // todo: precalculate expected stealing percentage and make that the parameter!
        BenchmarkId::new(
            format!("{queue_type:?}"),
            format!("{num_stealers} stealers"),
        ),
        &(),
        |b, _| {
            setup_stealers(&test_configuration);
            test_configuration.params.start.store(1, Release);
            fence(SeqCst);
            match &mut test_configuration.owner {
                QueueOwner::Bwos(owner) => {
                    b.iter_custom(|num_iters| {
                        bwos_owner_thread(
                            owner,
                            num_iters * enqueue_iterations,
                            &mut total_enqueues,
                            &mut total_dequeues,
                        )
                    });
                }
                QueueOwner::BwosUnsafe((producer, consumer)) => {
                    b.iter_custom(|num_iters| {
                        original_bwos_owner_thread(
                            producer,
                            consumer,
                            num_iters * enqueue_iterations,
                            &mut total_enqueues,
                            &mut total_dequeues,
                        )
                    });
                }
                QueueOwner::Tokio(owner) => {
                    b.iter_custom(|num_iters| {
                        tokio_q_owner_thread(
                            owner,
                            num_iters * enqueue_iterations,
                            &mut total_enqueues,
                            &mut total_dequeues,
                        )
                    });
                }
            }

            test_configuration.params.stop.store(true, Relaxed);
            test_configuration.params.start.store(2, Release);
        },
    );
    let steal_percentage = if total_enqueues == total_dequeues {
        "0%".to_string()
    } else {
        let p = ((total_enqueues - total_dequeues) as f64 / total_enqueues as f64) * 100.0;
        format!("{:.1}%", p)
    };
    eprintln!("Steal percentage: {steal_percentage}");
}

criterion_group! {name = benches;
config = Criterion::default();
targets = simple_enqueue_dequeue, bench_stealing}
criterion_main!(benches);
