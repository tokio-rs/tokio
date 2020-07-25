use crate::runtime::queue;
use crate::runtime::task::{self, Schedule, Task};

use std::thread;
use std::time::Duration;

#[test]
fn fits_256() {
    let (_, mut local) = queue::local();
    let inject = queue::Inject::new();

    for _ in 0..256 {
        let (task, _) = task::joinable::<_, Runtime>(async {});
        local.push_back(task, &inject);
    }

    assert!(inject.pop().is_none());

    while local.pop().is_some() {}
}

#[test]
fn overflow() {
    let (_, mut local) = queue::local();
    let inject = queue::Inject::new();

    for _ in 0..257 {
        let (task, _) = task::joinable::<_, Runtime>(async {});
        local.push_back(task, &inject);
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
    let (steal1, mut local1) = queue::local();
    let (_, mut local2) = queue::local();
    let inject = queue::Inject::new();

    for _ in 0..4 {
        let (task, _) = task::joinable::<_, Runtime>(async {});
        local1.push_back(task, &inject);
    }

    assert!(steal1.steal_into(&mut local2).is_some());

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

    for _ in 0..NUM_ITER {
        let (steal, mut local) = queue::local();
        let inject = queue::Inject::new();

        let th = thread::spawn(move || {
            let (_, mut local) = queue::local();
            let mut n = 0;

            for _ in 0..NUM_STEAL {
                if steal.steal_into(&mut local).is_some() {
                    n += 1;
                }

                while local.pop().is_some() {
                    n += 1;
                }

                thread::yield_now();
            }

            n
        });

        let mut n = 0;

        for _ in 0..NUM_LOCAL {
            for _ in 0..NUM_PUSH {
                let (task, _) = task::joinable::<_, Runtime>(async {});
                local.push_back(task, &inject);
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

    for _ in 0..NUM_ITER {
        let (steal, mut local) = queue::local();
        let inject = queue::Inject::new();

        let th = thread::spawn(move || {
            let (_, mut local) = queue::local();
            let mut n = 0;

            for _ in 0..NUM_STEAL {
                if steal.steal_into(&mut local).is_some() {
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
            let (task, _) = task::joinable::<_, Runtime>(async {});
            local.push_back(task, &inject);

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
    fn bind(task: Task<Self>) -> Runtime {
        std::mem::forget(task);
        Runtime
    }

    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        None
    }

    fn schedule(&self, _task: task::Notified<Self>) {
        unreachable!();
    }
}
