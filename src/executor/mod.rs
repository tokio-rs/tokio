//! Task execution utilities.
//!
//! In the Tokio execution model, futures are lazy. When a future is created, no
//! work is performed. In order for the work defined by the future to happen,
//! the future must be submitted to an executor. A future that is submitted to
//! an executor is called a "task".
//!
//! The executor executor is responsible for ensuring that [`Future::poll`] is
//! called whenever the task is [notified]. Notification happens when the
//! internal state of a task transitions from "not ready" to ready. For
//! example, a socket might have received data and a call to `read` will now be
//! able to succeed.
//!
//! The specific strategy used to manage the tasks is left up to the
//! executor. There are two main flavors of executors: single-threaded and
//! multithreaded. This module provides both.
//!
//! * **[`current_thread`]**: A single-threaded executor that support spawning
//! tasks that are not `Send`. It guarantees that tasks will be executed on
//! the same thread from which they are spawned.
//!
//! * **[`thread_pool`]**: A multi-threaded executor that maintains a pool of
//! threads. Tasks are spawned to one of the threads in the pool and executed.
//! The pool employes a [work-stealing] strategy for optimizing how tasks get
//! spread across the available threads.
//!
//! This module also provides the [`Executor`] trait (re-exported from
//! [`tokio-executor`]), which describes the API that all executors must
//! implement as well as a [`spawn`] function, which spawns a future onto the
//! current default executor for the context (tracked via a thread-local variable).
//!
//! [`Future::poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll
//! [notified]: https://docs.rs/futures/0.1/futures/executor/trait.Notify.html#tymethod.notify
//! [`current_thread`]: current_thread/index.html
//! [`thread_pool`]: thread_pool/index.html
//! [work-stealing]: https://en.wikipedia.org/wiki/Work_stealing
//! [`tokio-executor`]: #
//! [`Executor`]: #
//! [`spawn`]: #


pub mod current_thread;

pub mod thread_pool {
    //! Maintains a pool of threads across which the set of spawned tasks are
    //! executed.

    pub use tokio_threadpool::{
        Builder,
        Sender,
        Shutdown,
        ThreadPool,
    };
}

pub use tokio_executor::{Executor, DefaultExecutor, SpawnError, spawn};
