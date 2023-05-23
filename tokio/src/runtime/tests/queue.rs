use crate::runtime::scheduler::multi_thread::queue;
use crate::runtime::task::{self, Inject, Schedule, Task};
use crate::runtime::MetricsBatch;

use std::thread;
use std::time::Duration;

#[allow(unused)]
macro_rules! assert_metrics {
    ($metrics:ident, $field:ident == $v:expr) => {{
        use crate::runtime::WorkerMetrics;
        use std::sync::atomic::Ordering::Relaxed;

        let worker = WorkerMetrics::new();
        $metrics.submit(&worker);

        let expect = $v;
        let actual = worker.$field.load(Relaxed);

        assert!(actual == expect, "expect = {}; actual = {}", expect, actual)
    }};
}

fn metrics_batch() -> MetricsBatch {
    use crate::runtime::WorkerMetrics;
    MetricsBatch::new(&WorkerMetrics::new())
}

#[test]
fn fits_256_one_at_a_time() {
    let (_, mut local) = queue::local();
    let inject = Inject::new();
    let mut metrics = metrics_batch();

    for _ in 0..256 {
        let (task, _) = super::unowned(async {});
        local.push_back_or_overflow(task, &inject, &mut metrics);
    }

    cfg_metrics! {
        assert_metrics!(metrics, overflow_count == 0);
    }

    assert!(inject.pop().is_none());

    while local.pop().is_some() {}
}

#[test]
fn fits_256_all_at_once() {
    let (_, mut local) = queue::local();

    let mut tasks = (0..256)
        .map(|_| super::unowned(async {}).0)
        .collect::<Vec<_>>();
    local.push_back(tasks.drain(..));

    let mut i = 0;
    while local.pop().is_some() {
        i += 1;
    }

    assert_eq!(i, 256);
}

#[test]
fn fits_256_all_in_chunks() {
    let (_, mut local) = queue::local();

    let mut tasks = (0..256)
        .map(|_| super::unowned(async {}).0)
        .collect::<Vec<_>>();

    local.push_back(tasks.drain(..10));
    local.push_back(tasks.drain(..100));
    local.push_back(tasks.drain(..46));
    local.push_back(tasks.drain(..100));

    let mut i = 0;
    while local.pop().is_some() {
        i += 1;
    }

    assert_eq!(i, 256);
}

#[test]
fn overflow() {
    let (_, mut local) = queue::local();
    let inject = Inject::new();
    let mut metrics = metrics_batch();

    for _ in 0..257 {
        let (task, _) = super::unowned(async {});
        local.push_back_or_overflow(task, &inject, &mut metrics);
    }

    cfg_metrics! {
        assert_metrics!(metrics, overflow_count == 1);
    }

    let mut n = 0;

    while inject.pop().is_some() {
        n += 1;
    }

    while local.pop().is_some() {
        n += 1;
    }

    assert_eq!(n, 257);
}

#[test]
fn steal_batch() {
    let mut metrics = metrics_batch();

    let (steal1, mut local1) = queue::local();
    let (_, mut local2) = queue::local();
    let inject = Inject::new();

    for _ in 0..4 {
        let (task, _) = super::unowned(async {});
        local1.push_back_or_overflow(task, &inject, &mut metrics);
    }

    assert!(steal1.steal_into(&mut local2, &mut metrics).is_some());

    cfg_metrics! {
        assert_metrics!(metrics, steal_count == 2);
    }

    for _ in 0..1 {
        assert!(local2.pop().is_some());
    }

    assert!(local2.pop().is_none());

    for _ in 0..2 {
        assert!(local1.pop().is_some());
    }

    assert!(local1.pop().is_none());
}

const fn normal_or_miri(normal: usize, miri: usize) -> usize {
    if cfg!(miri) {
        miri
    } else {
        normal
    }
}

#[test]
fn stress1() {
    const NUM_ITER: usize = 5;
    const NUM_STEAL: usize = normal_or_miri(1_000, 10);
    const NUM_LOCAL: usize = normal_or_miri(1_000, 10);
    const NUM_PUSH: usize = normal_or_miri(500, 10);
    const NUM_POP: usize = normal_or_miri(250, 10);

    let mut metrics = metrics_batch();

    for _ in 0..NUM_ITER {
        let (steal, mut local) = queue::local();
        let inject = Inject::new();

        let th = thread::spawn(move || {
            let mut metrics = metrics_batch();
            let (_, mut local) = queue::local();
            let mut n = 0;

            for _ in 0..NUM_STEAL {
                if steal.steal_into(&mut local, &mut metrics).is_some() {
                    n += 1;
                }

                while local.pop().is_some() {
                    n += 1;
                }

                thread::yield_now();
            }

            cfg_metrics! {
                assert_metrics!(metrics, steal_count == n as _);
            }

            n
        });

        let mut n = 0;

        for _ in 0..NUM_LOCAL {
            for _ in 0..NUM_PUSH {
                let (task, _) = super::unowned(async {});
                local.push_back_or_overflow(task, &inject, &mut metrics);
            }

            for _ in 0..NUM_POP {
                if local.pop().is_some() {
                    n += 1;
                } else {
                    break;
                }
            }
        }

        while inject.pop().is_some() {
            n += 1;
        }

        n += th.join().unwrap();

        assert_eq!(n, NUM_LOCAL * NUM_PUSH);
    }
}

#[test]
fn stress2() {
    const NUM_ITER: usize = 1;
    const NUM_TASKS: usize = normal_or_miri(1_000_000, 50);
    const NUM_STEAL: usize = normal_or_miri(1_000, 10);

    let mut metrics = metrics_batch();

    for _ in 0..NUM_ITER {
        let (steal, mut local) = queue::local();
        let inject = Inject::new();

        let th = thread::spawn(move || {
            let mut stats = metrics_batch();
            let (_, mut local) = queue::local();
            let mut n = 0;

            for _ in 0..NUM_STEAL {
                if steal.steal_into(&mut local, &mut stats).is_some() {
                    n += 1;
                }

                while local.pop().is_some() {
                    n += 1;
                }

                thread::sleep(Duration::from_micros(10));
            }

            n
        });

        let mut num_pop = 0;

        for i in 0..NUM_TASKS {
            let (task, _) = super::unowned(async {});
            local.push_back_or_overflow(task, &inject, &mut metrics);

            if i % 128 == 0 && local.pop().is_some() {
                num_pop += 1;
            }

            while inject.pop().is_some() {
                num_pop += 1;
            }
        }

        num_pop += th.join().unwrap();

        while local.pop().is_some() {
            num_pop += 1;
        }

        while inject.pop().is_some() {
            num_pop += 1;
        }

        assert_eq!(num_pop, NUM_TASKS);
    }
}

struct Runtime;

impl Schedule for Runtime {
    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}
