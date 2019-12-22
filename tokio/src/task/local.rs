//! Runs `!Send` futures on the current thread.
use crate::sync::AtomicWaker;
use crate::task::{self, queue::MpscQueues, JoinHandle, Schedule, Task};

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

cfg_rt_util! {
    /// A set of tasks which are executed on the same thread.
    ///
    /// In some cases, it is necessary to run one or more futures that do not
    /// implement [`Send`] and thus are unsafe to send between threads. In these
    /// cases, a [local task set] may be used to schedule one or more `!Send`
    /// futures to run together on the same thread.
    ///
    /// For example, the following code will not compile:
    ///
    /// ```rust,compile_fail
    /// # use tokio::runtime::Runtime;
    /// use std::rc::Rc;
    ///
    /// // `Rc` does not implement `Send`, and thus may not be sent between
    /// // threads safely.
    /// let unsend_data = Rc::new("my unsend data...");
    ///
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// rt.block_on(async move {
    ///     let unsend_data = unsend_data.clone();
    ///     // Because the `async` block here moves `unsend_data`, the future is `!Send`.
    ///     // Since `tokio::spawn` requires the spawned future to implement `Send`, this
    ///     // will not compile.
    ///     tokio::spawn(async move {
    ///         println!("{}", unsend_data);
    ///         // ...
    ///     }).await.unwrap();
    /// });
    /// ```
    /// In order to spawn `!Send` futures, we can use a local task set to
    /// schedule them on the thread calling [`Runtime::block_on`]. When running
    /// inside of the local task set, we can use [`task::spawn_local`], which can
    /// spawn `!Send` futures. For example:
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// use std::rc::Rc;
    /// use tokio::task;
    ///
    /// let unsend_data = Rc::new("my unsend data...");
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// // Construct a local task set that can run `!Send` futures.
    /// let local = task::LocalSet::new();
    ///
    /// // Run the local task group.
    /// local.block_on(&mut rt, async move {
    ///     let unsend_data = unsend_data.clone();
    ///     // `spawn_local` ensures that the future is spawned on the local
    ///     // task group.
    ///     task::spawn_local(async move {
    ///         println!("{}", unsend_data);
    ///         // ...
    ///     }).await.unwrap();
    /// });
    /// ```
    ///
    /// [`Send`]: https://doc.rust-lang.org/std/marker/trait.Send.html
    /// [local task set]: struct.LocalSet.html
    /// [`Runtime::block_on`]: ../struct.Runtime.html#method.block_on
    /// [`task::spawn_local`]: fn.spawn.html
    #[derive(Debug)]
    pub struct LocalSet {
        scheduler: Rc<Scheduler>,
    }
}

#[derive(Debug)]
struct Scheduler {
    tick: Cell<u8>,

    queues: MpscQueues<Self>,

    /// Used to notify the `LocalFuture` when a task in the local task set is
    /// notified.
    waker: AtomicWaker,
}

pin_project! {
    struct LocalFuture<F> {
        scheduler: Rc<Scheduler>,
        #[pin]
        future: F,
    }
}

thread_local! {
    static CURRENT_TASK_SET: Cell<Option<NonNull<Scheduler>>> = Cell::new(None);
}

cfg_rt_util! {
    /// Spawns a `!Send` future on the local task set.
    ///
    /// The spawned future will be run on the same thread that called `spawn_local.`
    /// This may only be called from the context of a local task set.
    ///
    /// # Panics
    ///
    /// - This function panics if called outside of a local task set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// use std::rc::Rc;
    /// use tokio::task;
    ///
    /// let unsend_data = Rc::new("my unsend data...");
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = task::LocalSet::new();
    ///
    /// // Run the local task set.
    /// local.block_on(&mut rt, async move {
    ///     let unsend_data = unsend_data.clone();
    ///     task::spawn_local(async move {
    ///         println!("{}", unsend_data);
    ///         // ...
    ///     }).await.unwrap();
    /// });
    /// ```
    pub fn spawn_local<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        CURRENT_TASK_SET.with(|current| {
            let current = current
                .get()
                .expect("`spawn_local` called from outside of a task::LocalSet!");
            let (task, handle) = task::joinable_local(future);
            unsafe {
                // safety: this function is unsafe to call outside of the local
                // thread. Since the call above to get the current task set
                // would not succeed if we were outside of a local set, this is
                // safe.
                current.as_ref().queues.push_local(task);
            }

            handle
        })
    }
}

/// Max number of tasks to poll per tick.
const MAX_TASKS_PER_TICK: usize = 61;

impl LocalSet {
    /// Returns a new local task set.
    pub fn new() -> Self {
        Self {
            scheduler: Rc::new(Scheduler::new()),
        }
    }

    /// Spawns a `!Send` task onto the local task set.
    ///
    /// This task is guaranteed to be run on the current thread.
    ///
    /// Unlike the free function [`spawn_local`], this method may be used to
    /// spawn_local local tasks when the task set is _not_ running. For example:
    /// ```rust
    /// # use tokio::runtime::Runtime;
    /// use tokio::task;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = task::LocalSet::new();
    ///
    /// // Spawn a future on the local set. This future will be run when
    /// // we call `block_on` to drive the task set.
    /// local.spawn_local(async {
    ///    // ...
    /// });
    ///
    /// // Run the local task set.
    /// local.block_on(&mut rt, async move {
    ///     // ...
    /// });
    ///
    /// // When `block_on` finishes, we can spawn_local _more_ futures, which will
    /// // run in subsequent calls to `block_on`.
    /// local.spawn_local(async {
    ///    // ...
    /// });
    ///
    /// local.block_on(&mut rt, async move {
    ///     // ...
    /// });
    /// ```
    /// [`spawn_local`]: fn.spawn_local.html
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (task, handle) = task::joinable_local(future);
        unsafe {
            // safety: since `LocalSet` is not Send or Sync, this is
            // always being called from the local thread.
            self.scheduler.queues.push_local(task);
        }
        handle
    }

    /// Run a future to completion on the provided runtime, driving any local
    /// futures spawned on this task set on the current thread.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime. The future
    /// may also call [`spawn_local`] to spawn_local additional local futures on the
    /// current thread.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    ///
    /// # Notes
    ///
    /// Since this function internally calls [`Runtime::block_on`], and drives
    /// futures in the local task set inside that call to `block_on`, the local
    /// futures may not use [in-place blocking]. If a blocking call needs to be
    /// issued from a local task, the [`spawn_blocking`] API may be used instead.
    ///
    /// For example, this will panic:
    /// ```should_panic
    /// use tokio::runtime::Runtime;
    /// use tokio::task;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = task::LocalSet::new();
    /// local.block_on(&mut rt, async {
    ///     let join = task::spawn_local(async {
    ///         let blocking_result = task::block_in_place(|| {
    ///             // ...
    ///         });
    ///         // ...
    ///     });
    ///     join.await.unwrap();
    /// })
    /// ```
    /// This, however, will not panic:
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tokio::task;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let local = task::LocalSet::new();
    /// local.block_on(&mut rt, async {
    ///     let join = task::spawn_local(async {
    ///         let blocking_result = task::spawn_blocking(|| {
    ///             // ...
    ///         }).await;
    ///         // ...
    ///     });
    ///     join.await.unwrap();
    /// })
    /// ```
    ///
    /// [`spawn_local`]: fn.spawn_local.html
    /// [`Runtime::block_on`]: ../struct.Runtime.html#method.block_on
    /// [in-place blocking]: ../blocking/fn.in_place.html
    /// [`spawn_blocking`]: ../blocking/fn.spawn_blocking.html
    pub fn block_on<F>(&self, rt: &mut crate::runtime::Runtime, future: F) -> F::Output
    where
        F: Future,
    {
        let scheduler = self.scheduler.clone();
        self.scheduler
            .with(move || rt.block_on(LocalFuture { scheduler, future }))
    }
}

impl Default for LocalSet {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: Future> Future for LocalFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let scheduler = this.scheduler;
        let mut future = this.future;
        scheduler.waker.register_by_ref(cx.waker());

        if let Poll::Ready(output) = future.as_mut().poll(cx) {
            return Poll::Ready(output);
        }

        if scheduler.tick() {
            // If `tick` returns true, we need to notify the local future again:
            // there are still tasks remaining in the run queue.
            cx.waker().wake_by_ref();
        }

        Poll::Pending
    }
}

// === impl Scheduler ===

impl Schedule for Scheduler {
    fn bind(&self, task: &Task<Self>) {
        assert!(self.is_current());
        unsafe {
            self.queues.add_task(task);
        }
    }

    fn release(&self, task: Task<Self>) {
        // This will be called when dropping the local runtime.
        self.queues.release_remote(task);
    }

    fn release_local(&self, task: &Task<Self>) {
        debug_assert!(self.is_current());
        unsafe {
            self.queues.release_local(task);
        }
    }

    fn schedule(&self, task: Task<Self>) {
        if self.is_current() {
            unsafe { self.queues.push_local(task) };
        } else {
            let mut lock = self.queues.remote();
            lock.schedule(task, false);

            self.waker.wake();

            drop(lock);
        }
    }
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tick: Cell::new(0),
            queues: MpscQueues::new(),
            waker: AtomicWaker::new(),
        }
    }

    fn with<F>(&self, f: impl FnOnce() -> F) -> F {
        struct Entered<'a> {
            current: &'a Cell<Option<NonNull<Scheduler>>>,
        }

        impl<'a> Drop for Entered<'a> {
            fn drop(&mut self) {
                self.current.set(None);
            }
        }

        CURRENT_TASK_SET.with(|current| {
            let prev = current.replace(Some(NonNull::from(self)));
            assert!(prev.is_none(), "nested call to local::Scheduler::with");
            let _entered = Entered { current };
            f()
        })
    }

    fn is_current(&self) -> bool {
        CURRENT_TASK_SET
            .try_with(|current| {
                current
                    .get()
                    .iter()
                    .any(|current| ptr::eq(current.as_ptr(), self as *const _))
            })
            .unwrap_or(false)
    }

    /// Tick the scheduler, returning whether the local future needs to be
    /// notified again.
    fn tick(&self) -> bool {
        assert!(self.is_current());
        for _ in 0..MAX_TASKS_PER_TICK {
            let tick = self.tick.get().wrapping_add(1);
            self.tick.set(tick);

            let task = match unsafe {
                // safety: we must be on the local thread to call this. The assertion
                // the top of this method ensures that `tick` is only called locally.
                self.queues.next_task(tick)
            } {
                Some(task) => task,
                // We have fully drained the queue of notified tasks, so the
                // local future doesn't need to be notified again — it can wait
                // until something else wakes a task in the local set.
                None => return false,
            };

            if let Some(task) = task.run(&mut || Some(self.into())) {
                unsafe {
                    // safety: we must be on the local thread to call this. The
                    // the top of this method ensures that `tick` is only called locally.
                    self.queues.push_local(task);
                }
            }
        }

        true
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        unsafe {
            // safety: these functions are unsafe to call outside of the local
            // thread. Since the `Scheduler` type is not `Send` or `Sync`, we
            // know it will be dropped only from the local thread.
            self.queues.shutdown();

            // Wait until all tasks have been released.
            // XXX: this is a busy loop, but we don't really have any way to park
            // the thread here?
            loop {
                self.queues.drain_pending_drop();
                self.queues.drain_queues();

                if !self.queues.has_tasks_remaining() {
                    break;
                }

                std::thread::yield_now();
            }
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;
    use crate::{
        runtime,
        sync::{mpsc, oneshot},
        task, time,
    };
    use std::time::Duration;

    #[test]
    fn local_current_thread() {
        let mut rt = runtime::Builder::new().basic_scheduler().build().unwrap();
        LocalSet::new().block_on(&mut rt, async {
            spawn_local(async {}).await.unwrap();
        });
    }

    #[test]
    fn local_threadpool() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Runtime::new().unwrap();
        LocalSet::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            spawn_local(async {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            })
            .await
            .unwrap();
        });
    }

    #[test]
    fn local_threadpool_timer() {
        // This test ensures that runtime services like the timer are properly
        // set for the local task set.
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Builder::new()
            .threaded_scheduler()
            .enable_all()
            .build()
            .unwrap();
        LocalSet::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = spawn_local(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                crate::time::delay_for(Duration::from_millis(10)).await;
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        });
    }

    #[test]
    // This will panic, since the thread that calls `block_on` cannot use
    // in-place blocking inside of `block_on`.
    #[should_panic]
    fn local_threadpool_blocking_in_place() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Builder::new()
            .threaded_scheduler()
            .enable_all()
            .build()
            .unwrap();
        LocalSet::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = spawn_local(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                task::block_in_place(|| {});
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        });
    }

    #[test]
    fn local_threadpool_blocking_run() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Builder::new()
            .threaded_scheduler()
            .enable_all()
            .build()
            .unwrap();
        LocalSet::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let join = spawn_local(async move {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                task::spawn_blocking(|| {
                    assert!(
                        !ON_RT_THREAD.with(|cell| cell.get()),
                        "blocking must not run on the local task set's thread"
                    );
                })
                .await
                .unwrap();
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
            });
            join.await.unwrap();
        });
    }

    #[test]
    fn all_spawns_are_local() {
        use futures::future;
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Builder::new()
            .threaded_scheduler()
            .build()
            .unwrap();
        LocalSet::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            let handles = (0..128)
                .map(|_| {
                    spawn_local(async {
                        assert!(ON_RT_THREAD.with(|cell| cell.get()));
                    })
                })
                .collect::<Vec<_>>();
            for joined in future::join_all(handles).await {
                joined.unwrap();
            }
        })
    }

    #[test]
    fn nested_spawn_is_local() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Builder::new()
            .threaded_scheduler()
            .build()
            .unwrap();
        LocalSet::new().block_on(&mut rt, async {
            assert!(ON_RT_THREAD.with(|cell| cell.get()));
            spawn_local(async {
                assert!(ON_RT_THREAD.with(|cell| cell.get()));
                spawn_local(async {
                    assert!(ON_RT_THREAD.with(|cell| cell.get()));
                    spawn_local(async {
                        assert!(ON_RT_THREAD.with(|cell| cell.get()));
                        spawn_local(async {
                            assert!(ON_RT_THREAD.with(|cell| cell.get()));
                        })
                        .await
                        .unwrap();
                    })
                    .await
                    .unwrap();
                })
                .await
                .unwrap();
            })
            .await
            .unwrap();
        })
    }
    #[test]
    fn join_local_future_elsewhere() {
        thread_local! {
            static ON_RT_THREAD: Cell<bool> = Cell::new(false);
        }

        ON_RT_THREAD.with(|cell| cell.set(true));

        let mut rt = runtime::Builder::new()
            .threaded_scheduler()
            .build()
            .unwrap();
        let local = LocalSet::new();
        local.block_on(&mut rt, async move {
            let (tx, rx) = crate::sync::oneshot::channel();
            let join = spawn_local(async move {
                println!("hello world running...");
                assert!(
                    ON_RT_THREAD.with(|cell| cell.get()),
                    "local task must run on local thread, no matter where it is awaited"
                );
                rx.await.unwrap();

                println!("hello world task done");
                "hello world"
            });
            let join2 = task::spawn(async move {
                assert!(
                    !ON_RT_THREAD.with(|cell| cell.get()),
                    "spawned task should be on a worker"
                );

                tx.send(()).expect("task shouldn't have ended yet");
                println!("waking up hello world...");

                join.await.expect("task should complete successfully");

                println!("hello world task joined");
            });
            join2.await.unwrap()
        });
    }
    #[test]
    fn drop_cancels_tasks() {
        // This test reproduces issue #1842
        let mut rt = runtime::Builder::new()
            .enable_time()
            .basic_scheduler()
            .build()
            .unwrap();

        let (started_tx, started_rx) = oneshot::channel();

        let local = LocalSet::new();
        local.spawn_local(async move {
            started_tx.send(()).unwrap();
            loop {
                time::delay_for(Duration::from_secs(3600)).await;
            }
        });

        local.block_on(&mut rt, async {
            started_rx.await.unwrap();
        });
        drop(local);
        drop(rt);
    }

    #[test]
    fn drop_cancels_remote_tasks() {
        // This test reproduces issue #1885.
        use std::sync::mpsc::RecvTimeoutError;

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (tx, mut rx) = crate::sync::mpsc::channel::<()>(1024);

            let mut rt = runtime::Builder::new()
                .enable_time()
                .basic_scheduler()
                .build()
                .expect("building runtime should succeed");

            let local = LocalSet::new();
            local.spawn_local(async move { while let Some(_) = rx.recv().await {} });
            local.block_on(&mut rt, async {
                crate::time::delay_for(Duration::from_millis(1)).await;
            });

            drop(tx);

            // This enters an infinite loop if the remote notified tasks are not
            // properly cancelled.
            drop(local);

            // Send a message on the channel so that the test thread can
            // determine if we have entered an infinite loop:
            done_tx.send(()).unwrap();
        });

        // Since the failure mode of this test is an infinite loop, rather than
        // something we can easily make assertions about, we'll run it in a
        // thread. When the test thread finishes, it will send a message on a
        // channel to this thread. We'll wait for that message with a fairly
        // generous timeout, and if we don't recieve it, we assume the test
        // thread has hung.
        //
        // Note that it should definitely complete in under a minute, but just
        // in case CI is slow, we'll give it a long timeout.
        match done_rx.recv_timeout(Duration::from_secs(60)) {
            Err(RecvTimeoutError::Timeout) => panic!(
                "test did not complete within 60 seconds, \
                 we have (probably) entered an infinite loop!"
            ),
            // Did the test thread panic? We'll find out for sure when we `join`
            // with it.
            Err(RecvTimeoutError::Disconnected) => {
                println!("done_rx dropped, did the test thread panic?");
            }
            // Test completed successfully!
            Ok(()) => {}
        }

        thread.join().expect("test thread should not panic!")
    }

    #[test]
    fn local_tasks_are_polled_after_tick() {
        // Reproduces issues #1899 and #1900
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

        static RX1: AtomicUsize = AtomicUsize::new(0);
        static RX2: AtomicUsize = AtomicUsize::new(0);
        static EXPECTED: usize = 500;

        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut rt = runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()
            .unwrap();

        let local = LocalSet::new();

        local.block_on(&mut rt, async {
            let task2 = task::spawn(async move {
                // Wait a bit
                time::delay_for(Duration::from_millis(100)).await;

                let mut oneshots = Vec::with_capacity(EXPECTED);

                // Send values
                for _ in 0..EXPECTED {
                    let (oneshot_tx, oneshot_rx) = oneshot::channel();
                    oneshots.push(oneshot_tx);
                    tx.send(oneshot_rx).unwrap();
                }

                time::delay_for(Duration::from_millis(100)).await;

                for tx in oneshots.drain(..) {
                    tx.send(()).unwrap();
                }

                time::delay_for(Duration::from_millis(300)).await;
                let rx1 = RX1.load(SeqCst);
                let rx2 = RX2.load(SeqCst);
                println!("EXPECT = {}; RX1 = {}; RX2 = {}", EXPECTED, rx1, rx2);
                assert_eq!(EXPECTED, rx1);
                assert_eq!(EXPECTED, rx2);
            });

            while let Some(oneshot) = rx.recv().await {
                RX1.fetch_add(1, SeqCst);

                task::spawn_local(async move {
                    oneshot.await.unwrap();
                    RX2.fetch_add(1, SeqCst);
                });
            }

            task2.await.unwrap();
        });
    }
}
