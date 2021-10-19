use crate::runtime::blocking::NoopSchedule;
use crate::runtime::task::{self, unowned, JoinHandle, OwnedTasks, Schedule, Task};
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

// A Notified does not shut down on drop, but it is dropped once the ref-count
// hits zero.
#[test]
fn create_drop1() {
    let (ad, handle) = AssertDrop::new();
    let (notified, join) = unowned(
        async {
            drop(ad);
            unreachable!()
        },
        NoopSchedule,
    );
    drop(notified);
    handle.assert_not_dropped();
    drop(join);
    handle.assert_dropped();
}

#[test]
fn create_drop2() {
    let (ad, handle) = AssertDrop::new();
    let (notified, join) = unowned(
        async {
            drop(ad);
            unreachable!()
        },
        NoopSchedule,
    );
    drop(join);
    handle.assert_not_dropped();
    drop(notified);
    handle.assert_dropped();
}

// Shutting down through Notified works
#[test]
fn create_shutdown1() {
    let (ad, handle) = AssertDrop::new();
    let (notified, join) = unowned(
        async {
            drop(ad);
            unreachable!()
        },
        NoopSchedule,
    );
    drop(join);
    handle.assert_not_dropped();
    notified.shutdown();
    handle.assert_dropped();
}

#[test]
fn create_shutdown2() {
    let (ad, handle) = AssertDrop::new();
    let (notified, join) = unowned(
        async {
            drop(ad);
            unreachable!()
        },
        NoopSchedule,
    );
    handle.assert_not_dropped();
    notified.shutdown();
    handle.assert_dropped();
    drop(join);
}

#[test]
fn unowned_poll() {
    let (task, _) = unowned(async {}, NoopSchedule);
    task.run();
}

#[test]
fn schedule() {
    with(|rt| {
        rt.spawn(async {
            crate::task::yield_now().await;
        });

        assert_eq!(2, rt.tick());
        rt.shutdown();
    })
}

#[test]
fn shutdown() {
    with(|rt| {
        rt.spawn(async {
            loop {
                crate::task::yield_now().await;
            }
        });

        rt.tick_max(1);

        rt.shutdown();
    })
}

#[test]
fn shutdown_immediately() {
    with(|rt| {
        rt.spawn(async {
            loop {
                crate::task::yield_now().await;
            }
        });

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
            self.0.spawn(async {});
        }
    }

    with(|rt| {
        let rt2 = rt.clone();
        rt.spawn(async move {
            let _spawn_on_drop = SpawnOnDrop(rt2);

            loop {
                crate::task::yield_now().await;
            }
        });

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
    fn spawn<T>(&self, future: T) -> JoinHandle<T::Output>
    where
        T: 'static + Send + Future,
        T::Output: 'static + Send,
    {
        let (handle, notified) = self.0.owned.bind(future, self.clone());

        if let Some(notified) = notified {
            self.schedule(notified);
        }

        handle
    }

    fn tick(&self) -> usize {
        self.tick_max(usize::MAX)
    }

    fn tick_max(&self, max: usize) -> usize {
        let mut n = 0;

        while !self.is_empty() && n < max {
            let task = self.next_task();
            n += 1;
            let task = self.0.owned.assert_owner(task);
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

        self.0.owned.close_and_shutdown_all();

        while let Some(task) = core.queue.pop_back() {
            drop(task);
        }

        drop(core);

        assert!(self.0.owned.is_empty());
    }
}

impl Schedule for Runtime {
    fn release(&self, task: &Task<Self>) -> Option<Task<Self>> {
        self.0.owned.remove(task)
    }

    fn schedule(&self, task: task::Notified<Self>) {
        self.0.core.try_lock().unwrap().queue.push_back(task);
    }
}
