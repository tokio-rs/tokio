use crate::runtime::queue;
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

#[test]
fn fits_256() {
    let (_, mut local) = queue::local();
    let inject = Inject::new();
    let mut metrics = MetricsBatch::new();

    for _ in 0..256 {
        let (task, _) = super::unowned(async {});
        local.push_back(task, &inject, &mut metrics);
    }

    cfg_metrics! {
        assert_metrics!(metrics, overflow_count == 0);
    }

    assert!(inject.pop().is_none());

    while local.pop().is_some() {}
}

#[test]
fn overflow() {
    let (_, mut local) = queue::local();
    let inject = Inject::new();
    let mut metrics = MetricsBatch::new();

    for _ in 0..257 {
        let (task, _) = super::unowned(async {});
        local.push_back(task, &inject, &mut metrics);
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
    let mut metrics = MetricsBatch::new();

    let (steal1, mut local1) = queue::local();
    let (_, mut local2) = queue::local();
    let inject = Inject::new();

    for _ in 0..4 {
        let (task, _) = super::unowned(async {});
        local1.push_back(task, &inject, &mut metrics);
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

#[test]
fn stress1() {
    const NUM_ITER: usize = 1;
    const NUM_STEAL: usize = 1_000;
    const NUM_LOCAL: usize = 1_000;
    const NUM_PUSH: usize = 500;
    const NUM_POP: usize = 250;

    let mut metrics = MetricsBatch::new();

    for _ in 0..NUM_ITER {
        let (steal, mut local) = queue::local();
        let inject = Inject::new();

        let th = thread::spawn(move || {
            let mut metrics = MetricsBatch::new();
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
                local.push_back(task, &inject, &mut metrics);
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
    const NUM_TASKS: usize = 1_000_000;
    const NUM_STEAL: usize = 1_000;

    let mut metrics = MetricsBatch::new();

    for _ in 0..NUM_ITER {
        let (steal, mut local) = queue::local();
        let inject = Inject::new();

        let th = thread::spawn(move || {
            let mut stats = MetricsBatch::new();
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
            local.push_back(task, &inject, &mut metrics);

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

#[test]
fn mandatory_blocking_tasks_get_executed() {
    use crate::runtime;
    use std::sync::{Arc, atomic::AtomicBool};

    // We need to execute the test a few times because the failure is
    // non-deterministic. I tried to write a loom that would prove the
    // bug is there (when using spawn_blocking) but failed. The test
    // I wrote didn't fail, even when running >30 minutes.
    for i in 1..1000 {
        let rt = runtime::Builder::new_multi_thread()
            .max_blocking_threads(1)
            .worker_threads(1)
            .build().unwrap();

        let did_blocking_task_execute = Arc::new(
            AtomicBool::new(false)
        );

        {
            let did_blocking_task_execute = did_blocking_task_execute.clone();
            let _enter = rt.enter();

            rt.block_on(async move {
                // We need to have spawned a previous blocking task
                // so that the thread in the blocking pool gets to
                // the state where it's waiting either for a shutdown
                // or another task.
                runtime::spawn_blocking(move || {

                }).await.unwrap();

                // Changing this spawn_mandatory_blocking to
                // spawn_mandatory makes the test fail in a few
                // iterations
                runtime::spawn_mandatory_blocking(move || {
                    did_blocking_task_execute.store(
                        true,
                        std::sync::atomic::Ordering::Release
                    );
                })
            });
            drop(rt);
        }

        assert!(
            did_blocking_task_execute.load(
                std::sync::atomic::Ordering::Acquire
            ),
            "Failed at iteration {:?}", i
        );
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
