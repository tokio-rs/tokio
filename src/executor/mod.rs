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
//! The pool employs a [work-stealing] strategy for optimizing how tasks get
//! spread across the available threads.
//!
//! # `Executor` trait.
//!
//! This module provides the [`Executor`] trait (re-exported from
//! [`tokio-executor`]), which describes the API that all executors must
//! implement.
//!
//! A free [`spawn`] function is provided that allows spawning futures onto the
//! default executor (tracked via a thread-local variable) without referencing a
//! handle. It is expected that all executors will set a value for the default
//! executor. This value will often be set to the executor itself, but it is
//! possible that the default executor might be set to a different executor.
//!
//! For example, the [`current_thread`] executor might set the default executor
//! to a thread pool instead of itself, allowing futures to spawn new tasks onto
//! the thread pool when those tasks are `Send`.
//!
//! [`Future::poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll
//! [notified]: https://docs.rs/futures/0.1/futures/executor/trait.Notify.html#tymethod.notify
//! [`current_thread`]: current_thread/index.html
//! [`thread_pool`]: thread_pool/index.html
//! [work-stealing]: https://en.wikipedia.org/wiki/Work_stealing
//! [`tokio-executor`]: #
//! [`Executor`]: #
//! [`spawn`]: #

pub mod current_thread {
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
        TaskExecutor,
        CurrentThread,
        RunError,
        Entered,
        spawn,
        block_on_all,
    };
}

pub mod thread_pool {
    //! Maintains a pool of threads across which the set of spawned tasks are
    //! executed.
    //!
    //! [`ThreadPool`] is an executor that uses a thread pool for executing
    //! tasks concurrently across multiple cores. It uses a thread pool that is
    //! optimized for use cases that involve multiplexing large number of
    //! independent tasks that perform short(ish) amounts of computation and are
    //! mainly waiting on I/O, i.e. the Tokio use case.
    //!
    //! Usually, users of [`ThreadPool`] will not create pool instances.
    //! Instead, they will create a [`Runtime`] instance, which comes with a
    //! pre-configured thread pool.
    //!
    //! At the core, [`ThreadPool`] uses a work-stealing based scheduling
    //! strategy. When spawning a task while *external* to the thread pool
    //! (i.e., from a thread that is not part of the thread pool), the task is
    //! randomly assigned to a worker thread. When spawning a task while
    //! *internal* to the thread pool, the task is assigned to the current
    //! worker.
    //!
    //! Each worker maintains its own queue and first focuses on processing all
    //! tasks in its queue. When the worker's queue is empty, the worker will
    //! attempt to *steal* tasks from other worker queues. This strategy helps
    //! ensure that work is evenly distributed across threads while minimizing
    //! synchronization between worker threads.
    //!
    //! # Usage
    //!
    //! Thread pool instances are created using [`ThreadPool::new`] or
    //! [`Builder::new`]. The first option returns a thread pool with default
    //! configuration values. The second option allows configuring the thread
    //! pool before instantiating it.
    //!
    //! Once an instance is obtained, futures may be spawned onto it using the
    //! [`spawn`] function.
    //!
    //! A handle to the thread pool is obtained using [`ThreadPool::sender`].
    //! This handle is **only** able to spawn futures onto the thread pool. It
    //! is unable to affect the lifecycle of the thread pool in any way. This
    //! handle can be passed into functions or stored in structs as a way to
    //! grant the capability of spawning futures.
    //!
    //! # Examples
    //!
    //! ```rust
    //! # extern crate tokio;
    //! # extern crate futures;
    //! # use tokio::executor::thread_pool::ThreadPool;
    //! use futures::future::{Future, lazy};
    //!
    //! # pub fn main() {
    //! // Create a thread pool with default configuration values
    //! let thread_pool = ThreadPool::new();
    //!
    //! thread_pool.spawn(lazy(|| {
    //!     println!("called from a worker thread");
    //!     Ok(())
    //! }));
    //!
    //! // Gracefully shutdown the threadpool
    //! thread_pool.shutdown().wait().unwrap();
    //! # }
    //! ```
    //!
    //! [`ThreadPool`]: struct.ThreadPool.html
    //! [`ThreadPool::new`]: struct.ThreadPool.html#method.new
    //! [`ThreadPool::sender`]: struct.ThreadPool.html#method.sender
    //! [`spawn`]: struct.ThreadPool.html#method.spawn
    //! [`Builder::new`]: struct.Builder.html#method.new
    //! [`Runtime`]: ../../runtime/struct.Runtime.html

    pub use tokio_threadpool::{
        Builder,
        Sender,
        Shutdown,
        ThreadPool,
    };
}

pub use tokio_executor::{Executor, DefaultExecutor, SpawnError};

use futures::{Future, IntoFuture};
use futures::future::{self, FutureResult};

#[cfg(feature = "unstable-futures")]
use futures2;

/// Return value from the `spawn` function.
///
/// Currently this value doesn't actually provide any functionality. However, it
/// provides a way to add functionality later without breaking backwards
/// compatibility.
///
/// This also implements `IntoFuture` so that it can be used as the return value
/// in a `for_each` loop.
///
/// See [`spawn`] for more details.
///
/// [`spawn`]: fn.spawn.html
#[derive(Debug)]
pub struct Spawn(());

/// Spawns a future on the default executor.
///
/// In order for a future to do work, it must be spawned on an executor. The
/// `spawn` function is the easiest way to do this. It spawns a future on the
/// [default executor] for the current execution context (tracked using a
/// thread-local variable).
///
/// The default executor is **usually** a thread pool.
///
/// # Examples
///
/// In this example, a server is started and `spawn` is used to start a new task
/// that processes each received connection.
///
/// ```rust
/// # extern crate tokio;
/// # extern crate futures;
/// # use futures::{Future, Stream};
/// use tokio::net::TcpListener;
///
/// # fn process<T>(_: T) -> Box<Future<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio::spawn(process(socket))
///     });
///
/// tokio::run(server);
/// # }
/// # pub fn main() {}
/// ```
///
/// [default executor]: struct.DefaultExecutor.html
///
/// # Panics
///
/// This function will panic if the default executor is not set or if spawning
/// onto the default executor returns an error. To avoid the panic, use
/// [`DefaultExecutor`].
///
/// [`DefaultExecutor`]: struct.DefaultExecutor.html
pub fn spawn<F>(f: F) -> Spawn
where F: Future<Item = (), Error = ()> + 'static + Send
{
    ::tokio_executor::spawn(f);
    Spawn(())
}

/// Like `spawn`, but compatible with futures 0.2
#[cfg(feature = "unstable-futures")]
pub fn spawn2<F>(f: F) -> Spawn
    where F: futures2::Future<Item = (), Error = futures2::Never> + 'static + Send
{
    ::tokio_executor::spawn2(f);
    Spawn(())
}

impl IntoFuture for Spawn {
    type Future = FutureResult<(), ()>;
    type Item = ();
    type Error = ();

    fn into_future(self) -> Self::Future {
        future::ok(())
    }
}

#[cfg(feature = "unstable-futures")]
impl futures2::IntoFuture for Spawn {
    type Future = futures2::future::FutureResult<(), ()>;
    type Item = ();
    type Error = ();

    fn into_future(self) -> Self::Future {
        futures2::future::ok(())
    }
}
