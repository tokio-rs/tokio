use crate::runtime::queue;
use crate::runtime::task::{self, Schedule, Task};

use loom::thread;

#[test]
fn multi_stealer() {
    const NUM_TASKS: usize = 5;

    fn steal_tasks(steal: queue::Steal<Runtime>) -> usize {
        let (_, mut local) = queue::local();

        if steal.steal_into(&mut local).is_none() {
            return 0;
        }

        let mut n = 1;

        while local.pop().is_some() {
            n += 1;
        }

        n
    }

    loom::model(|| {
        let (steal, mut local) = queue::local();
        let inject = queue::Inject::new();

        // Push work
        for _ in 0..NUM_TASKS {
            let (task, _) = task::joinable::<_, Runtime>(async {});
            local.push_back(task, &inject);
        }

        let th1 = {
            let steal = steal.clone();
            thread::spawn(move || steal_tasks(steal))
        };

        let th2 = thread::spawn(move || steal_tasks(steal));

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
