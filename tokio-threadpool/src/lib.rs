//! A work-stealing based thread pool for executing futures.

#![doc(html_root_url = "https://docs.rs/tokio-threadpool/0.1.1")]
#![deny(warnings, missing_docs, missing_debug_implementations)]

extern crate tokio_executor;
extern crate futures;
extern crate crossbeam_deque as deque;
extern crate num_cpus;
extern crate rand;

#[macro_use]
extern crate log;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

mod task;

use tokio_executor::{Enter, SpawnError};

use task::Task;

use futures::{future, Future, Poll, Async};
use futures::executor::Notify;
use futures::task::AtomicTask;

use rand::{Rng, SeedableRng, XorShiftRng};

use std::{fmt, mem, thread, usize};
use std::cell::{Cell, UnsafeCell};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Weak, Mutex, Condvar};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release, Relaxed};
use std::time::{Instant, Duration};

#[derive(Debug)]
struct ShutdownTask {
    task1: AtomicTask,

    #[cfg(feature = "unstable-futures")]
    task2: futures2::task::AtomicWaker,
}

/// Work-stealing based thread pool for executing futures.
///
/// If a `ThreadPool` instance is dropped without explicitly being shutdown,
/// `shutdown_now` is called implicitly, forcing all tasks that have not yet
/// completed to be dropped.
///
/// Create `ThreadPool` instances using `Builder`.
#[derive(Debug)]
pub struct ThreadPool {
    inner: Option<Sender>,
}

/// Submit futures to the associated thread pool for execution.
///
/// A `Sender` instance is a handle to a single thread pool, allowing the owner
/// of the handle to spawn futures onto the thread pool. New futures are spawned
/// using [`Sender::spawn`].
///
/// The `Sender` handle is *only* used for spawning new futures. It does not
/// impact the lifecycle of the thread pool in any way.
///
/// `Sender` instances are obtained by calling [`ThreadPool::sender`]. The
/// `Sender` struct implements the `Executor` trait.
///
/// [`Sender::spawn`]: #method.spawn
/// [`ThreadPool::sender`]: struct.ThreadPool.html#method.sender
#[derive(Debug)]
pub struct Sender {
    inner: Arc<Inner>,
}

/// Future that resolves when the thread pool is shutdown.
///
/// A `ThreadPool` is shutdown once all the worker have drained their queues and
/// shutdown their threads.
///
/// `Shutdown` is returned by [`shutdown`], [`shutdown_on_idle`], and
/// [`shutdown_now`].
///
/// [`shutdown`]: struct.ThreadPool.html#method.shutdown
/// [`shutdown_on_idle`]: struct.ThreadPool.html#method.shutdown_on_idle
/// [`shutdown_now`]: struct.ThreadPool.html#method.shutdown_now
#[derive(Debug)]
pub struct Shutdown {
    inner: Sender,
}

/// Builds a thread pool with custom configuration values.
///
/// Methods can be chanined in order to set the configuration values. The thread
/// pool is constructed by calling [`build`].
///
/// New instances of `Builder` are obtained via [`Builder::new`].
///
/// See function level documentation for details on the various configuration
/// settings.
///
/// [`build`]: #method.build
/// [`Builder::new`]: #method.new
///
/// # Examples
///
/// ```
/// # extern crate tokio_threadpool;
/// # extern crate futures;
/// # use tokio_threadpool::Builder;
/// use futures::future::{Future, lazy};
/// use std::time::Duration;
///
/// # pub fn main() {
/// // Create a thread pool with default configuration values
/// let thread_pool = Builder::new()
///     .pool_size(4)
///     .keep_alive(Some(Duration::from_secs(30)))
///     .build();
///
/// thread_pool.spawn(lazy(|| {
///     println!("called from a worker thread");
///     Ok(())
/// }));
///
/// // Gracefully shutdown the threadpool
/// thread_pool.shutdown().wait().unwrap();
/// # }
/// ```
#[derive(Debug)]
pub struct Builder {
    /// Thread pool specific configuration values
    config: Config,

    /// Number of workers to spawn
    pool_size: usize,
}

/// Thread pool specific configuration values
#[derive(Debug, Clone)]
struct Config {
    keep_alive: Option<Duration>,
    // Used to configure a worker thread
    name_prefix: Option<String>,
    stack_size: Option<usize>,
    around_worker: Option<Callback>,
}

#[derive(Debug)]
struct Inner {
    // ThreadPool state
    state: AtomicUsize,

    // Stack tracking sleeping workers.
    sleep_stack: AtomicUsize,

    // Number of workers who haven't reached the final state of shutdown
    //
    // This is only used to know when to single `shutdown_task` once the
    // shutdown process has completed.
    num_workers: AtomicUsize,

    // Used to generate a thread local RNG seed
    next_thread_id: AtomicUsize,

    // Storage for workers
    //
    // This will *usually* be a small number
    workers: Box<[WorkerEntry]>,

    // Task notified when the worker shuts down
    shutdown_task: ShutdownTask,

    // Configuration
    config: Config,
}

#[derive(Clone)]
struct Callback {
    f: Arc<Fn(&Worker, &mut Enter) + Send + Sync>,
}

/// Implements the future `Notify` API.
///
/// This is how external events are able to signal the task, informing it to try
/// to poll the future again.
#[derive(Debug)]
struct Notifier {
    inner: Weak<Inner>,
}

#[cfg(feature = "unstable-futures")]
struct Futures2Wake {
    notifier: Arc<Notifier>,
    id: usize,
}

/// ThreadPool state.
///
/// The two least significant bits are the shutdown flags.  (0 for active, 1 for
/// shutdown on idle, 2 for shutting down). The remaining bits represent the
/// number of futures that still need to complete.
#[derive(Eq, PartialEq, Clone, Copy)]
struct State(usize);

/// Flag used to track if the pool is running
const SHUTDOWN_ON_IDLE: usize = 1;
const SHUTDOWN_NOW: usize = 2;

/// Mask used to extract the number of futures from the state
const LIFECYCLE_MASK: usize = 0b11;
const NUM_FUTURES_MASK: usize = !LIFECYCLE_MASK;
const NUM_FUTURES_OFFSET: usize = 2;

/// Max number of futures the pool can handle.
const MAX_FUTURES: usize = usize::MAX >> NUM_FUTURES_OFFSET;

/// State related to the stack of sleeping workers.
///
/// - Parked head     16 bits
/// - Sequence        remaining
///
/// The parked head value has a couple of special values:
///
/// - EMPTY: No sleepers
/// - TERMINATED: Don't spawn more threads
#[derive(Eq, PartialEq, Clone, Copy)]
struct SleepStack(usize);

/// Extracts the head of the worker stack from the scheduler state
const STACK_MASK: usize = ((1 << 16) - 1);

/// Max number of workers that can be part of a pool. This is the most that can
/// fit in the scheduler state. Note, that this is the max number of **active**
/// threads. There can be more standby threads.
const MAX_WORKERS: usize = 1 << 15;

/// Used to mark the stack as empty
const EMPTY: usize = MAX_WORKERS;

/// Used to mark the stack as terminated
const TERMINATED: usize = EMPTY + 1;

/// How many bits the treiber ABA guard is offset by
const ABA_GUARD_SHIFT: usize = 16;

#[cfg(target_pointer_width = "64")]
const ABA_GUARD_MASK: usize = (1 << (64 - ABA_GUARD_SHIFT)) - 1;

#[cfg(target_pointer_width = "32")]
const ABA_GUARD_MASK: usize = (1 << (32 - ABA_GUARD_SHIFT)) - 1;

// Some constants used to work with State
// const A: usize: 0;

// TODO: This should be split up between what is accessed by each thread and
// what is concurrent. The bits accessed by each thread should be sized to
// exactly one cache line.
#[derive(Debug)]
struct WorkerEntry {
    // Worker state. This is mutated when notifying the worker.
    state: AtomicUsize,

    // Next entry in the parked Trieber stack
    next_sleeper: UnsafeCell<usize>,

    // Worker half of deque
    deque: deque::Deque<Task>,

    // Stealer half of deque
    steal: deque::Stealer<Task>,

    // Park mutex
    park_mutex: Mutex<()>,

    // Park condvar
    park_condvar: Condvar,

    // MPSC queue of jobs submitted to the worker from an external source.
    inbound: task::Queue,
}

/// Tracks worker state
#[derive(Clone, Copy, Eq, PartialEq)]
struct WorkerState(usize);

/// Set when the worker is pushed onto the scheduler's stack of sleeping
/// threads.
const PUSHED_MASK: usize = 0b001;

/// Manages the worker lifecycle part of the state
const WORKER_LIFECYCLE_MASK: usize = 0b1110;
const WORKER_LIFECYCLE_SHIFT: usize = 1;

/// The worker does not currently have an associated thread.
const WORKER_SHUTDOWN: usize = 0;

/// The worker is currently processing its task.
const WORKER_RUNNING: usize = 1;

/// The worker is currently asleep in the condvar
const WORKER_SLEEPING: usize = 2;

/// The worker has been notified it should process more work.
const WORKER_NOTIFIED: usize = 3;

/// A stronger form of notification. In this case, the worker is expected to
/// wakeup and try to acquire more work... if it enters this state while already
/// busy with other work, it is expected to signal another worker.
const WORKER_SIGNALED: usize = 4;

/// Thread worker
///
/// This is passed to the `around_worker` callback set on `Builder`. This
/// callback is only expected to call `run` on it.
#[derive(Debug)]
pub struct Worker {
    // Shared scheduler data
    inner: Arc<Inner>,

    // WorkerEntry index
    idx: usize,

    // Set when the worker should finalize on drop
    should_finalize: Cell<bool>,

    // Keep the value on the current thread.
    _p: PhantomData<Rc<()>>,
}

// Pointer to the current worker info
thread_local!(static CURRENT_WORKER: Cell<*const Worker> = Cell::new(0 as *const _));

// ===== impl Builder =====

impl Builder {
    /// Returns a new thread pool builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    /// use std::time::Duration;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .pool_size(4)
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn new() -> Builder {
        let num_cpus = num_cpus::get();

        Builder {
            pool_size: num_cpus,
            config: Config {
                keep_alive: None,
                name_prefix: None,
                stack_size: None,
                around_worker: None,
            },
        }
    }

    /// Set the maximum number of worker threads for the thread pool instance.
    ///
    /// This must be a number between 1 and 32,768 though it is advised to keep
    /// this value on the smaller side.
    ///
    /// The default value is the number of cores available to the system.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .pool_size(4)
    ///     .build();
    /// # }
    /// ```
    pub fn pool_size(&mut self, val: usize) -> &mut Self {
        assert!(val >= 1, "at least one thread required");
        assert!(val <= MAX_WORKERS, "max value is {}", 32768);

        self.pool_size = val;
        self
    }

    /// Set the worker thread keep alive duration
    ///
    /// If set, a worker thread will wait for up to the specified duration for
    /// work, at which point the thread will shutdown. When work becomes
    /// available, a new thread will eventually be spawned to replace the one
    /// that shut down.
    ///
    /// When the value is `None`, the thread will wait for work forever.
    ///
    /// The default value is `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    /// use std::time::Duration;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .keep_alive(Some(Duration::from_secs(30)))
    ///     .build();
    /// # }
    /// ```
    pub fn keep_alive(&mut self, val: Option<Duration>) -> &mut Self {
        self.config.keep_alive = val;
        self
    }

    /// Set name prefix of threads spawned by the scheduler
    ///
    /// Thread name prefix is used for generating thread names. For example, if
    /// prefix is `my-pool-`, then threads in the pool will get names like
    /// `my-pool-1` etc.
    ///
    /// If this configuration is not set, then the thread will use the system
    /// default naming scheme.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .name_prefix("my-pool-")
    ///     .build();
    /// # }
    /// ```
    pub fn name_prefix<S: Into<String>>(&mut self, val: S) -> &mut Self {
        self.config.name_prefix = Some(val.into());
        self
    }

    /// Set the stack size (in bytes) for worker threads.
    ///
    /// The actual stack size may be greater than this value if the platform
    /// specifies minimal stack size.
    ///
    /// The default stack size for spawned threads is 2 MiB, though this
    /// particular stack size is subject to change in the future.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .stack_size(32 * 1024)
    ///     .build();
    /// # }
    /// ```
    pub fn stack_size(&mut self, val: usize) -> &mut Self {
        self.config.stack_size = Some(val);
        self
    }

    /// Execute function `f` on each worker thread.
    ///
    /// This function is provided a handle to the worker and is expected to call
    /// `Worker::run`, otherwise the worker thread will shutdown without doing
    /// any work.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .around_worker(|worker, _| {
    ///         println!("worker is starting up");
    ///         worker.run();
    ///         println!("worker is shutting down");
    ///     })
    ///     .build();
    /// # }
    /// ```
    pub fn around_worker<F>(&mut self, f: F) -> &mut Self
        where F: Fn(&Worker, &mut Enter) + Send + Sync + 'static
    {
        self.config.around_worker = Some(Callback::new(f));
        self
    }

    /// Create the configured `ThreadPool`.
    ///
    /// The returned `ThreadPool` instance is ready to spawn tasks.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::Builder;
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = Builder::new()
    ///     .build();
    /// # }
    /// ```
    pub fn build(&self) -> ThreadPool {
        let mut workers = vec![];

        trace!("build; num-workers={}", self.pool_size);

        for _ in 0..self.pool_size {
            workers.push(WorkerEntry::new());
        }

        let inner = Arc::new(Inner {
            state: AtomicUsize::new(State::new().into()),
            sleep_stack: AtomicUsize::new(SleepStack::new().into()),
            num_workers: AtomicUsize::new(self.pool_size),
            next_thread_id: AtomicUsize::new(0),
            workers: workers.into_boxed_slice(),
            shutdown_task: ShutdownTask {
                task1: AtomicTask::new(),
                #[cfg(feature = "unstable-futures")]
                task2: futures2::task::AtomicWaker::new(),
            },
            config: self.config.clone(),
        });

        // Now, we prime the sleeper stack
        for i in 0..self.pool_size {
            inner.push_sleeper(i).unwrap();
        }

        let inner = Some(Sender { inner });

        ThreadPool { inner }
    }
}

// ===== impl ThreadPool =====

impl ThreadPool {
    /// Create a new `ThreadPool` with default values.
    ///
    /// Use [`Builder`] for creating a configured thread pool.
    ///
    /// [`Builder`]: struct.Builder.html
    pub fn new() -> ThreadPool {
        Builder::new().build()
    }

    /// Spawn a future onto the thread pool.
    ///
    /// This function takes ownership of the future and randomly assigns it to a
    /// worker thread. The thread will then start executing the future.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::ThreadPool;
    /// use futures::future::{Future, lazy};
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = ThreadPool::new();
    ///
    /// thread_pool.spawn(lazy(|| {
    ///     println!("called from a worker thread");
    ///     Ok(())
    /// }));
    ///
    /// // Gracefully shutdown the threadpool
    /// thread_pool.shutdown().wait().unwrap();
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Use [`Sender::spawn`] for a
    /// version that returns a `Result` instead of panicking.
    pub fn spawn<F>(&self, future: F)
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.sender().spawn(future).unwrap();
    }

    /// Return a reference to the sender handle
    ///
    /// The handle is used to spawn futures onto the thread pool. It also
    /// implements the `Executor` trait.
    pub fn sender(&self) -> &Sender {
        self.inner.as_ref().unwrap()
    }

    /// Return a mutable reference to the sender handle
    pub fn sender_mut(&mut self) -> &mut Sender {
        self.inner.as_mut().unwrap()
    }

    /// Shutdown the pool once it becomes idle.
    ///
    /// Idle is defined as the completion of all futures that have been spawned
    /// onto the thread pool. There may still be outstanding handles when the
    /// thread pool reaches an idle state.
    ///
    /// Once the idle state is reached, calling `spawn` on any outstanding
    /// handle will result in an error. All worker threads are signaled and will
    /// shutdown. The returned future completes once all worker threads have
    /// completed the shutdown process.
    pub fn shutdown_on_idle(mut self) -> Shutdown {
        self.inner().shutdown(false, false);
        Shutdown { inner: self.inner.take().unwrap() }
    }

    /// Shutdown the pool
    ///
    /// This prevents the thread pool from accepting new tasks but will allow
    /// any existing tasks to complete.
    ///
    /// Calling `spawn` on any outstanding handle will result in an error. All
    /// worker threads are signaled and will shutdown. The returned future
    /// completes once all worker threads have completed the shutdown process.
    pub fn shutdown(mut self) -> Shutdown {
        self.inner().shutdown(true, false);
        Shutdown { inner: self.inner.take().unwrap() }
    }

    /// Shutdown the pool immediately
    ///
    /// This will prevent the thread pool from accepting new tasks **and**
    /// abort any tasks that are currently running on the thread pool.
    ///
    /// Calling `spawn` on any outstanding handle will result in an error. All
    /// worker threads are signaled and will shutdown. The returned future
    /// completes once all worker threads have completed the shutdown process.
    pub fn shutdown_now(mut self) -> Shutdown {
        self.inner().shutdown(true, true);
        Shutdown { inner: self.inner.take().unwrap() }
    }

    fn inner(&self) -> &Inner {
        &*self.inner.as_ref().unwrap().inner
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        if let Some(sender) = self.inner.take() {
            sender.inner.shutdown(true, true);
            let shutdown = Shutdown { inner: sender };
            let _ = shutdown.wait();
        }
    }
}

// ===== impl Sender ======

impl Sender {
    /// Spawn a future onto the thread pool
    ///
    /// This function takes ownership of the future and spawns it onto the
    /// thread pool, assigning it to a worker thread. The exact strategy used to
    /// assign a future to a worker depends on if the caller is already on a
    /// worker thread or external to the thread pool.
    ///
    /// If the caller is currently on the thread pool, the spawned future will
    /// be assigned to the same worker that the caller is on. If the caller is
    /// external to the thread pool, the future will be assigned to a random
    /// worker.
    ///
    /// If `spawn` returns `Ok`, this does not mean that the future will be
    /// executed. The thread pool can be forcibly shutdown between the time
    /// `spawn` is called and the future has a chance to execute.
    ///
    /// If `spawn` returns `Err`, then the future failed to be spawned. There
    /// are two possible causes:
    ///
    /// * The thread pool is at capacity and is unable to spawn a new future.
    ///   This is a temporary failure. At some point in the future, the thread
    ///   pool might be able to spawn new futures.
    /// * The thread pool is shutdown. This is a permanent failure indicating
    ///   that the handle will never be able to spawn new futures.
    ///
    /// The status of the thread pool can be queried before calling `spawn`
    /// using the `status` function (part of the `Executor` trait).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate tokio_threadpool;
    /// # extern crate futures;
    /// # use tokio_threadpool::ThreadPool;
    /// use futures::future::{Future, lazy};
    ///
    /// # pub fn main() {
    /// // Create a thread pool with default configuration values
    /// let thread_pool = ThreadPool::new();
    ///
    /// thread_pool.sender().spawn(lazy(|| {
    ///     println!("called from a worker thread");
    ///     Ok(())
    /// })).unwrap();
    ///
    /// // Gracefully shutdown the threadpool
    /// thread_pool.shutdown().wait().unwrap();
    /// # }
    /// ```
    pub fn spawn<F>(&self, future: F) -> Result<(), SpawnError>
    where F: Future<Item = (), Error = ()> + Send + 'static,
    {
        let mut s = self;
        tokio_executor::Executor::spawn(&mut s, Box::new(future))
    }

    /// Logic to prepare for spawning
    fn prepare_for_spawn(&self) -> Result<(), SpawnError> {
        let mut state: State = self.inner.state.load(Acquire).into();

        // Increment the number of futures spawned on the pool as well as
        // validate that the pool is still running/
        loop {
            let mut next = state;

            if next.num_futures() == MAX_FUTURES {
                // No capacity
                return Err(SpawnError::at_capacity());
            }

            if next.lifecycle() == SHUTDOWN_NOW {
                // Cannot execute the future, executor is shutdown.
                return Err(SpawnError::shutdown());
            }

            next.inc_num_futures();

            let actual = self.inner.state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                trace!("execute; count={:?}", next.num_futures());
                break;
            }

            state = actual;
        }

        Ok(())
    }
}

impl tokio_executor::Executor for Sender {
    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        let s = self;
        tokio_executor::Executor::status(&s)
    }

    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        let mut s = &*self;
        tokio_executor::Executor::spawn(&mut s, future)
    }

    #[cfg(feature = "unstable-futures")]
    fn spawn2(&mut self, f: Task2) -> Result<(), futures2::executor::SpawnError> {
        futures2::executor::Executor::spawn(self, f)
    }
}

impl<'a> tokio_executor::Executor for &'a Sender {
    fn status(&self) -> Result<(), tokio_executor::SpawnError> {
        let state: State = self.inner.state.load(Acquire).into();

        if state.num_futures() == MAX_FUTURES {
            // No capacity
            return Err(SpawnError::at_capacity());
        }

        if state.lifecycle() == SHUTDOWN_NOW {
            // Cannot execute the future, executor is shutdown.
            return Err(SpawnError::shutdown());
        }

        Ok(())
    }

    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        self.prepare_for_spawn()?;

        // At this point, the pool has accepted the future, so schedule it for
        // execution.

        // Create a new task for the future
        let task = Task::new(future);

        self.inner.submit(task, &self.inner);

        Ok(())
    }

    #[cfg(feature = "unstable-futures")]
    fn spawn2(&mut self, f: Task2) -> Result<(), futures2::executor::SpawnError> {
        futures2::executor::Executor::spawn(self, f)
    }
}

impl<T> future::Executor<T> for Sender
where T: Future<Item = (), Error = ()> + Send + 'static,
{
    fn execute(&self, future: T) -> Result<(), future::ExecuteError<T>> {
        if let Err(e) = tokio_executor::Executor::status(self) {
            let kind = if e.is_at_capacity() {
                future::ExecuteErrorKind::NoCapacity
            } else {
                future::ExecuteErrorKind::Shutdown
            };

            return Err(future::ExecuteError::new(kind, future));
        }

        let _ = self.spawn(future);
        Ok(())
    }
}

#[cfg(feature = "unstable-futures")]
type Task2 = Box<futures2::Future<Item = (), Error = futures2::Never> + Send>;

#[cfg(feature = "unstable-futures")]
impl futures2::executor::Executor for Sender {
    fn spawn(&mut self, f: Task2) -> Result<(), futures2::executor::SpawnError> {
        let mut s = &*self;
        futures2::executor::Executor::spawn(&mut s, f)
    }

    fn status(&self) -> Result<(), futures2::executor::SpawnError> {
        let s = &*self;
        futures2::executor::Executor::status(&s)
    }
}

#[cfg(feature = "unstable-futures")]
impl<'a> futures2::executor::Executor for &'a Sender {
    fn spawn(&mut self, f: Task2) -> Result<(), futures2::executor::SpawnError> {
        self.prepare_for_spawn()
            // TODO: get rid of this once the futures crate adds more error types
            .map_err(|_| futures2::executor::SpawnError::shutdown())?;

        // At this point, the pool has accepted the future, so schedule it for
        // execution.

        // Create a new task for the future
        let task = Task::new2(f, |id| into_waker(Arc::new(Futures2Wake::new(id, &self.inner))));

        self.inner.submit(task, &self.inner);

        Ok(())
    }

    fn status(&self) -> Result<(), futures2::executor::SpawnError> {
        tokio_executor::Executor::status(self)
        // TODO: get rid of this once the futures crate adds more error types
            .map_err(|_| futures2::executor::SpawnError::shutdown())
    }
}


impl Clone for Sender {
    #[inline]
    fn clone(&self) -> Sender {
        let inner = self.inner.clone();
        Sender { inner }
    }
}

// ===== impl ShutdownTask =====

impl ShutdownTask {
    #[cfg(not(feature = "unstable-futures"))]
    fn notify(&self) {
        self.task1.notify();
    }

    #[cfg(feature = "unstable-futures")]
    fn notify(&self) {
        self.task1.notify();
        self.task2.wake();
    }
}

// ===== impl Shutdown =====

impl Shutdown {
    fn inner(&self) -> &Inner {
        &*self.inner.inner
    }
}

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        use futures::task;
        trace!("Shutdown::poll");

        self.inner().shutdown_task.task1.register_task(task::current());

        if 0 != self.inner().num_workers.load(Acquire) {
            return Ok(Async::NotReady);
        }

        Ok(().into())
    }
}

#[cfg(feature = "unstable-futures")]
impl futures2::Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self, cx: &mut futures2::task::Context) -> futures2::Poll<(), ()> {
        trace!("Shutdown::poll");

        self.inner().shutdown_task.task2.register(cx.waker());

        if 0 != self.inner().num_workers.load(Acquire) {
            return Ok(futures2::Async::Pending);
        }

        Ok(().into())
    }
}

// ===== impl Inner =====

impl Inner {
    /// Start shutting down the pool. This means that no new futures will be
    /// accepted.
    fn shutdown(&self, now: bool, purge_queue: bool) {
        let mut state: State = self.state.load(Acquire).into();

        trace!("shutdown; state={:?}", state);

        // For now, this must be true
        debug_assert!(!purge_queue || now);

        // Start by setting the SHUTDOWN flag
        loop {
            let mut next = state;

            let num_futures = next.num_futures();

            if next.lifecycle() >= SHUTDOWN_NOW {
                // Already transitioned to shutting down state

                if !purge_queue || num_futures == 0 {
                    // Nothing more to do
                    return;
                }

                // The queue must be purged
                debug_assert!(purge_queue);
                next.clear_num_futures();
            } else {
                next.set_lifecycle(if now || num_futures == 0 {
                    // If already idle, always transition to shutdown now.
                    SHUTDOWN_NOW
                } else {
                    SHUTDOWN_ON_IDLE
                });

                if purge_queue {
                    next.clear_num_futures();
                }
            }

            let actual = self.state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if state == actual {
                state = next;
                break;
            }

            state = actual;
        }

        trace!("  -> transitioned to shutdown");

        // Only transition to terminate if there are no futures currently on the
        // pool
        if state.num_futures() != 0 {
            return;
        }

        self.terminate_sleeping_workers();
    }

    fn terminate_sleeping_workers(&self) {
        trace!("  -> shutting down workers");
        // Wakeup all sleeping workers. They will wake up, see the state
        // transition, and terminate.
        while let Some((idx, worker_state)) = self.pop_sleeper(WORKER_SIGNALED, TERMINATED) {
            trace!("  -> shutdown worker; idx={:?}; state={:?}", idx, worker_state);
            self.signal_stop(idx, worker_state);
        }
    }

    /// Signals to the worker that it should stop
    fn signal_stop(&self, idx: usize, mut state: WorkerState) {
        let worker = &self.workers[idx];

        // Transition the worker state to signaled
        loop {
            let mut next = state;

            match state.lifecycle() {
                WORKER_SHUTDOWN => {
                    trace!("signal_stop -- WORKER_SHUTDOWN; idx={}", idx);
                    // If the worker is in the shutdown state, then it will never be
                    // started again.
                    self.worker_terminated();

                    return;
                }
                WORKER_RUNNING | WORKER_SLEEPING => {}
                _ => {
                    trace!("signal_stop -- skipping; idx={}; state={:?}", idx, state);
                    // All other states will naturally converge to a state of
                    // shutdown.
                    return;
                }
            }

            next.set_lifecycle(WORKER_SIGNALED);

            let actual = worker.state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                break;
            }

            state = actual;
        }

        // Wakeup the worker
        worker.wakeup();
    }

    fn worker_terminated(&self) {
        let prev = self.num_workers.fetch_sub(1, AcqRel);

        trace!("worker_terminated; num_workers={}", prev - 1);

        if 1 == prev {
            trace!("notifying shutdown task");
            self.shutdown_task.notify();
        }
    }

    /// Submit a task to the scheduler.
    ///
    /// Called from either inside or outside of the scheduler. If currently on
    /// the scheduler, then a fast path is taken.
    fn submit(&self, task: Task, inner: &Arc<Inner>) {
        Worker::with_current(|worker| {
            match worker {
                Some(worker) => {
                    let idx = worker.idx;

                    trace!("    -> submit internal; idx={}", idx);

                    worker.inner.workers[idx].submit_internal(task);
                    worker.inner.signal_work(inner);
                }
                None => {
                    self.submit_external(task, inner);
                }
            }
        });
    }

    /// Submit a task to the scheduler from off worker
    ///
    /// Called from outside of the scheduler, this function is how new tasks
    /// enter the system.
    fn submit_external(&self, task: Task, inner: &Arc<Inner>) {
        // First try to get a handle to a sleeping worker. This ensures that
        // sleeping tasks get woken up
        if let Some((idx, state)) = self.pop_sleeper(WORKER_NOTIFIED, EMPTY) {
            trace!("submit to existing worker; idx={}; state={:?}", idx, state);
            self.submit_to_external(idx, task, state, inner);
            return;
        }

        // All workers are active, so pick a random worker and submit the
        // task to it.
        let len = self.workers.len();
        let idx = self.rand_usize() % len;

        trace!("  -> submitting to random; idx={}", idx);

        let state: WorkerState = self.workers[idx].state.load(Acquire).into();
        self.submit_to_external(idx, task, state, inner);
    }

    fn submit_to_external(&self,
                          idx: usize,
                          task: Task,
                          state: WorkerState,
                          inner: &Arc<Inner>)
    {
        let entry = &self.workers[idx];

        if !entry.submit_external(task, state) {
            Worker::spawn(idx, inner);
        }
    }

    /// If there are any other workers currently relaxing, signal them that work
    /// is available so that they can try to find more work to process.
    fn signal_work(&self, inner: &Arc<Inner>) {
        if let Some((idx, mut state)) = self.pop_sleeper(WORKER_SIGNALED, EMPTY) {
            let entry = &self.workers[idx];

            // Transition the worker state to signaled
            loop {
                let mut next = state;

                // pop_sleeper should skip these
                debug_assert!(state.lifecycle() != WORKER_SIGNALED);
                next.set_lifecycle(WORKER_SIGNALED);

                let actual = entry.state.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual == state {
                    break;
                }

                state = actual;
            }

            // The state has been transitioned to signal, now we need to wake up
            // the worker if necessary.
            match state.lifecycle() {
                WORKER_SLEEPING => {
                    trace!("signal_work -- wakeup; idx={}", idx);
                    self.workers[idx].wakeup();
                }
                WORKER_SHUTDOWN => {
                    trace!("signal_work -- spawn; idx={}", idx);
                    Worker::spawn(idx, inner);
                }
                _ => {}
            }
        }
    }

    /// Push a worker on the sleep stack
    ///
    /// Returns `Err` if the pool has been terminated
    fn push_sleeper(&self, idx: usize) -> Result<(), ()> {
        let mut state: SleepStack = self.sleep_stack.load(Acquire).into();

        debug_assert!(WorkerState::from(self.workers[idx].state.load(Relaxed)).is_pushed());

        loop {
            let mut next = state;

            let head = state.head();

            if head == TERMINATED {
                // The pool is terminated, cannot push the sleeper.
                return Err(());
            }

            self.workers[idx].set_next_sleeper(head);
            next.set_head(idx);

            let actual = self.sleep_stack.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if state == actual {
                return Ok(());
            }

            state = actual;
        }
    }

    /// Pop a worker from the sleep stack
    fn pop_sleeper(&self, max_lifecycle: usize, terminal: usize)
        -> Option<(usize, WorkerState)>
    {
        debug_assert!(terminal == EMPTY || terminal == TERMINATED);

        let mut state: SleepStack = self.sleep_stack.load(Acquire).into();

        loop {
            let head = state.head();

            if head == EMPTY {
                let mut next = state;
                next.set_head(terminal);

                if next == state {
                    debug_assert!(terminal == EMPTY);
                    return None;
                }

                let actual = self.sleep_stack.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual != state {
                    state = actual;
                    continue;
                }

                return None;
            } else if head == TERMINATED {
                return None;
            }

            debug_assert!(head < MAX_WORKERS);

            let mut next = state;

            let next_head = self.workers[head].next_sleeper();

            // TERMINATED can never be set as the "next pointer" on a worker.
            debug_assert!(next_head != TERMINATED);

            if next_head == EMPTY {
                next.set_head(terminal);
            } else {
                next.set_head(next_head);
            }

            let actual = self.sleep_stack.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                // The worker has been removed from the stack, so the pushed bit
                // can be unset. Release ordering is used to ensure that this
                // operation happens after actually popping the task.
                debug_assert_eq!(1, PUSHED_MASK);

                // Unset the PUSHED flag and get the current state.
                let state: WorkerState = self.workers[head].state
                    .fetch_sub(PUSHED_MASK, Release).into();

                if state.lifecycle() >= max_lifecycle {
                    // If the worker has already been notified, then it is
                    // warming up to do more work. In this case, try to pop
                    // another thread that might be in a relaxed state.
                    continue;
                }

                return Some((head, state));
            }

            state = actual;
        }
    }

    /// Generates a random number
    ///
    /// Uses a thread-local seeded XorShift.
    fn rand_usize(&self) -> usize {
        // Use a thread-local random number generator. If the thread does not
        // have one yet, then seed a new one
        thread_local!(static THREAD_RNG_KEY: UnsafeCell<Option<XorShiftRng>> = UnsafeCell::new(None));

        THREAD_RNG_KEY.with(|t| {
            #[cfg(target_pointer_width = "32")]
            fn new_rng(thread_id: usize) -> XorShiftRng {
                XorShiftRng::from_seed([
                    thread_id as u32,
                    0x00000000,
                    0xa8a7d469,
                    0x97830e05])
            }

            #[cfg(target_pointer_width = "64")]
            fn new_rng(thread_id: usize) -> XorShiftRng {
                XorShiftRng::from_seed([
                    thread_id as u32,
                    (thread_id >> 32) as u32,
                    0xa8a7d469,
                    0x97830e05])
            }

            let thread_id = self.next_thread_id.fetch_add(1, Relaxed);
            let rng = unsafe { &mut *t.get() };

            if rng.is_none() {
                *rng = Some(new_rng(thread_id));
            }

            rng.as_mut().unwrap().next_u32() as usize
        })
    }
}

impl Notify for Notifier {
    fn notify(&self, id: usize) {
        trace!("Notifier::notify; id=0x{:x}", id);

        let id = id as usize;
        let task = unsafe { Task::from_notify_id_ref(&id) };

        if !task.schedule() {
            trace!("    -> task already scheduled");
            // task is already scheduled, there is nothing more to do
            return;
        }

        // TODO: Check if the pool is still running

        // Bump the ref count
        let task = task.clone();

        if let Some(inner) = self.inner.upgrade() {
            let _ = inner.submit(task, &inner);
        }
    }

    fn clone_id(&self, id: usize) -> usize {
        unsafe {
            let handle = Task::from_notify_id_ref(&id);
            mem::forget(handle.clone());
        }

        id
    }

    fn drop_id(&self, id: usize) {
        unsafe {
            let _ = Task::from_notify_id(id);
        }
    }
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

// ===== impl Worker =====

impl Worker {
    fn spawn(idx: usize, inner: &Arc<Inner>) {
        trace!("spawning new worker thread; idx={}", idx);

        let mut th = thread::Builder::new();

        if let Some(ref prefix) = inner.config.name_prefix {
            th = th.name(format!("{}{}", prefix, idx));
        }

        if let Some(stack) = inner.config.stack_size {
            th = th.stack_size(stack);
        }

        let inner = inner.clone();

        th.spawn(move || {
            let worker = Worker {
                inner: inner,
                idx: idx,
                should_finalize: Cell::new(false),
                _p: PhantomData,
            };

            // Make sure the ref to the worker does not move
            let wref = &worker;

            // Create another worker... It's ok, this is just a new type around
            // `Inner` that is expected to stay on the current thread.
            CURRENT_WORKER.with(|c| {
                c.set(wref as *const _);

                let inner = wref.inner.clone();
                let mut sender = Sender { inner };

                // Enter an execution context
                let mut enter = tokio_executor::enter().unwrap();

                tokio_executor::with_default(&mut sender, &mut enter, |enter| {
                    if let Some(ref callback) = wref.inner.config.around_worker {
                        callback.call(wref, enter);
                    } else {
                        wref.run();
                    }
                });
            });
        }).unwrap();
    }

    fn with_current<F: FnOnce(Option<&Worker>) -> R, R>(f: F) -> R {
        CURRENT_WORKER.with(move |c| {
            let ptr = c.get();

            if ptr.is_null() {
                f(None)
            } else {
                f(Some(unsafe { &*ptr }))
            }
        })
    }

    /// Run the worker
    ///
    /// This function blocks until the worker is shutting down.
    pub fn run(&self) {
        // Get the notifier.
        let notify = Arc::new(Notifier {
            inner: Arc::downgrade(&self.inner),
        });
        let mut sender = Sender { inner: self.inner.clone() };

        let mut first = true;
        let mut spin_cnt = 0;

        while self.check_run_state(first) {
            first = false;

            // Poll inbound until empty, transfering all tasks to the internal
            // queue.
            let consistent = self.drain_inbound();

            // Run the next available task
            if self.try_run_task(&notify, &mut sender) {
                spin_cnt = 0;
                // As long as there is work, keep looping.
                continue;
            }

            // No work in this worker's queue, it is time to try stealing.
            if self.try_steal_task(&notify, &mut sender) {
                spin_cnt = 0;
                continue;
            }

            if !consistent {
                spin_cnt = 0;
                continue;
            }

            // Starting to get sleeeeepy
            if spin_cnt < 32 {
                spin_cnt += 1;

                // Don't do anything further
            } else if spin_cnt < 256 {
                spin_cnt += 1;

                // Yield the thread
                thread::yield_now();
            } else {
                if !self.sleep() {
                    return;
                }
            }

            // If there still isn't any work to do, shutdown the worker?
        }

        self.should_finalize.set(true);
    }

    /// Checks the worker's current state, updating it as needed.
    ///
    /// Returns `true` if the worker should run.
    #[inline]
    fn check_run_state(&self, first: bool) -> bool {
        let mut state: WorkerState = self.entry().state.load(Acquire).into();

        loop {
            let pool_state: State = self.inner.state.load(Acquire).into();

            if pool_state.is_terminated() {
                return false;
            }

            let mut next = state;

            match state.lifecycle() {
                WORKER_RUNNING => break,
                WORKER_NOTIFIED | WORKER_SIGNALED => {
                    // transition back to running
                    next.set_lifecycle(WORKER_RUNNING);
                }
                lifecycle => panic!("unexpected worker state; lifecycle={}", lifecycle),
            }

            let actual = self.entry().state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                break;
            }

            state = actual;
        }

        // If this is the first iteration of the worker loop, then the state can
        // be signaled.
        if !first && state.is_signaled() {
            trace!("Worker::check_run_state; delegate signal");
            // This worker is not ready to be signaled, so delegate the signal
            // to another worker.
            self.inner.signal_work(&self.inner);
        }

        true
    }

    /// Runs the next task on this worker's queue.
    ///
    /// Returns `true` if work was found.
    #[inline]
    fn try_run_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
        use deque::Steal::*;

        // Poll the internal queue for a task to run
        match self.entry().deque.steal() {
            Data(task) => {
                self.run_task(task, notify, sender);
                true
            }
            Empty => false,
            Retry => true,
        }
    }

    /// Tries to steal a task from another worker.
    ///
    /// Returns `true` if work was found
    #[inline]
    fn try_steal_task(&self, notify: &Arc<Notifier>, sender: &mut Sender) -> bool {
        use deque::Steal::*;

        let len = self.inner.workers.len();
        let mut idx = self.inner.rand_usize() % len;
        let mut found_work = false;
        let start = idx;

        loop {
            if idx < len {
                match self.inner.workers[idx].steal.steal() {
                    Data(task) => {
                        trace!("stole task");

                        self.run_task(task, notify, sender);

                        trace!("try_steal_task -- signal_work; self={}; from={}",
                               self.idx, idx);

                        // Signal other workers that work is available
                        self.inner.signal_work(&self.inner);

                        return true;
                    }
                    Empty => {}
                    Retry => found_work = true,
                }

                idx += 1;
            } else {
                idx = 0;
            }

            if idx == start {
                break;
            }
        }

        found_work
    }

    fn run_task(&self, task: Task, notify: &Arc<Notifier>, sender: &mut Sender) {
        use task::Run::*;

        match task.run(notify, sender) {
            Idle => {}
            Schedule => {
                self.entry().push_internal(task);
            }
            Complete => {
                let mut state: State = self.inner.state.load(Acquire).into();

                loop {
                    let mut next = state;
                    next.dec_num_futures();

                    let actual = self.inner.state.compare_and_swap(
                        state.into(), next.into(), AcqRel).into();

                    if actual == state {
                        trace!("task complete; state={:?}", next);

                        if state.num_futures() == 1 {
                            // If the thread pool has been flagged as shutdown,
                            // start terminating workers. This involves waking
                            // up any sleeping worker so that they can notice
                            // the shutdown state.
                            if next.is_terminated() {
                                self.inner.terminate_sleeping_workers();
                            }
                        }

                        // The worker's run loop will detect the shutdown state
                        // next iteration.
                        return;
                    }

                    state = actual;
                }
            }
        }
    }

    /// Drains all tasks on the extern queue and pushes them onto the internal
    /// queue.
    ///
    /// Returns `true` if the operation was able to complete in a consistent
    /// state.
    #[inline]
    fn drain_inbound(&self) -> bool {
        use task::Poll::*;

        let mut found_work = false;

        loop {
            let task = unsafe { self.entry().inbound.poll() };

            match task {
                Empty => {
                    if found_work {
                        trace!("found work while draining; signal_work");
                        self.inner.signal_work(&self.inner);
                    }

                    return true;
                }
                Inconsistent => {
                    if found_work {
                        trace!("found work while draining; signal_work");
                        self.inner.signal_work(&self.inner);
                    }

                    return false;
                }
                Data(task) => {
                    found_work = true;
                    self.entry().push_internal(task);
                }
            }
        }
    }

    /// Put the worker to sleep
    ///
    /// Returns `true` if woken up due to new work arriving.
    #[inline]
    fn sleep(&self) -> bool {
        trace!("Worker::sleep; idx={}", self.idx);

        let mut state: WorkerState = self.entry().state.load(Acquire).into();

        // The first part of the sleep process is to transition the worker state
        // to "pushed". Now, it may be that the worker is already pushed on the
        // sleeper stack, in which case, we don't push again. However, part of
        // this process is also to do some final state checks to avoid entering
        // the mutex if at all possible.

        loop {
            let mut next = state;

            match state.lifecycle() {
                WORKER_RUNNING => {
                    // Try setting the pushed state
                    next.set_pushed();
                }
                WORKER_NOTIFIED | WORKER_SIGNALED => {
                    // No need to sleep, transition back to running and move on.
                    next.set_lifecycle(WORKER_RUNNING);
                }
                actual => panic!("unexpected worker state; {}", actual),
            }

            let actual = self.entry().state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                if state.is_notified() {
                    // The previous state was notified, so we don't need to
                    // sleep.
                    return true;
                }

                if !state.is_pushed() {
                    debug_assert!(next.is_pushed());

                    trace!("  sleeping -- push to stack; idx={}", self.idx);

                    // We obtained permission to push the worker into the
                    // sleeper queue.
                    if let Err(_) = self.inner.push_sleeper(self.idx) {
                        trace!("  sleeping -- push to stack failed; idx={}", self.idx);
                        // The push failed due to the pool being terminated.
                        //
                        // This is true because the "work" being woken up for is
                        // shutting down.
                        return true;
                    }
                }

                break;
            }

            state = actual;
        }

        // Acquire the sleep mutex, the state is transitioned to sleeping within
        // the mutex in order to avoid losing wakeup notifications.
        let mut lock = self.entry().park_mutex.lock().unwrap();

        // Transition the state to sleeping, a CAS is still needed as other
        // state transitions could happen unrelated to the sleep / wakeup
        // process. We also have to redo the lifecycle check done above as
        // the state could have been transitioned before entering the mutex.
        loop {
            let mut next = state;

            match state.lifecycle() {
                WORKER_RUNNING => {}
                WORKER_NOTIFIED | WORKER_SIGNALED => {
                    // Release the lock, sleep will not happen this call.
                    drop(lock);

                    // Transition back to running
                    loop {
                        let mut next = state;
                        next.set_lifecycle(WORKER_RUNNING);

                        let actual = self.entry().state.compare_and_swap(
                            state.into(), next.into(), AcqRel).into();

                        if actual == state {
                            return true;
                        }

                        state = actual;
                    }
                }
                _ => unreachable!(),
            }

            trace!(" sleeping -- set WORKER_SLEEPING; idx={}", self.idx);

            next.set_lifecycle(WORKER_SLEEPING);

            let actual = self.entry().state.compare_and_swap(
                state.into(), next.into(), AcqRel).into();

            if actual == state {
                break;
            }

            state = actual;
        }

        trace!("    -> starting to sleep; idx={}", self.idx);

        let sleep_until = self.inner.config.keep_alive
            .map(|dur| Instant::now() + dur);

        // The state has been transitioned to sleeping, we can now wait on the
        // condvar. This is done in a loop as condvars can wakeup spuriously.
        loop {
            let mut drop_thread = false;

            lock = match sleep_until {
                Some(when) => {
                    let now = Instant::now();

                    if when >= now {
                        drop_thread = true;
                    }

                    let dur = when - now;

                    self.entry().park_condvar
                        .wait_timeout(lock, dur)
                        .unwrap().0
                }
                None => {
                    self.entry().park_condvar.wait(lock).unwrap()
                }
            };

            trace!("    -> wakeup; idx={}", self.idx);

            // Reload the state
            state = self.entry().state.load(Acquire).into();

            loop {
                match state.lifecycle() {
                    WORKER_SLEEPING => {}
                    WORKER_NOTIFIED | WORKER_SIGNALED => {
                        // Release the lock, done sleeping
                        drop(lock);

                        // Transition back to running
                        loop {
                            let mut next = state;
                            next.set_lifecycle(WORKER_RUNNING);

                            let actual = self.entry().state.compare_and_swap(
                                state.into(), next.into(), AcqRel).into();

                            if actual == state {
                                return true;
                            }

                            state = actual;
                        }
                    }
                    _ => unreachable!(),
                }

                if !drop_thread {
                    break;
                }

                let mut next = state;
                next.set_lifecycle(WORKER_SHUTDOWN);

                let actual = self.entry().state.compare_and_swap(
                    state.into(), next.into(), AcqRel).into();

                if actual == state {
                    // Transitioned to a shutdown state
                    return false;
                }

                state = actual;
            }

            // The worker hasn't been notified, go back to sleep
        }
    }

    fn entry(&self) -> &WorkerEntry {
        &self.inner.workers[self.idx]
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        trace!("shutting down thread; idx={}", self.idx);

        if self.should_finalize.get() {
            // Drain all work
            self.drain_inbound();

            while let Some(_) = self.entry().deque.pop() {
            }

            // TODO: Drain the work queue...
            self.inner.worker_terminated();
        }
    }
}

// ===== impl State =====

impl State {
    #[inline]
    fn new() -> State {
        State(0)
    }

    /// Returns the number of futures still pending completion.
    fn num_futures(&self) -> usize {
        self.0 >> NUM_FUTURES_OFFSET
    }

    /// Increment the number of futures pending completion.
    ///
    /// Returns false on failure.
    fn inc_num_futures(&mut self) {
        debug_assert!(self.num_futures() < MAX_FUTURES);
        debug_assert!(self.lifecycle() < SHUTDOWN_NOW);

        self.0 += 1 << NUM_FUTURES_OFFSET;
    }

    /// Decrement the number of futures pending completion.
    fn dec_num_futures(&mut self) {
        let num_futures = self.num_futures();

        if num_futures == 0 {
            // Already zero
            return;
        }

        self.0 -= 1 << NUM_FUTURES_OFFSET;

        if self.lifecycle() == SHUTDOWN_ON_IDLE && num_futures == 1 {
            self.0 = SHUTDOWN_NOW;
        }
    }

    /// Set the number of futures pending completion to zero
    fn clear_num_futures(&mut self) {
        self.0 = self.0 & LIFECYCLE_MASK;
    }

    fn lifecycle(&self) -> usize {
        self.0 & LIFECYCLE_MASK
    }

    fn set_lifecycle(&mut self, val: usize) {
        self.0 = (self.0 & NUM_FUTURES_MASK) | val;
    }

    fn is_terminated(&self) -> bool {
        self.lifecycle() == SHUTDOWN_NOW && self.num_futures() == 0
    }
}

impl From<usize> for State {
    fn from(src: usize) -> Self {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> Self {
        src.0
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("State")
            .field("lifecycle", &self.lifecycle())
            .field("num_futures", &self.num_futures())
            .finish()
    }
}

// ===== impl SleepStack =====

impl SleepStack {
    #[inline]
    fn new() -> SleepStack {
        SleepStack(EMPTY)
    }

    #[inline]
    fn head(&self) -> usize {
        self.0 & STACK_MASK
    }

    #[inline]
    fn set_head(&mut self, val: usize) {
        // The ABA guard protects against the ABA problem w/ treiber stacks
        let aba_guard = ((self.0 >> ABA_GUARD_SHIFT) + 1) & ABA_GUARD_MASK;

        self.0 = (aba_guard << ABA_GUARD_SHIFT) | val;
    }
}

impl From<usize> for SleepStack {
    fn from(src: usize) -> Self {
        SleepStack(src)
    }
}

impl From<SleepStack> for usize {
    fn from(src: SleepStack) -> Self {
        src.0
    }
}

impl fmt::Debug for SleepStack {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let head = self.head();

        let mut fmt = fmt.debug_struct("SleepStack");

        if head < MAX_WORKERS {
            fmt.field("head", &head);
        } else if head == EMPTY {
            fmt.field("head", &"EMPTY");
        } else if head == TERMINATED {
            fmt.field("head", &"TERMINATED");
        }

        fmt.finish()
    }
}

// ===== impl WorkerEntry =====

impl WorkerEntry {
    fn new() -> Self {
        let w = deque::Deque::new();
        let s = w.stealer();

        WorkerEntry {
            state: AtomicUsize::new(WorkerState::default().into()),
            next_sleeper: UnsafeCell::new(0),
            deque: w,
            steal: s,
            inbound: task::Queue::new(),
            park_mutex: Mutex::new(()),
            park_condvar: Condvar::new(),
        }
    }

    #[inline]
    fn submit_internal(&self, task: Task) {
        self.push_internal(task);
    }

    /// Submits a task to the worker. This assumes that the caller is external
    /// to the worker. Internal submissions go through another path.
    ///
    /// Returns `false` if the worker needs to be spawned.
    fn submit_external(&self, task: Task, mut state: WorkerState) -> bool {
        // Push the task onto the external queue
        self.push_external(task);

        loop {
            let mut next = state;
            next.notify();

            let actual = self.state.compare_and_swap(
                state.into(), next.into(),
                AcqRel).into();

            if state == actual {
                break;
            }

            state = actual;
        }

        match state.lifecycle() {
            WORKER_SLEEPING => {
                // The worker is currently sleeping, the condition variable must
                // be signaled
                self.wakeup();
                true
            }
            WORKER_SHUTDOWN => false,
            _ => true,
        }
    }

    #[inline]
    fn push_external(&self, task: Task) {
        self.inbound.push(task);
    }

    #[inline]
    fn push_internal(&self, task: Task) {
        self.deque.push(task);
    }

    #[inline]
    fn wakeup(&self) {
        let _lock = self.park_mutex.lock().unwrap();
        self.park_condvar.notify_one();
    }

    #[inline]
    fn next_sleeper(&self) -> usize {
        unsafe { *self.next_sleeper.get() }
    }

    #[inline]
    fn set_next_sleeper(&self, val: usize) {
        unsafe { *self.next_sleeper.get() = val; }
    }
}

// ===== impl WorkerState =====

impl WorkerState {
    /// Returns true if the worker entry is pushed in the sleeper stack
    fn is_pushed(&self) -> bool {
        self.0 & PUSHED_MASK == PUSHED_MASK
    }

    fn set_pushed(&mut self) {
        self.0 |= PUSHED_MASK
    }

    fn is_notified(&self) -> bool {
        match self.lifecycle() {
            WORKER_NOTIFIED | WORKER_SIGNALED => true,
            _ => false,
        }
    }

    fn lifecycle(&self) -> usize {
        (self.0 & WORKER_LIFECYCLE_MASK) >> WORKER_LIFECYCLE_SHIFT
    }

    fn set_lifecycle(&mut self, val: usize) {
        self.0 = (self.0 & !WORKER_LIFECYCLE_MASK) |
            (val << WORKER_LIFECYCLE_SHIFT)
    }

    fn is_signaled(&self) -> bool {
        self.lifecycle() == WORKER_SIGNALED
    }

    fn notify(&mut self) {
        if self.lifecycle() != WORKER_SIGNALED {
            self.set_lifecycle(WORKER_NOTIFIED)
        }
    }
}

impl Default for WorkerState {
    fn default() -> WorkerState {
        // All workers will start pushed in the sleeping stack
        WorkerState(PUSHED_MASK)
    }
}

impl From<usize> for WorkerState {
    fn from(src: usize) -> Self {
        WorkerState(src)
    }
}

impl From<WorkerState> for usize {
    fn from(src: WorkerState) -> Self {
        src.0
    }
}

impl fmt::Debug for WorkerState {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("WorkerState")
            .field("lifecycle", &match self.lifecycle() {
                WORKER_SHUTDOWN => "WORKER_SHUTDOWN",
                WORKER_RUNNING => "WORKER_RUNNING",
                WORKER_SLEEPING => "WORKER_SLEEPING",
                WORKER_NOTIFIED => "WORKER_NOTIFIED",
                WORKER_SIGNALED => "WORKER_SIGNALED",
                _ => unreachable!(),
            })
            .field("is_pushed", &self.is_pushed())
            .finish()
    }
}

// ===== impl Callback =====

impl Callback {
    fn new<F>(f: F) -> Self
        where F: Fn(&Worker, &mut Enter) + Send + Sync + 'static
    {
        Callback { f: Arc::new(f) }
    }

    pub fn call(&self, worker: &Worker, enter: &mut Enter) {
        (self.f)(worker, enter)
    }
}

impl fmt::Debug for Callback {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Fn")
    }
}

// ===== impl Futures2Wake =====

#[cfg(feature = "unstable-futures")]
impl Futures2Wake {
    fn new(id: usize, inner: &Arc<Inner>) -> Futures2Wake {
        let notifier = Arc::new(Notifier {
            inner: Arc::downgrade(inner),
        });
        Futures2Wake { id, notifier }
    }
}

#[cfg(feature = "unstable-futures")]
impl Drop for Futures2Wake {
    fn drop(&mut self) {
        self.notifier.drop_id(self.id)
    }
}

#[cfg(feature = "unstable-futures")]
struct ArcWrapped(PhantomData<Futures2Wake>);

#[cfg(feature = "unstable-futures")]
unsafe impl futures2::task::UnsafeWake for ArcWrapped {
    unsafe fn clone_raw(&self) -> futures2::task::Waker {
        let me: *const ArcWrapped = self;
        let arc = (*(&me as *const *const ArcWrapped as *const Arc<Futures2Wake>)).clone();
        arc.notifier.clone_id(arc.id);
        into_waker(arc)
    }

    unsafe fn drop_raw(&self) {
        let mut me: *const ArcWrapped = self;
        let me = &mut me as *mut *const ArcWrapped as *mut Arc<Futures2Wake>;
        (*me).notifier.drop_id((*me).id);
        ::std::ptr::drop_in_place(me);
    }

    unsafe fn wake(&self) {
        let me: *const ArcWrapped = self;
        let me = &me as *const *const ArcWrapped as *const Arc<Futures2Wake>;
        (*me).notifier.notify((*me).id)
    }
}

#[cfg(feature = "unstable-futures")]
fn into_waker(rc: Arc<Futures2Wake>) -> futures2::task::Waker {
    unsafe {
        let ptr = mem::transmute::<Arc<Futures2Wake>, *mut ArcWrapped>(rc);
        futures2::task::Waker::new(ptr)
    }
}
