//! Thread pool for blocking operations

use crate::loom::sync::{Arc, Condvar, Mutex};
use crate::loom::thread;
use crate::runtime::{self, io, time, Builder, Callback};
use crate::runtime::blocking::shutdown;
use crate::runtime::blocking::schedule::NoopSchedule;
use crate::runtime::blocking::task::BlockingTask;
use crate::task::{self, JoinHandle};

use std::cell::Cell;
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

pub(crate) struct BlockingPool {
    spawner: Spawner,
    shutdown_rx: shutdown::Receiver,
}

#[derive(Clone)]
pub(crate) struct Spawner {
    inner: Arc<Inner>,
}

struct Inner {
    /// State shared between worker threads
    shared: Mutex<Shared>,

    /// Pool threads wait on this.
    condvar: Condvar,

    /// Spawned threads use this name
    thread_name: String,

    /// Spawned thread stack size
    stack_size: Option<usize>,

    /// Call after a thread starts
    after_start: Option<Callback>,

    /// Call before a thread stops
    before_stop: Option<Callback>,

    /// Spawns async tasks
    spawner: runtime::Spawner,

    /// Runtime I/O driver handle
    io_handle: io::Handle,

    /// Runtime time driver handle
    time_handle: time::Handle,

    /// Source of `Instant::now()`
    clock: time::Clock,

    thread_cap: usize,

}

struct Shared {
    queue: VecDeque<Task>,
    num_th: usize,
    num_idle: u32,
    num_notify: u32,
    shutdown: bool,
    shutdown_tx: Option<shutdown::Sender>,
}

type Task = task::Task<NoopSchedule>;

thread_local! {
    /// Thread-local tracking the current executor
    static BLOCKING: Cell<Option<*const Spawner>> = Cell::new(None)
}

const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Run the provided function on an executor dedicated to blocking operations.
pub(crate) fn spawn_blocking<F, R>(func: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
{
    BLOCKING.with(|cell| {
        let schedule = match cell.get() {
            Some(ptr) => unsafe { &*ptr },
            None => panic!("not currently running on the Tokio runtime."),
        };

        let (task, handle) = task::joinable(BlockingTask::new(func));
        schedule.schedule(task);
        handle
    })
}

// ===== impl BlockingPool =====

impl BlockingPool {
    pub(crate) fn new(
        builder: &Builder,
        spawner: &runtime::Spawner,
        io: &io::Handle,
        time: &time::Handle,
        clock: &time::Clock,
        thread_cap: usize,
    ) -> BlockingPool {
        let (shutdown_tx, shutdown_rx) = shutdown::channel();

        BlockingPool {
            spawner: Spawner {
                inner: Arc::new(Inner {
                    shared: Mutex::new(Shared {
                        queue: VecDeque::new(),
                        num_th: 0,
                        num_idle: 0,
                        num_notify: 0,
                        shutdown: false,
                        shutdown_tx: Some(shutdown_tx),
                    }),
                    condvar: Condvar::new(),
                    thread_name: builder.thread_name.clone(),
                    stack_size: builder.thread_stack_size,
                    after_start: builder.after_start.clone(),
                    before_stop: builder.before_stop.clone(),
                    spawner: spawner.clone(),
                    io_handle: io.clone(),
                    time_handle: time.clone(),
                    clock: clock.clone(),
                    thread_cap,
                }),
            },
            shutdown_rx,
        }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }
}

impl Drop for BlockingPool {
    fn drop(&mut self) {
        let mut shared = self.spawner.inner.shared.lock().unwrap();

        shared.shutdown = true;
        shared.shutdown_tx = None;
        self.spawner.inner.condvar.notify_all();

        drop(shared);

        self.shutdown_rx.wait();
    }
}

impl fmt::Debug for BlockingPool {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BlockingPool").finish()
    }
}

// ===== impl Spawner =====

impl Spawner {
    /// Set the blocking pool for the duration of the closure
    ///
    /// If a blocking pool is already set, it will be restored when the closure
    /// returns or if it panics.
    pub(crate) fn enter<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // While scary, this is safe. The function takes a `&BlockingPool`,
        // which guarantees that the reference lives for the duration of
        // `with_pool`.
        //
        // Because we are always clearing the TLS value at the end of the
        // function, we can cast the reference to 'static which thread-local
        // cells require.
        BLOCKING.with(|cell| {
            let was = cell.replace(None);

            // Ensure that the pool is removed from the thread-local context
            // when leaving the scope. This handles cases that involve panicking.
            struct Reset<'a>(&'a Cell<Option<*const Spawner>>, Option<*const Spawner>);

            impl Drop for Reset<'_> {
                fn drop(&mut self) {
                    self.0.set(self.1);
                }
            }

            let _reset = Reset(cell, was);
            cell.set(Some(self as *const Spawner));
            f()
        })
    }

    fn schedule(&self, task: Task) {
        let shutdown_tx = {
            let mut shared = self.inner.shared.lock().unwrap();

            if shared.shutdown {
                // Shutdown the task
                task.shutdown();

                // no need to even push this task; it would never get picked up
                return;
            }

            shared.queue.push_back(task);

            if shared.num_idle == 0 {
                // No threads are able to process the task.

                if shared.num_th == self.inner.thread_cap {
                    // At max number of threads
                    None
                } else {
                    shared.num_th += 1;
                    assert!(shared.shutdown_tx.is_some());
                    shared.shutdown_tx.clone()
                }
            } else {
                // Notify an idle worker thread. The notification counter
                // is used to count the needed amount of notifications
                // exactly. Thread libraries may generate spurious
                // wakeups, this counter is used to keep us in a
                // consistent state.
                shared.num_idle -= 1;
                shared.num_notify += 1;
                self.inner.condvar.notify_one();
                None
            }
        };

        if let Some(shutdown_tx) = shutdown_tx {
            self.spawn_thread(shutdown_tx);
        }
    }

    fn spawn_thread(&self, shutdown_tx: shutdown::Sender) {
        let mut builder = thread::Builder::new().name(self.inner.thread_name.clone());

        if let Some(stack_size) = self.inner.stack_size {
            builder = builder.stack_size(stack_size);
        }

        let inner = self.inner.clone();

        builder
            .spawn(move || {
                inner.run();

                // Make sure `inner` drops first to ensure that the shutdown_rx
                // sees all refs to `Inner` are dropped when the `shutdown_rx`
                // resolves.
                drop(inner);
                drop(shutdown_tx);
            })
            .unwrap();
    }
}

impl Inner {
    fn run(&self) {
        let _io = io::set_default(&self.io_handle);

        time::with_default(&self.time_handle, &self.clock, || {
            self.spawner.enter(|| self.run2());
        });
    }

    fn run2(&self) {
        if let Some(f) = &self.after_start {
            f()
        }

        let mut shared = self.shared.lock().unwrap();

        'main: loop {
            // BUSY
            while let Some(task) = shared.queue.pop_front() {
                drop(shared);
                run_task(task);

                shared = self.shared.lock().unwrap();
                if shared.shutdown {
                    break; // Need to increment idle before we exit
                }
            }

            // IDLE
            shared.num_idle += 1;

            while !shared.shutdown {
                let lock_result = self.condvar.wait_timeout(shared, KEEP_ALIVE).unwrap();

                shared = lock_result.0;
                let timeout_result = lock_result.1;

                if shared.num_notify != 0 {
                    // We have received a legitimate wakeup,
                    // acknowledge it by decrementing the counter
                    // and transition to the BUSY state.
                    shared.num_notify -= 1;
                    break;
                }

                // Even if the condvar "timed out", if the pool is entering the
                // shutdown phase, we want to perform the cleanup logic.
                if !shared.shutdown && timeout_result.timed_out() {
                    break 'main;
                }

                // Spurious wakeup detected, go back to sleep.
            }

            if shared.shutdown {
                // Drain the queue
                while let Some(task) = shared.queue.pop_front() {
                    drop(shared);
                    task.shutdown();

                    shared = self.shared.lock().unwrap();
                }

                // Work was produced, and we "took" it (by decrementing num_notify).
                // This means that num_idle was decremented once for our wakeup.
                // But, since we are exiting, we need to "undo" that, as we'll stay idle.
                shared.num_idle += 1;
                // NOTE: Technically we should also do num_notify++ and notify again,
                // but since we're shutting down anyway, that won't be necessary.
                break;
            }
        }

        // Thread exit
        shared.num_th -= 1;

        // num_idle should now be tracked exactly, panic
        // with a descriptive message if it is not the
        // case.
        shared.num_idle = shared
            .num_idle
            .checked_sub(1)
            .expect("num_idle underflowed on thread exit");

        if shared.shutdown && shared.num_th == 0 {
            self.condvar.notify_one();
        }

        drop(shared);

        if let Some(f) = &self.before_stop {
            f()
        }
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("blocking::Spawner").finish()
    }
}

fn run_task(f: Task) {
    let scheduler: &'static NoopSchedule = &NoopSchedule;
    let res = f.run(|| Some(scheduler.into()));
    assert!(res.is_none());
}
