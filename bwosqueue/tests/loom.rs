use std::sync::atomic::Ordering::{Acquire, Relaxed, Release, SeqCst};
use std::sync::atomic::{fence, AtomicBool, AtomicU32, Ordering};
use tracing::{event, span, Level};

#[cfg(loom)]
use loom::{
    self, model,
    thread::{self, JoinHandle},
};
#[cfg(not(loom))]
use std::thread::{self, JoinHandle};

use bwosqueue::Owner;

#[cfg(not(loom))]
fn model<F>(f: F)
where
    F: FnOnce(),
{
    f();
}

type QueueOwner = bwosqueue::Owner<u64, 2, 2>;
type QueueStealer = bwosqueue::Stealer<u64, 2, 2>;

struct Stat {
    sum: u64,
    buf: u64,
}

impl Stat {
    fn new() -> Self {
        Self { sum: 0, buf: 1 }
    }
    fn put(&mut self, owner: &mut QueueOwner) {
        if owner.enqueue(self.buf).is_ok() {
            event!(Level::INFO, "put succeeded");
            self.sum += self.buf;
            self.buf <<= 1;
        } else {
            event!(Level::INFO, "put failed");
        }
    }

    fn get(&mut self, owner: &mut QueueOwner) -> bool {
        if let Some(data) = owner.dequeue() {
            event!(Level::INFO, "get succeeded");
            self.sum += data;
            true
        } else {
            event!(Level::INFO, "get failed");
            false
        }
    }

    fn steal(&mut self, stealer: &QueueStealer) {
        event!(Level::INFO, "attempting to steal");
        if let Some(data) = stealer.steal() {
            event!(Level::INFO, "steal succeeded");
            self.sum += data;
        } else {
            event!(Level::INFO, "steal failed");
        }
    }
}

fn thread0(
    mut q_owner: QueueOwner,
    mut enq_stat: Stat,
    mut deq_stat: Stat,
) -> (QueueOwner, Stat, Stat) {
    let owner = &mut q_owner;

    let span = span!(Level::INFO, "Owner Put A");
    let guard = span.enter();
    for i in 0..3 {
        event!(Level::INFO, put_iter = i);
        enq_stat.put(owner);
    }
    drop(guard);

    let span = span!(Level::INFO, "Owner Get B");
    let guard = span.enter();
    for i in 0..2 {
        event!(Level::INFO, get_iter = i);
        deq_stat.get(owner);
    }
    drop(guard);

    let span = span!(Level::INFO, "Owner Put C");
    let guard = span.enter();
    for i in 0..4 {
        event!(Level::INFO, put_iter = i);
        enq_stat.put(owner);
    }
    drop(guard);

    let span = span!(Level::INFO, "Owner Get D");
    let guard = span.enter();
    for i in 0..3 {
        event!(Level::INFO, get_iter = i);
        deq_stat.get(owner);
    }
    drop(guard);

    let span = span!(Level::INFO, "Owner Put E");
    let guard = span.enter();
    for i in 0..3 {
        event!(Level::INFO, put_iter = i);
        enq_stat.put(owner);
    }
    drop(guard);

    for _ in 0..4 {
        deq_stat.get(owner);
    }

    (q_owner, enq_stat, deq_stat)
}

fn thread1(stealer: QueueStealer, mut s1: Stat) -> Stat {
    let span = span!(Level::INFO, "Stealer 1");
    let _guard = span.enter();
    s1.steal(&stealer);
    event!(Level::INFO, "Steal A done");
    s1
}

fn thread2(stealer: QueueStealer, mut s2: Stat) -> Stat {
    let span = span!(Level::INFO, "Stealer 2");
    let _guard = span.enter();
    s2.steal(&stealer);
    event!(Level::INFO, "Steal B done");
    s2.steal(&stealer);
    event!(Level::INFO, "Steal C done");
    s2
}

fn test_inner(stealers: usize) {
    assert!(stealers <= 2, "We only have 2 stealers implemented");
    let explored_executions = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let l_explored_executions = explored_executions.clone();
    println!();
    model(move || {
        let current_iteration = l_explored_executions.fetch_add(1, Ordering::Relaxed);
        let (owner, s1): (QueueOwner, QueueStealer) = bwosqueue::new();
        let enq_stat = Stat::new();
        let deq_stat = Stat::new();
        let s1_stat = Stat::new();
        let s2_stat = Stat::new();

        let owner_handle = thread::spawn(move || thread0(owner, enq_stat, deq_stat));

        let mut stealer_handles = Vec::with_capacity(stealers);
        if stealers > 0 {
            if stealers > 1 {
                let s2 = s1.clone();
                stealer_handles.push(thread::spawn(move || thread2(s2, s2_stat)));
            }
            stealer_handles.push(thread::spawn(move || thread1(s1, s1_stat)));
        }

        let (mut owner, enq_stat, mut deq_stat) =
            owner_handle.join().expect("Owner thread panicked");
        let total_stolen: u64 = stealer_handles
            .into_iter()
            .map(|handle| handle.join().expect("Stealer thread panicked").sum)
            .sum();
        while deq_stat.get(&mut owner) {}
        assert_eq!(enq_stat.sum, deq_stat.sum + total_stolen);

        if current_iteration > 0 && current_iteration % 50_000 == 0 {
            println!("Explored {current_iteration} iterations");
        }
    });
    println!(
        "Loom model explored {} interleavings.",
        explored_executions.load(SeqCst)
    );
}

#[test]
fn no_stealer() {
    test_inner(0);
}

#[test]
fn one_stealer() {
    test_inner(1);
}

// This test will take a very long time with loom, so ignore it unless specifically requested
#[test]
#[cfg_attr(loom, ignore)]
fn two_stealers() {
    test_inner(2);
}

#[test]
fn steal_block_loom() {
    model(|| {
        const NB: usize = 4;
        const NE: usize = 4;
        let (mut owner, stealer) = bwosqueue::new::<u64, NB, NE>();
        // explicitly not a loom type, since this only for the test and we do not care about reorderings
        let total_dequeues: std::sync::Arc<AtomicU32> = std::sync::Arc::new(AtomicU32::new(0));

        let mut total_enq = 0;
        while owner.enqueue(5).is_ok() {
            total_enq += 1;
        }

        let mut handles: [Option<JoinHandle<Owner<u64, NB, NE>>>; NB - 1] =
            array_init::array_init(|_| None);
        for th_handle in handles.iter_mut() {
            let local_stealer = stealer.clone();
            let local_total_dequeues = total_dequeues.clone();
            let handle = thread::spawn(move || {
                let (mut dst_owner, _) = bwosqueue::new::<u64, NB, NE>();
                // Any ordering of stealers and consumer is possible, so maybe the consumer consumed everything
                // already and there is nothing left to steal.
                // Stealing could fail sporadically due to steal_block nature, but we can't do much about that.
                if let Some(stolen_iter) = local_stealer.steal_block() {
                    assert!(
                        !stolen_iter.is_empty(),
                        "Successfull steal implies at least one stolen item"
                    );
                    unsafe { dst_owner.enqueue_batch_unchecked(Box::new(stolen_iter)) };

                    while let Some(val) = dst_owner.dequeue() {
                        assert_eq!(val, 5);
                        local_total_dequeues.fetch_add(1, Relaxed);
                    }
                }
                fence(SeqCst);
                dst_owner
            });
            *th_handle = Some(handle);
        }
        while let Some(val) = owner.dequeue() {
            assert_eq!(val, 5);
            total_dequeues.fetch_add(1, Relaxed);
        }

        for handle in handles {
            let mut dst_queue = handle
                .expect("Handle not initialized")
                .join()
                .expect("Join failed");
            #[cfg(feature = "stats")]
            assert!(!dst_queue.can_consume());
            assert!(dst_queue.dequeue().is_none());
            assert!(dst_queue.dequeue_block().is_none());
        }

        std::sync::atomic::fence(SeqCst);
        assert_eq!(total_dequeues.load(SeqCst), total_enq);

        #[cfg(feature = "stats")]
        assert!(!owner.can_consume());
    });
}

#[test]
fn queue_loom() {
    model(|| {
        const NB: usize = 4;
        const NE: usize = 8;
        const ITERATIONS: u32 = 80;
        let (mut owner, stealer) = bwosqueue::new::<u64, NB, NE>();
        // explicitly not `loom` types, since this only for the test and we do not care about reorderings
        let total_dequeues = std::sync::Arc::new(AtomicU32::new(0));
        let finished = std::sync::Arc::new(AtomicBool::new(false));

        let owner_total_deq = total_dequeues.clone();
        let owner_finished = finished.clone();
        let owner_thread_handle = thread::spawn(move || {
            let mut total_enq: u32 = 0;
            while total_enq < ITERATIONS {
                while total_enq < ITERATIONS && owner.enqueue(5).is_ok() {
                    total_enq += 1;
                }

                while let Some(res) = owner.dequeue() {
                    assert_eq!(res, 5);
                    owner_total_deq.fetch_add(1, Relaxed);
                }
            }
            while let Some(val) = owner.dequeue() {
                assert_eq!(val, 5);
                owner_total_deq.fetch_add(1, Relaxed);
            }

            #[cfg(feature = "stats")]
            assert!(!owner.can_consume());
            owner_finished.store(true, Release);
            total_enq
        });

        let mut handles: [Option<JoinHandle<_>>; 2] = array_init::array_init(|_| None);
        for th_handle in &mut handles {
            let local_stealer = stealer.clone();
            let local_total_dequeues = total_dequeues.clone();
            let local_finished = finished.clone();

            let handle = thread::spawn(move || {
                let (mut dst_owner, _) = bwosqueue::new::<u64, NB, NE>();

                // Any ordering of stealers and consumer is possible, so maybe the consumer consumed everything
                // already and there is nothing left to steal.
                // Stealing could fail sporadically due to steal_block nature, but we can't do much about that.
                while !local_finished.load(Acquire) {
                    if let Some(stolen_iter) = local_stealer.steal_block() {
                        assert!(
                            !stolen_iter.is_empty(),
                            "Successful steal implies at least one stolen item"
                        );
                        unsafe { dst_owner.enqueue_batch_unchecked(Box::new(stolen_iter)) };

                        while let Some(val) = dst_owner.dequeue() {
                            assert_eq!(val, 5);
                            local_total_dequeues.fetch_add(1, Relaxed);
                        }
                    }
                }
                fence(Release);
                dst_owner
            });
            *th_handle = Some(handle);
        }

        for handle in handles {
            let mut dst_queue = handle
                .expect("Handle not initialized")
                .join()
                .expect("Join failed");
            #[cfg(feature = "stats")]
            assert!(!dst_queue.can_consume());
            assert!(dst_queue.dequeue().is_none());
            assert!(dst_queue.dequeue_block().is_none());
        }

        let total_enqueued = owner_thread_handle.join().expect("Owner thread failed");
        std::sync::atomic::fence(SeqCst);
        assert_eq!(total_dequeues.load(SeqCst), total_enqueued);
    });
}
