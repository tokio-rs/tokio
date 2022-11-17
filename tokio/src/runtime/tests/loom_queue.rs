use crate::runtime::scheduler::multi_thread::queue;
use crate::runtime::task::Inject;
use crate::runtime::tests::NoopSchedule;
use crate::runtime::MetricsBatch;
use std::sync::Arc;

use crate::runtime::builder::MultiThreadFlavor;
use loom::thread;

fn metrics_batch() -> MetricsBatch {
    MetricsBatch::new(&crate::runtime::WorkerMetrics::new())
}

fn basic_flavor(flavor: MultiThreadFlavor) {
    loom::model(move || {
        let (steal, mut local) = queue::local(flavor);
        let inject = Inject::new();
        let mut metrics = metrics_batch();

        let th = thread::spawn(move || {
            let mut metrics = metrics_batch();
            let (_, mut local) = queue::local(flavor);
            let mut n = 0;

            for _ in 0..3 {
                if steal.steal_into(&mut *local, &mut metrics).is_some() {
                    n += 1;
                }

                while local.pop().is_some() {
                    n += 1;
                }
            }

            n
        });

        let mut n = 0;

        for _ in 0..2 {
            for _ in 0..2 {
                let (task, _) = super::unowned(async {});
                local.push_back_or_overflow(task, &inject, &mut metrics);
            }

            if local.pop().is_some() {
                n += 1;
            }

            // Push another task
            let (task, _) = super::unowned(async {});
            local.push_back_or_overflow(task, &inject, &mut metrics);

            while local.pop().is_some() {
                n += 1;
            }
        }

        while inject.pop().is_some() {
            n += 1;
        }

        n += th.join().unwrap();

        assert_eq!(6, n);
    });
}

#[test]
fn basic_default() {
    basic_flavor(MultiThreadFlavor::Default);
}

#[cfg(all(tokio_unstable, feature = "bwos"))]
#[test]
fn basic_bwos() {
    basic_flavor(MultiThreadFlavor::Bwos);
}

fn steal_overflow(flavor: MultiThreadFlavor) {
    loom::model(move || {
        let (steal, mut local) = queue::local(flavor);
        let inject = Inject::new();
        let mut metrics = metrics_batch();

        let th = thread::spawn(move || {
            let mut metrics = metrics_batch();
            let (_, mut local) = queue::local(flavor);
            let mut n = 0;

            if steal.steal_into(&mut *local, &mut metrics).is_some() {
                n += 1;
            }

            while local.pop().is_some() {
                n += 1;
            }

            n
        });

        let mut n = 0;

        // push a task, pop a task
        let (task, _) = super::unowned(async {});
        local.push_back_or_overflow(task, &inject, &mut metrics);

        if local.pop().is_some() {
            n += 1;
        }

        for _ in 0..6 {
            let (task, _) = super::unowned(async {});
            local.push_back_or_overflow(task, &inject, &mut metrics);
        }

        n += th.join().unwrap();

        while local.pop().is_some() {
            n += 1;
        }

        while inject.pop().is_some() {
            n += 1;
        }

        assert_eq!(7, n);
    });
}

#[test]
fn steal_overflow_default() {
    steal_overflow(MultiThreadFlavor::Default)
}

#[cfg(all(tokio_unstable, feature = "bwos"))]
#[test]
fn steal_overflow_bwos() {
    steal_overflow(MultiThreadFlavor::Bwos)
}

fn multi_stealer_flavor(flavor: MultiThreadFlavor) {
    const NUM_TASKS: usize = 5;

    fn steal_tasks(
        steal: Arc<dyn queue::Stealer<NoopSchedule>>,
        flavor: MultiThreadFlavor,
    ) -> usize {
        let mut metrics = metrics_batch();
        let (_, mut local) = queue::local(flavor);

        if steal.steal_into(&mut *local, &mut metrics).is_none() {
            return 0;
        }

        let mut n = 1;

        while local.pop().is_some() {
            n += 1;
        }

        n
    }

    loom::model(move || {
        let (steal, mut local) = queue::local(flavor);
        let inject = Inject::new();
        let mut metrics = metrics_batch();

        // Push work
        for _ in 0..NUM_TASKS {
            let (task, _) = super::unowned(async {});
            local.push_back_or_overflow(task, &inject, &mut metrics);
        }

        let steal: Arc<dyn queue::Stealer<NoopSchedule> + Send + Sync> = steal.into();

        let th1 = {
            let steal = steal.clone();
            thread::spawn(move || steal_tasks(steal, flavor))
        };

        let th2 = thread::spawn(move || steal_tasks(steal, flavor));

        let mut n = 0;

        while local.pop().is_some() {
            n += 1;
        }

        while inject.pop().is_some() {
            n += 1;
        }

        n += th1.join().unwrap();
        n += th2.join().unwrap();

        assert_eq!(n, NUM_TASKS);
    });
}

#[test]
fn multi_stealer_default() {
    multi_stealer_flavor(MultiThreadFlavor::Default)
}

#[cfg(all(tokio_unstable, feature = "bwos"))]
#[test]
fn multi_stealer_bwos() {
    multi_stealer_flavor(MultiThreadFlavor::Bwos)
}

fn chained_steal(flavor: MultiThreadFlavor) {
    loom::model(move || {
        let mut metrics = metrics_batch();
        let (s1, mut l1) = queue::local(flavor);
        let (s2, mut l2) = queue::local(flavor);
        let inject = Inject::new();

        // Load up some tasks
        for _ in 0..4 {
            let (task, _) = super::unowned(async {});
            l1.push_back_or_overflow(task, &inject, &mut metrics);

            let (task, _) = super::unowned(async {});
            l2.push_back_or_overflow(task, &inject, &mut metrics);
        }

        // Spawn a task to steal from **our** queue
        let th = thread::spawn(move || {
            let mut metrics = metrics_batch();
            let (_, mut local) = queue::local(flavor);
            s1.steal_into(&mut *local, &mut metrics);

            while local.pop().is_some() {}
        });

        // Drain our tasks, then attempt to steal
        while l1.pop().is_some() {}

        s2.steal_into(&mut *l1, &mut metrics);

        th.join().unwrap();

        while l1.pop().is_some() {}
        while l2.pop().is_some() {}
        while inject.pop().is_some() {}
    });
}

#[test]
fn chained_steal_default() {
    chained_steal(MultiThreadFlavor::Default)
}

#[cfg(all(tokio_unstable, feature = "bwos"))]
#[test]
fn chained_steal_bwos() {
    chained_steal(MultiThreadFlavor::Bwos)
}
