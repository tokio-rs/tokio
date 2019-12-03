//! Asynchronous green-threads.
//!
//! ## What are Tasks?
//!
//! A _task_ is a light weight, non-blocking unit of execution. A task is similar
//! to an OS thread, but rather than being managed by the OS scheduler, they are
//! managed by the [Tokio runtime][rt]. Another name for this general pattern is
//! [green threads]. If you are familiar with [Go's goroutines], [Kotlin's
//! coroutines], or [Erlang's processes], you can think of Tokio's tasks as
//! something similar.
//!
//! Key points about tasks include:
//!
//! * Tasks are **light weight**. Because tasks are scheduled by the Tokio
//!   runtime rather than the operating system, creating new tasks or switching
//!   between tasks does not require a context switch and has fairly low
//!   overhead. Creating, running, and destroying large numbers of tasks is
//!   quite cheap, especially compared to OS threads.
//!
//! * Tasks are scheduled **cooperatively**. Most operating systems implement
//!   _preemptive multitasking_. This is a scheduling technique where the
//!   operating system allows each thread to run for a period of time, and then
//!   _preempts_ it, temporarily pausing that thread and switching to another.
//!   Tasks, on the other hand, implement _cooperative multitasking_. In
//!   cooperative multitasking, a task is allowed to run until it _yields_,
//!   indicating to the Tokio runtime's scheduler that it cannot currently
//!   continue executing. When a task yields, the Tokio runtime switches to
//!   executing the next task.
//!
//! * Tasks are **non-blocking**. Typically, when an OS thread performs I/O or
//!   must synchronize with another thread, it _blocks_, allowing the OS to
//!   schedule another thread. When a task cannot continue executing, it must
//!   yield instead, allowing the Tokio runtime to schedule another task. Tasks
//!   should generally not perform system calls or other operations that could
//!   block a thread, as this would prevent other tasks running on the same
//!   thread from executing as well. Instead, this module provides APIs for
//!   running blocking operations in an asynchronous context.
//!
//! [rt]: crate::runtime
//! [green threads]: https://en.wikipedia.org/wiki/Green_threads
//! [Go's goroutines]: https://tour.golang.org/concurrency/1
//! [Kotlin's coroutines]: https://kotlinlang.org/docs/reference/coroutines-overview.html
//! [Erlang's processes]: http://erlang.org/doc/getting_started/conc_prog.html#processes
//!
//! ## Working with Tasks
//!
//! This module provides the following APIs for working with tasks:
//!
//! ### Spawning
//!
//! Perhaps the most important function in this module is [`task::spawn`]. This
//! function can be thought of as an async equivalent to the standard library's
//! [`thread::spawn`][`std::thread::spawn`]. It takes an `async` block or other
//! [future], and creates a new task to run that work concurrently:
//!
//! ```
//! use tokio::task;
//!
//! # async fn doc() {
//! task::spawn(async {
//!     // perform some work here...
//! });
//! # }
//! ```
//!
//! Like [`std::thread::spawn`], `task::spawn` returns a [`JoinHandle`] struct.
//! A `JoinHandle` is itself a future which may be used to await the output of
//! the spawned task. For example:
//!
//! ```
//! use tokio::task;
//!
//! # #[tokio::main] async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let join = task::spawn(async {
//!     // ...
//!     "hello world!"
//! });
//!
//! // ...
//!
//! // Await the result of the spawned task.
//! let result = join.await?;
//! assert_eq!(result, "hello world!");
//! # Ok(())
//! # }
//! ```
//!
//! Again, like `std::thread`'s [`JoinHandle` type][thread_join], if the spawned
//! task panics, awaiting its `JoinHandle` will return a [`JoinError`]`. For
//! example:
//!
//! ```
//! use tokio::task;
//!
//! # #[tokio::main] async fn main() {
//! let join = task::spawn(async {
//!     panic!("something bad happened!")
//! });
//!
//! // The returned result indicates that the task failed.
//! assert!(join.await.is_err());
//! # }
//! ```
//!
//! `spawn`, `JoinHandle`, and `JoinError` are present when the "rt-core"
//! feature flag is enabled.
//!
//! [`task::spawn`]: crate::task::spawn()
//! [future]: std::future::Future
//! [`std::thread::spawn`]: std::thread::spawn
//! [`JoinHandle`]: crate::task::JoinHandle
//! [thread_join]: std::thread::JoinHandle
//! [`JoinError`]: crate::task::JoinError
//!
//! ### Blocking and Yielding
//!
//! As we discussed above, code running in asynchronous tasks should not perform
//! operations that can block. A blocking operation performed in a task running
//! on a thread that is also running other tasks would block the entire thread,
//! preventing other tasks from running.
//!
//! Instead, Tokio provides two APIs for running blocking operations in an
//! asynchronous context: [`task::spawn_blocking`] and [`task::block_in_place`].
//!
//! The `task::spawn_blocking` function is similar to the `task::spawn` function
//! discussed in the previous section, but rather than spawning an
//! _non-blocking_ future on the Tokio runtime, it instead spawns a
//! _blocking_ function on a dedicated thread pool for blocking tasks. For
//! example:
//!
//! ```
//! use tokio::task;
//!
//! # async fn docs() {
//! task::spawn_blocking(|| {
//!     // do some compute-heavy work or call synchronous code
//! });
//! # }
//! ```
//!
//! Just like `task::spawn`, `task::spawn_blocking` returns a `JoinHandle`
//! which we can use to await the result of the blocking operation:
//!
//! ```rust
//! # use tokio::task;
//! # async fn docs() -> Result<(), Box<dyn std::error::Error>>{
//! let join = task::spawn_blocking(|| {
//!     // do some compute-heavy work or call synchronous code
//!     "blocking completed"
//! });
//!
//! let result = join.await?;
//! assert_eq!(result, "blocking completed");
//! # Ok(())
//! # }
//! ```
//!
//! When using the [threaded runtime][rt-threaded], the [`task::block_in_place`]
//! function is also available. Like `task::spawn_blocking`, this function
//! allows running a blocking operation from an asynchronous context. Unlike
//! `spawn_blocking`, however, `block_in_place` works by transitioning the
//! _current_ worker thread to a blocking thread, moving other tasks running on
//! that thread to another worker thread. This can improve performance by avoiding
//! context switches.
//!
//! For example:
//!
//! ```
//! use tokio::task;
//!
//! # async fn docs() {
//! let result = task::block_in_place(|| {
//!     // do some compute-heavy work or call synchronous code
//!     "blocking completed"
//! });
//!
//! assert_eq!(result, "blocking completed");
//! # }
//! ```
//!
//! In addition, this module also provides a [`task::yield_now`] async function
//! that is analogous to the standard library's [`thread::yield_now`]. Calling and
//! `await`ing this function will cause the current task to yield to the Tokio
//! runtime's scheduler, allowing another task to be scheduled. Eventually, the
//! yielding task will be polled again, allowing it to execute. For example:
//!
//! ```rust
//! use tokio::task;
//!
//! # #[tokio::main] async fn main() {
//! async {
//!     task::spawn(async {
//!         // ...
//!         println!("spawned task done!")
//!     });
//!
//!     // Yield, allowing the newly-spawned task to execute first.
//!     task::yield_now().await;
//!     println!("main task done!");
//! }
//! # .await;
//! # }
//! ```
//!
//! [`task::spawn_blocking`]: crate::task::spawn_blocking
//! [`task::block_in_place`]: crate::task::block_in_place
//! [rt-threaded]: ../runtime/index.html#threaded-scheduler
//! [`task::yield_now`]: crate::task::yield_now()
//! [`thread::yield_now`]: std::thread::yield_now
cfg_blocking! {
    mod blocking;
    pub use blocking::spawn_blocking;

    cfg_rt_threaded! {
        pub use blocking::block_in_place;
    }
}

cfg_rt_core! {
    mod core;
    use self::core::Cell;
    pub(crate) use self::core::Header;

    mod error;
    pub use self::error::JoinError;

    mod harness;
    use self::harness::Harness;

    mod join;
    #[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
    pub use self::join::JoinHandle;

    mod list;
    pub(crate) use self::list::OwnedList;

    mod raw;
    use self::raw::RawTask;

    mod spawn;
    pub use spawn::spawn;

    mod stack;
    pub(crate) use self::stack::TransferStack;

    mod state;
    use self::state::{Snapshot, State};

    mod waker;

    mod yield_now;
    pub use yield_now::yield_now;
}

cfg_rt_util! {
    mod local;
    pub use local::{spawn_local, LocalSet};
}

cfg_rt_core! {
    /// Unit tests
    #[cfg(test)]
    mod tests;

    use std::future::Future;
    use std::marker::PhantomData;
    use std::ptr::NonNull;
    use std::{fmt, mem};

    /// An owned handle to the task, tracked by ref count
    pub(crate) struct Task<S: 'static> {
        raw: RawTask,
        _p: PhantomData<S>,
    }

    unsafe impl<S: ScheduleSendOnly + 'static> Send for Task<S> {}

    /// Task result sent back
    pub(crate) type Result<T> = std::result::Result<T, JoinError>;

    pub(crate) trait Schedule: Sized + 'static {
        /// Bind a task to the executor.
        ///
        /// Guaranteed to be called from the thread that called `poll` on the task.
        fn bind(&self, task: &Task<Self>);

        /// The task has completed work and is ready to be released. The scheduler
        /// is free to drop it whenever.
        fn release(&self, task: Task<Self>);

        /// The has been completed by the executor it was bound to.
        fn release_local(&self, task: &Task<Self>);

        /// Schedule the task
        fn schedule(&self, task: Task<Self>);
    }

    /// Marker trait indicating that a scheduler can only schedule tasks which
    /// implement `Send`.
    ///
    /// Schedulers that implement this trait may not schedule `!Send` futures. If
    /// trait is implemented, the corresponding `Task` type will implement `Send`.
    pub(crate) trait ScheduleSendOnly: Schedule + Send + Sync {}

    /// Create a new task with an associated join handle
    pub(crate) fn joinable<T, S>(task: T) -> (Task<S>, JoinHandle<T::Output>)
    where
        T: Future + Send + 'static,
        S: ScheduleSendOnly,
    {
        let raw = RawTask::new_joinable::<_, S>(task);

        let task = Task {
            raw,
            _p: PhantomData,
        };

        let join = JoinHandle::new(raw);

        (task, join)
    }

    cfg_rt_util! {
        /// Create a new `!Send` task with an associated join handle
        pub(crate) fn joinable_local<T, S>(task: T) -> (Task<S>, JoinHandle<T::Output>)
        where
            T: Future + 'static,
            S: Schedule,
        {
            let raw = RawTask::new_joinable_local::<_, S>(task);

            let task = Task {
                raw,
                _p: PhantomData,
            };

            let join = JoinHandle::new(raw);

            (task, join)
        }
    }

    impl<S: 'static> Task<S> {
        pub(crate) unsafe fn from_raw(ptr: NonNull<Header>) -> Task<S> {
            Task {
                raw: RawTask::from_raw(ptr),
                _p: PhantomData,
            }
        }

        pub(crate) fn header(&self) -> &Header {
            self.raw.header()
        }

        pub(crate) fn into_raw(self) -> NonNull<Header> {
            let raw = self.raw.into_raw();
            mem::forget(self);
            raw
        }
    }

    impl<S: Schedule> Task<S> {
        /// Returns `self` when the task needs to be immediately re-scheduled
        pub(crate) fn run<F>(self, mut executor: F) -> Option<Self>
        where
            F: FnMut() -> Option<NonNull<S>>,
        {
            if unsafe {
                self.raw
                    .poll(&mut || executor().map(|ptr| ptr.cast::<()>()))
            } {
                Some(self)
            } else {
                // Cleaning up the `Task` instance is done from within the poll
                // function.
                mem::forget(self);
                None
            }
        }

        /// Pre-emptively cancel the task as part of the shutdown process.
        pub(crate) fn shutdown(self) {
            self.raw.cancel_from_queue();
            mem::forget(self);
        }
    }

    impl<S: 'static> Drop for Task<S> {
        fn drop(&mut self) {
            self.raw.drop_task();
        }
    }

    impl<S> fmt::Debug for Task<S> {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt.debug_struct("Task").finish()
        }
    }
}
