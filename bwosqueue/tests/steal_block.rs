extern crate core;

use bwosqueue::{Owner, Stealer};
use core::time;
use core_affinity::CoreId;
use rand::Rng;
use std::arch::asm;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::sync::Arc;
use std::thread;

#[derive(Copy, Clone)]
struct TestParams {
    num_stealers: usize,
    duration: usize,
    idle_loop: usize,
    push_percentage: usize,
    stealer_core: Option<usize>,
    steal_blocks: bool,
}

impl Default for TestParams {
    fn default() -> Self {
        Self {
            num_stealers: 0,
            duration: 1,
            idle_loop: 0,
            push_percentage: 50,
            stealer_core: None,
            steal_blocks: false,
        }
    }
}
fn owner_thread<const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>(
    mut owner: Owner<u64, NUM_BLOCKS, ENTRIES_PER_BLOCK>,
    push_percentage: usize,
    stop_signal: Arc<AtomicBool>,
) -> (usize, usize) {
    let mut counter: usize = 0;
    let mut rng = rand::thread_rng();

    let mut enqueued_count = 0;
    let mut dequeued_count = 0;

    loop {
        if rng.gen_range(1..=100) > push_percentage {
            while let Some(data) = owner.dequeue() {
                assert_eq!(data, 12345);
                dequeued_count += 1;
            }
        } else {
            while owner.enqueue(12345).is_ok() {
                enqueued_count += 1;
            }
        }
        counter = counter.wrapping_add(1);
        if counter % 1000 == 0 && stop_signal.load(Relaxed) {
            break;
        }
    }
    // dequeue until empty
    while let Some(data) = owner.dequeue() {
        assert_eq!(data, 12345);
        dequeued_count += 1;
    }

    (enqueued_count, dequeued_count)
}

fn stealer_thread<const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>(
    stealer: Stealer<u64, NUM_BLOCKS, ENTRIES_PER_BLOCK>,
    stealer_core: Option<CoreId>,
    stop_signal: Arc<AtomicBool>,
    stealer_work_noops: usize,
) -> usize {
    if let Some(core_id) = stealer_core {
        core_affinity::set_for_current(core_id);
    }
    let mut num_stolen = 0;
    let mut counter: usize = 0;
    loop {
        if let Some(data) = stealer.steal() {
            assert!(data > 0);
            num_stolen += 1;
            for _ in 0..stealer_work_noops {
                unsafe { asm!("nop") }
            }
        }
        counter = counter.wrapping_add(1);
        if counter % 1000 == 0 && stop_signal.load(Relaxed) {
            break;
        }
    }
    num_stolen
}

fn steal_block_thread<const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>(
    stealer: Stealer<u64, NUM_BLOCKS, ENTRIES_PER_BLOCK>,
    stealer_core: Option<CoreId>,
    stop_signal: Arc<AtomicBool>,
    stealer_work_noops: usize,
) -> usize {
    if let Some(core_id) = stealer_core {
        core_affinity::set_for_current(core_id);
    }
    let mut num_stolen = 0;
    let mut counter: usize = 0;
    loop {
        if let Some(items) = stealer.steal_block() {
            assert!(!items.is_empty());
            let stolen = items.len();
            num_stolen += items.len();
            for _ in 0..stealer_work_noops {
                unsafe { asm!("nop") }
            }
            // start at one to account for `data` which was not enqueued into the local queue.
            let mut local_dequeues = 0;
            for data in items {
                assert!(data > 0);
                local_dequeues += 1;
            }
            assert_eq!(local_dequeues, stolen);
        }
        counter = counter.wrapping_add(1);
        if counter % 1000 == 0 && stop_signal.load(Relaxed) {
            break;
        }
    }
    num_stolen
}

fn test_queue(params: TestParams) {
    let stop = Arc::new(AtomicBool::new(false));
    let (owner, stealer) = bwosqueue::new::<u64, 8, 32>();

    let producer_stop = stop.clone();
    let owner_handle =
        thread::spawn(move || owner_thread(owner, params.push_percentage, producer_stop));
    let mut stealer_handles = Vec::with_capacity(params.num_stealers);
    for k in 0..params.num_stealers {
        let stealer = stealer.clone();
        let stealer_stop = stop.clone();

        let stealer_core = if let Some(core_id) = params.stealer_core {
            Some(CoreId { id: core_id })
        } else {
            Some(CoreId { id: k + 1 })
        };
        let stealer_handle = thread::spawn(move || {
            if params.steal_blocks {
                steal_block_thread(stealer, stealer_core, stealer_stop, params.idle_loop)
            } else {
                stealer_thread(stealer, stealer_core, stealer_stop, params.idle_loop)
            }
        });
        stealer_handles.push(stealer_handle);
    }

    thread::sleep(time::Duration::from_secs(params.duration as u64));
    println!("Test finished");
    stop.store(true, SeqCst);
    let total_stolen: usize = stealer_handles
        .into_iter()
        .map(|handle| handle.join().expect("Joining stealer failed"))
        .sum();
    println!("Waiting for owner to finish");
    let (total_enqueued, total_dequeued) = owner_handle.join().expect("Failed to join owner");
    assert_eq!(total_enqueued, total_dequeued + total_stolen);
}

#[test]
fn no_stealers_short() {
    let p = TestParams {
        num_stealers: 0,
        duration: 10,
        idle_loop: 0,
        push_percentage: 50,
        stealer_core: None,
        steal_blocks: false,
    };
    test_queue(p);
}

#[test]
fn with_stealers_short() {
    let p = TestParams {
        num_stealers: 2,
        duration: 10,
        idle_loop: 0,
        push_percentage: 50,
        stealer_core: None,
        steal_blocks: false,
    };
    test_queue(p);
}

#[test]
#[ignore]
fn with_stealers_long() {
    let p = TestParams {
        num_stealers: 5,
        duration: 100,
        idle_loop: 500000,
        push_percentage: 70,
        stealer_core: None,
        steal_blocks: true,
    };
    test_queue(p);
}
