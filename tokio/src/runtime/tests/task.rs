use crate::runtime::task::{self, Schedule, Task};
use crate::util::TryLock;

use std::collections::VecDeque;
use std::sync::Arc;

#[test]
fn create_drop() {
    let _ = task::joinable::<_, Arc<RunQueue>>(async { unreachable!() });
}

#[test]
fn schedule() {
    with(|queue| {
        let (task, _) = task::joinable(async {
            crate::task::yield_now().await;
        });

        queue.schedule(task);

        assert_eq!(2, queue.tick());
    })
}

#[test]
fn shutdown() {
    with(|queue| {
        let (task, _) = task::joinable(async {
            loop {
                crate::task::yield_now().await;
            }
        });

        queue.schedule(task);
    })
}

fn with(f: impl FnOnce(Arc<RunQueue>)) {
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            QUEUE.try_lock().unwrap().take();
        }
    }

    let _r = Reset;

    let queue = Arc::new(RunQueue(TryLock::new(VecDeque::new())));
    *QUEUE.try_lock().unwrap() = Some(queue.clone());
    f(queue)
}

struct RunQueue(TryLock<VecDeque<task::Notified<Arc<RunQueue>>>>);

static QUEUE: TryLock<Option<Arc<RunQueue>>> = TryLock::new(None);

impl RunQueue {
    fn tick(&self) -> usize {
        self.tick_max(usize::max_value())
    }

    fn tick_max(&self, max: usize) -> usize {
        let mut n = 0;

        while !self.is_empty() && n < max {
            let task = self.next_task();
            n += 1;
            task.run();
        }

        n
    }

    fn is_empty(&self) -> bool {
        self.0.try_lock().unwrap().is_empty()
    }

    fn next_task(&self) -> task::Notified<Arc<RunQueue>> {
        self.0.try_lock().unwrap().pop_front().unwrap()
    }
}

impl Schedule for Arc<RunQueue> {
    fn bind(_task: Task<Self>) -> Arc<RunQueue> {
        QUEUE.try_lock().unwrap().as_ref().unwrap().clone()
    }

    fn release(&self, _task: &Task<Self>) -> Option<Task<Self>> {
        None
    }

    fn schedule(&self, task: task::Notified<Self>) {
        self.0.try_lock().unwrap().push_back(task);
    }
}

impl task::ScheduleSendOnly for Arc<RunQueue> {}
