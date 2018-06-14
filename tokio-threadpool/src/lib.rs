//! A work-stealing based thread pool for executing futures.

#![doc(html_root_url = "https://docs.rs/tokio-threadpool/0.1.4")]
#![deny(warnings, missing_docs, missing_debug_implementations)]

// The Tokio thread pool is a thread pool designed to scheduled futures in Tokio
// based applications. The thread pool structure manages two sets of threads:
//
// * Worker threads.
// * Backup threads.
//
// Worker threads are primary threads and are used to schedule futures. These
// threads operate using a work-stealing strategy. Backup threads are to support
// the `blocking` API. Threads will transition between the two sets.
//
// The advantage of the work-stealing strategy is minimal cross thread
// coordination. The thread pool attempts to make as much progress as possible
// without communicating across threads.
//
// # Crate layout
//
// The primary type is `Pool`. This struct contains the majority of the pool
// state. Because worker threads will shutdown and be reestarted, the pool also
// contains the state for each worker. The state dedicated to a worker is
// represented by `worker::Entry`.
//
// `Worker` contains the logic that runs on each worker thread. It holds an
// `Arc` to `Pool` and is able to access its state from `Pool`.
//
// `Task` is a harness around an individual future. It manages polling and
// scheduling that future.
//
// # Worker overview
//
// Each worker has two queues: a deque and a mpsc channel. The deque is the
// primary queue for tasks that are scheduled to run on the worker thread. Tasks
// can only be pushed onto the deque by the worker, but other workers may
// "steal" from that deque. The mpsc channel is used to submit futures while
// external to the pool.
//
// As long as the thread pool has not been shutdown, a worker will run in a
// loop. Each loop, it consumes all tasks on its mpsc channel and pushes it onto
// the deque. It then pops tasks off of the deque and executes them.
//
// If a worker has no work, i.e., both queues are empty. It attempts to steal.
// To do this, it randomly scans other workers' deques and tries to pop a task.
// If it finds no work to steal, the thread goes to sleep.
//
// # Thread pool initialization
//
// By default, no threads are spawned on creation. Instead, when new futures are
// spawned, the pool first checks if there are enough active workeer threads. If
// not, a new worker thread is spawned.
//
// # Spawning futures
//
// The process for spawning a future depends on whether the action is taken from
// a workere thread or external to the thread pool.
//
// When spawning a future while external to the thread pool, the current
// strategy is to randomly pick a worker to submit the task to. The task is then
// pushed onto that worker's mpsc channel.
//
// When spawning a future while on a worker thread, the task is pushed onto the
// back of the current worker's deque.
//
// # Sleeping workers
//
// Sleeping workers are tracked using a treiber stack. This results in the
// thread that most recently went to sleep getting woken up first. When the pool
// is not under load, this helps threads shutdown faster.
//
// Sleeping is done by using `tokio_executor::Park` implementations. This allows
// the user of the thread pool to customize the work that is performed to sleep.
// This is how injecting timers and other functionality into the thread pool is
// done.
//
// # Notifying workers
//
// When there is work to be done, workers must be notified. However, notifying a
// worker requires cross thread coordination. Ideally, a worker would only be
// notified when it is sleeping, but there is no way to know if a worker is
// sleeping without cross thread communication.
//
// The two cases when a worker might need to be notified are:
//
// 1) A task is externally submitted to a worker via the mpsc channel.
// 2) A worker has a back log of work and needs other workers to steal from it.
//
// In the first case, the worker will always be notified. However, it could be
// possible to avoid the notification if the mpsc channel has two or greater
// number of tasks *after* the task is submitted. In this case, we are able to
// assume that the worker has preeviously been notified.
//
// The second case is trickier. Currently, whenever a worker spawns a new future
// (pushing it onto its deque) and when it pops a future from its mpsc, it tries
// to notify a sleeping worker to wake up and start stealing. This is a lot of
// notification and it **might** be possible to reduce it.
//
// Also, whenever a worker is woken up via a signal and it does find work, it,
// in turn, will try to wake up a new worker.
//
// # `blocking`
//
// The strategy for handling blocking closures is to hand off the worker to a
// new thread. This implies handing off the `deque` and `mpsc`. Once this is
// done, the new thread continues to process the work queue and the original
// thread is able to block. Once it finishes processing the blocking future, the
// thread has no additional work and is inserted into the backup pool. This
// makes it available to other workers that encounter a `blocking` call.

extern crate tokio_executor;

extern crate crossbeam_deque as deque;
#[macro_use]
extern crate futures;
extern crate num_cpus;
extern crate rand;

#[macro_use]
extern crate log;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

pub mod park;

mod blocking;
mod builder;
mod callback;
mod config;
#[cfg(feature = "unstable-futures")]
mod futures2_wake;
mod notifier;
mod pool;
mod sender;
mod shutdown;
mod shutdown_task;
mod task;
mod thread_pool;
mod worker;

pub use blocking::{blocking, BlockingError};
pub use builder::Builder;
pub use sender::Sender;
pub use shutdown::Shutdown;
pub use thread_pool::ThreadPool;
pub use worker::Worker;
