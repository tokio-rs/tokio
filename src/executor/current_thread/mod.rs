#![allow(deprecated)]

//! Execute many tasks concurrently on the current thread.
//!
//! [`CurrentThread`] is an executor that keeps tasks on the same thread that
//! they were spawned from. This allows it to execute futures that are not
//! `Send`.
//!
//! A single [`CurrentThread`] instance is able to efficiently manage a large
//! number of tasks and will attempt to schedule all tasks fairly.
//!
//! All tasks that are being managed by a [`CurrentThread`] executor are able to
//! spawn additional tasks by calling [`spawn`]. This function only works from
//! within the context of a running [`CurrentThread`] instance.
//!
//! The easiest way to start a new [`CurrentThread`] executor is to call
//! [`block_on_all`] with an initial task to seed the executor.
//!
//! For example:
//!
//! ```
//! # extern crate tokio;
//! # extern crate futures;
//! # use tokio::executor::current_thread;
//! use futures::future::lazy;
//!
//! // Calling execute here results in a panic
//! // current_thread::spawn(my_future);
//!
//! # pub fn main() {
//! current_thread::block_on_all(lazy(|| {
//!     // The execution context is setup, futures may be executed.
//!     current_thread::spawn(lazy(|| {
//!         println!("called from the current thread executor");
//!         Ok(())
//!     }));
//!
//!     Ok::<_, ()>(())
//! }));
//! # }
//! ```
//!
//! The `block_on_all` function will block the current thread until **all**
//! tasks that have been spawned onto the [`CurrentThread`] instance have
//! completed.
//!
//! More fine-grain control can be achieved by using [`CurrentThread`] directly.
//!
//! ```
//! # extern crate tokio;
//! # extern crate futures;
//! # use tokio::executor::current_thread::CurrentThread;
//! use futures::future::{lazy, empty};
//! use std::time::Duration;
//!
//! // Calling execute here results in a panic
//! // current_thread::spawn(my_future);
//!
//! # pub fn main() {
//! let mut current_thread = CurrentThread::new();
//!
//! // Spawn a task, the task is not executed yet.
//! current_thread.spawn(lazy(|| {
//!     println!("Spawning a task");
//!     Ok(())
//! }));
//!
//! // Spawn a task that never completes
//! current_thread.spawn(empty());
//!
//! // Run the executor, but only until the provided future completes. This
//! // provides the opportunity to start executing previously spawned tasks.
//! let res = current_thread.block_on(lazy(|| {
//!     Ok::<_, ()>("Hello")
//! })).unwrap();
//!
//! // Now, run the executor for *at most* 1 second. Since a task was spawned
//! // that never completes, this function will return with an error.
//! current_thread.run_timeout(Duration::from_secs(1)).unwrap_err();
//! # }
//! ```
//!
//! # Execution model
//!
//! Internally, [`CurrentThread`] maintains a queue. When one of its tasks is
//! notified, the task gets added to the queue. The executor will pop tasks from
//! the queue and call [`Future::poll`]. If the task gets notified while it is
//! being executed, it won't get re-executed until all other tasks currently in
//! the queue get polled.
//!
//! Before the task is polled, a thread-local variable referencing the current
//! [`CurrentThread`] instance is set. This enables [`spawn`] to spawn new tasks
//! onto the same executor without having to thread through a handle value.
//!
//! If the [`CurrentThread`] instance still has uncompleted tasks, but none of
//! these tasks are ready to be polled, the current thread is put to sleep. When
//! a task is notified, the thread is woken up and processing resumes.
//!
//! All tasks managed by [`CurrentThread`] remain on the current thread. When a
//! task completes, it is dropped.
//!
//! [`spawn`]: fn.spawn.html
//! [`block_on_all`]: fn.block_on_all.html
//! [`CurrentThread`]: struct.CurrentThread.html
//! [`Future::poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll

pub use tokio_current_thread::{
    BlockError,
    CurrentThread,
    Entered,
    Handle,
    RunError,
    RunTimeoutError,
    TaskExecutor,
    Turn,
    TurnError,
    block_on_all,
    spawn,
};

use std::cell::Cell;
use std::marker::PhantomData;

use futures::future::{self};

#[deprecated(since = "0.1.2", note = "use block_on_all instead")]
#[doc(hidden)]
#[derive(Debug)]
pub struct Context<'a> {
    cancel: Cell<bool>,
    _p: PhantomData<&'a ()>,
}

impl<'a> Context<'a> {
    /// Cancels *all* executing futures.
    pub fn cancel_all_spawned(&self) {
        self.cancel.set(true);
    }
}

#[deprecated(since = "0.1.2", note = "use block_on_all instead")]
#[doc(hidden)]
pub fn run<F, R>(f: F) -> R
    where F: FnOnce(&mut Context) -> R
{
    let mut context = Context {
        cancel: Cell::new(false),
        _p: PhantomData,
    };

    let mut current_thread = CurrentThread::new();

    let ret = current_thread
        .block_on(future::lazy(|| Ok::<_, ()>(f(&mut context))))
        .unwrap();

    if context.cancel.get() {
        return ret;
    }

    current_thread.run().unwrap();
    ret
}

#[deprecated(since = "0.1.2", note = "use TaskExecutor::current instead")]
#[doc(hidden)]
pub fn task_executor() -> TaskExecutor {
    TaskExecutor::current()
}

