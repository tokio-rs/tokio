use crate::runtime::blocking::NoopSchedule;
use crate::runtime::task::{self, joinable, OwnedTasks, Schedule, Task, UnboundTask};
use crate::util::TryLock;

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct AssertDropHandle {
    is_dropped: Arc<AtomicBool>,
}
impl AssertDropHandle {
    #[track_caller]
    fn assert_dropped(&self) {
        assert!(self.is_dropped.load(Ordering::SeqCst));
    }

    #[track_caller]
    fn assert_not_dropped(&self) {
        assert!(!self.is_dropped.load(Ordering::SeqCst));
    }
}

struct AssertDrop {
    is_dropped: Arc<AtomicBool>,
}
impl AssertDrop {
    fn new() -> (Self, AssertDropHandle) {
        let shared = Arc::new(AtomicBool::new(false));
        (
            AssertDrop {
                is_dropped: shared.clone(),
            },
            AssertDropHandle {
                is_dropped: shared.clone(),
            },
        )
    }
}
impl Drop for AssertDrop {
    fn drop(&mut self) {
        self.is_dropped.store(true, Ordering::SeqCst);
    }
}

// An UnboundTask shuts down on drop.
#[test]
fn create_drop1() {
    let (ad, handle) = AssertDrop::new();
    let (task, join) = joinable::<_, Runtime>(async {
        drop(ad);
        unreachable!()
    });
    handle.assert_not_dropped();
    drop(task);
    handle.assert_dropped();
    drop(join);
}

#[test]
fn create_drop2() {
    let (ad, handle) = AssertDrop::new();
    let (task, join) = joinable::<_, Runtime>(async {
        drop(ad);
        unreachable!()
    });
    handle.assert_not_dropped();
    drop(task);
    handle.assert_dropped();
    drop(join);
}

// A Notified does not shut down on drop, but it is dropped once the ref-count
// hits zero.
#[test]
fn into_notified1() {
    let (ad, handle) = AssertDrop::new();
    let (task, join) = joinable(async {
        drop(ad);
        unreachable!()
    });
    let notified = task.into_notified(NoopSchedule);
    drop(notified);
    handle.assert_not_dropped();
    drop(join);
    handle.assert_dropped();
}

#[test]
fn into_notified2() {
    let (ad, handle) = AssertDrop::new();
    let (task, join) = joinable(async {
        drop(ad);
        unreachable!()
    });
    let notified = task.into_notified(NoopSchedule);
    drop(join);
    handle.assert_not_dropped();
    drop(notified);
    handle.assert_dropped();
}

// Shutting down through Notified works
#[test]
fn into_notified_shutdown1() {
    let (ad, handle) = AssertDrop::new();
    let (task, join) = joinable(async {
        drop(ad);
        unreachable!()
    });
    let notified = task.into_notified(NoopSchedule);
    drop(join);
    handle.assert_not_dropped();
    notified.shutdown();
    handle.assert_dropped();
}

#[test]
fn into_notified_shutdown2() {
    let (ad, handle) = AssertDrop::new();
    let (task, join) = joinable(async {
        drop(ad);
        unreachable!()
    });
    let notified = task.into_notified(NoopSchedule);
    handle.assert_not_dropped();
    notified.shutdown();
    handle.assert_dropped();
    drop(join);
}

#[test]
fn schedule() {
    with(|rt| {
        let (task, _) = joinable(async {
            crate::task::yield_now().await;
        });

        rt.spawn(task);

        assert_eq!(2, rt.tick());
        rt.shutdown();
    })
}

#[test]
fn shutdown() {
    with(|rt| {
        let (task, _) = joinable(async {
            loop {
                crate::task::yield_now().await;
            }
        });

        rt.spawn(task);
        rt.tick_max(1);

        rt.shutdown();
    })
}

#[test]
fn shutdown_immediately() {
    with(|rt| {
        let (task, _) = joinable(async {
            loop {
                crate::task::yield_now().await;
            }
        });

        rt.spawn(task);
        rt.shutdown();
    })
}

#[test]
fn spawn_during_shutdown() {
    static DID_SPAWN: AtomicBool = AtomicBool::new(false);

    struct SpawnOnDrop(Runtime);
    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            DID_SPAWN.store(true, Ordering::SeqCst);
            self.0.spawn(joinable(async {}).0);
        }
    }

    with(|rt| {
        let rt2 = rt.clone();
        let (task, _) = joinable(async move {
            let _spawn_on_drop = SpawnOnDrop(rt2);

            loop {
                crate::task::yield_now().await;
            }
        });

        rt.spawn(task);
        rt.tick_max(1);
        rt.shutdown();
    });

    assert!(DID_SPAWN.load(Ordering::SeqCst));
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
    fn spawn<T: 'static + Send + Future>(&self, task: UnboundTask<T, Runtime>) {
        match self.0.owned.bind(task, self.clone()) {
            Ok(notified) => self.schedule(notified),
            Err(task) => drop(task),
        }
    }

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

        self.0.owned.close();
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
    fn release(&self, task: &Task<Self>) -> Option<Task<Self>> {
        // safety: copying worker.rs
        unsafe { self.0.owned.remove(task) }
    }

    fn schedule(&self, task: task::Notified<Self>) {
        self.0.core.try_lock().unwrap().queue.push_back(task);
    }
}
