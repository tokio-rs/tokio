use crate::runtime::task::{self, OwnedTasks, Schedule, Task};
use crate::util::TryLock;

use std::collections::VecDeque;
use std::sync::Arc;

#[test]
fn create_drop() {
    let _ = super::joinable::<_, Runtime>(async { unreachable!() });
}

#[test]
fn schedule() {
    with(|rt| {
        let (task, _) = super::joinable(async {
            crate::task::yield_now().await;
        });

        rt.schedule(task);

        assert_eq!(2, rt.tick());
    })
}

#[test]
fn shutdown() {
    with(|rt| {
        let (task, _) = super::joinable(async {
            loop {
                crate::task::yield_now().await;
            }
        });

        rt.schedule(task);
        rt.tick_max(1);

        rt.shutdown();
    })
}

fn with(f: impl FnOnce(Runtime)) {
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            let _rt = CURRENT.try_lock().unwrap().take();
        }
    }

    let _reset = Reset;

    let rt = Runtime(Arc::new(Inner {
        owned: OwnedTasks::new(),
        core: TryLock::new(Core {
            queue: VecDeque::new(),
        }),
    }));

    *CURRENT.try_lock().unwrap() = Some(rt.clone());
    f(rt)
}

#[derive(Clone)]
struct Runtime(Arc<Inner>);

struct Inner {
    core: TryLock<Core>,
    owned: OwnedTasks<Runtime>,
}

struct Core {
    queue: VecDeque<task::Notified<Runtime>>,
}

static CURRENT: TryLock<Option<Runtime>> = TryLock::new(None);

impl Runtime {
    fn tick(&self) -> usize {
        self.tick_max(usize::MAX)
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
        self.0.core.try_lock().unwrap().queue.is_empty()
    }

    fn next_task(&self) -> task::Notified<Runtime> {
        self.0.core.try_lock().unwrap().queue.pop_front().unwrap()
    }

    fn shutdown(&self) {
        let mut core = self.0.core.try_lock().unwrap();

        while let Some(task) = self.0.owned.pop_back() {
            task.shutdown();
        }

        while let Some(task) = core.queue.pop_back() {
            task.shutdown();
        }

        drop(core);

        assert!(self.0.owned.is_empty());
    }
}

impl Schedule for Runtime {
    fn bind(task: Task<Self>) -> Runtime {
        let rt = CURRENT.try_lock().unwrap().as_ref().unwrap().clone();
        rt.0.owned.push_front(task);
        rt
    }

    fn release(&self, task: &Task<Self>) -> Option<Task<Self>> {
        // safety: copying worker.rs
        unsafe { self.0.owned.remove(task) }
    }

    fn schedule(&self, task: task::Notified<Self>) {
        self.0.core.try_lock().unwrap().queue.push_back(task);
    }
}
