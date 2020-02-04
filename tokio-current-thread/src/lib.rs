#![doc(html_root_url = "https://docs.rs/tokio-current-thread/0.1.7")]
#![deny(missing_docs, missing_debug_implementations)]

//! A single-threaded executor which executes tasks on the same thread from which
//! they are spawned.
//!
//! > **Note:** This crate is **deprecated in tokio 0.2.x** and has been moved
//! > and refactored into various places in the [`tokio`] crate. The closest
//! replacement is to make use of [`tokio::task::LocalSet::block_on`] which
//! requires the [`rt-util` feature].
//!
//! [`tokio`]: https://docs.rs/tokio/latest/tokio/index.html
//! [`tokio::task::LocalSet::block_on`]: https://docs.rs/tokio/latest/tokio/task/struct.LocalSet.html#method.block_on
//! [`rt-util` feature]: https://docs.rs/tokio/latest/tokio/index.html#feature-flags
//!
//! The crate provides:
//!
//! * [`CurrentThread`] is the main type of this crate. It executes tasks on the current thread.
//!   The easiest way to start a new [`CurrentThread`] executor is to call
//!   [`block_on_all`] with an initial task to seed the executor.
//!   All tasks that are being managed by a [`CurrentThread`] executor are able to
//!   spawn additional tasks by calling [`spawn`].
//!
//!
//! Application authors will not use this crate directly. Instead, they will use the
//! `tokio` crate. Library authors should only depend on `tokio-current-thread` if they
//! are building a custom task executor.
//!
//! For more details, see [executor module] documentation in the Tokio crate.
//!
//! [`CurrentThread`]: struct.CurrentThread.html
//! [`spawn`]: fn.spawn.html
//! [`block_on_all`]: fn.block_on_all.html
//! [executor module]: https://docs.rs/tokio/0.1/tokio/executor/index.html

extern crate futures;
extern crate tokio_executor;

mod scheduler;

use self::scheduler::Scheduler;

use tokio_executor::park::{Park, ParkThread, Unpark};
use tokio_executor::{Enter, SpawnError};

use futures::future::{ExecuteError, ExecuteErrorKind, Executor};
use futures::{executor, Async, Future};

use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::rc::Rc;
use std::sync::{atomic, mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

/// Executes tasks on the current thread
pub struct CurrentThread<P: Park = ParkThread> {
    /// Execute futures and receive unpark notifications.
    scheduler: Scheduler<P::Unpark>,

    /// Current number of futures being executed.
    ///
    /// The LSB is used to indicate that the runtime is preparing to shut down.
    /// Thus, to get the actual number of pending futures, `>>1`.
    num_futures: Arc<atomic::AtomicUsize>,

    /// Thread park handle
    park: P,

    /// Handle for spawning new futures from other threads
    spawn_handle: Handle,

    /// Receiver for futures spawned from other threads
    spawn_receiver: mpsc::Receiver<Box<dyn Future<Item = (), Error = ()> + Send + 'static>>,

    /// The thread-local ID assigned to this executor.
    id: u64,
}

/// Executes futures on the current thread.
///
/// All futures executed using this executor will be executed on the current
/// thread. As such, `run` will wait for these futures to complete before
/// returning.
///
/// For more details, see the [module level](index.html) documentation.
#[derive(Debug, Clone)]
pub struct TaskExecutor {
    // Prevent the handle from moving across threads.
    _p: ::std::marker::PhantomData<Rc<()>>,
}

/// Returned by the `turn` function.
#[derive(Debug)]
pub struct Turn {
    polled: bool,
}

impl Turn {
    /// `true` if any futures were polled at all and `false` otherwise.
    pub fn has_polled(&self) -> bool {
        self.polled
    }
}

/// A `CurrentThread` instance bound to a supplied execution context.
pub struct Entered<'a, P: Park + 'a> {
    executor: &'a mut CurrentThread<P>,
    enter: &'a mut Enter,
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    _p: (),
}

impl fmt::Display for RunError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl Error for RunError {
    fn description(&self) -> &str {
        "Run error"
    }
}

/// Error returned by the `run_timeout` function.
#[derive(Debug)]
pub struct RunTimeoutError {
    timeout: bool,
}

impl fmt::Display for RunTimeoutError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl Error for RunTimeoutError {
    fn description(&self) -> &str {
        if self.timeout {
            "Run timeout error (timeout)"
        } else {
            "Run timeout error (not timeout)"
        }
    }
}

/// Error returned by the `turn` function.
#[derive(Debug)]
pub struct TurnError {
    _p: (),
}

impl fmt::Display for TurnError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl Error for TurnError {
    fn description(&self) -> &str {
        "Turn error"
    }
}

/// Error returned by the `block_on` function.
#[derive(Debug)]
pub struct BlockError<T> {
    inner: Option<T>,
}

impl<T> fmt::Display for BlockError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Block error")
    }
}

impl<T: fmt::Debug> Error for BlockError<T> {
    fn description(&self) -> &str {
        "Block error"
    }
}

/// This is mostly split out to make the borrow checker happy.
struct Borrow<'a, U: 'a> {
    id: u64,
    scheduler: &'a mut Scheduler<U>,
    num_futures: &'a atomic::AtomicUsize,
}

trait SpawnLocal {
    fn spawn_local(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()>>,
        already_counted: bool,
    );
}

struct CurrentRunner {
    spawn: Cell<Option<*mut dyn SpawnLocal>>,
    id: Cell<Option<u64>>,
}

thread_local! {
    /// Current thread's task runner. This is set in `TaskRunner::with`
    static CURRENT: CurrentRunner = CurrentRunner {
        spawn: Cell::new(None),
        id: Cell::new(None),
    }
}

thread_local! {
    /// Unique ID to assign to each new executor launched on this thread.
    ///
    /// The unique ID is used to determine if the currently running executor matches the one
    /// referred to by a `Handle` so that direct task dispatch can be used.
    static EXECUTOR_ID: Cell<u64> = Cell::new(0)
}

/// Run the executor bootstrapping the execution with the provided future.
///
/// This creates a new [`CurrentThread`] executor, spawns the provided future,
/// and blocks the current thread until the provided future and **all**
/// subsequently spawned futures complete. In other words:
///
/// * If the provided bootstrap future does **not** spawn any additional tasks,
///   `block_on_all` returns once `future` completes.
/// * If the provided bootstrap future **does** spawn additional tasks, then
///   `block_on_all` returns once **all** spawned futures complete.
///
/// See [module level][mod] documentation for more details.
///
/// [`CurrentThread`]: struct.CurrentThread.html
/// [mod]: index.html
pub fn block_on_all<F>(future: F) -> Result<F::Item, F::Error>
where
    F: Future,
{
    let mut current_thread = CurrentThread::new();

    let ret = current_thread.block_on(future);
    current_thread.run().unwrap();

    ret.map_err(|e| e.into_inner().expect("unexpected execution error"))
}

/// Executes a future on the current thread.
///
/// The provided future must complete or be canceled before `run` will return.
///
/// Unlike [`tokio::spawn`], this function will always spawn on a
/// `CurrentThread` executor and is able to spawn futures that are not `Send`.
///
/// # Panics
///
/// This function can only be invoked from the context of a `run` call; any
/// other use will result in a panic.
///
/// [`tokio::spawn`]: ../fn.spawn.html
pub fn spawn<F>(future: F)
where
    F: Future<Item = (), Error = ()> + 'static,
{
    TaskExecutor::current()
        .spawn_local(Box::new(future))
        .unwrap();
}

// ===== impl CurrentThread =====

impl CurrentThread<ParkThread> {
    /// Create a new instance of `CurrentThread`.
    pub fn new() -> Self {
        CurrentThread::new_with_park(ParkThread::new())
    }
}

impl<P: Park> CurrentThread<P> {
    /// Create a new instance of `CurrentThread` backed by the given park
    /// handle.
    pub fn new_with_park(park: P) -> Self {
        let unpark = park.unpark();

        let (spawn_sender, spawn_receiver) = mpsc::channel();
        let thread = thread::current().id();
        let id = EXECUTOR_ID.with(|idc| {
            let id = idc.get();
            idc.set(id + 1);
            id
        });

        let scheduler = Scheduler::new(unpark);
        let notify = scheduler.notify();

        let num_futures = Arc::new(atomic::AtomicUsize::new(0));

        CurrentThread {
            scheduler: scheduler,
            num_futures: num_futures.clone(),
            park,
            id,
            spawn_handle: Handle {
                sender: spawn_sender,
                num_futures: num_futures,
                notify: notify,
                shut_down: Cell::new(false),
                thread: thread,
                id,
            },
            spawn_receiver: spawn_receiver,
        }
    }

    /// Returns `true` if the executor is currently idle.
    ///
    /// An idle executor is defined by not currently having any spawned tasks.
    ///
    /// Note that this method is inherently racy -- if a future is spawned from a remote `Handle`,
    /// this method may return `true` even though there are more futures to be executed.
    pub fn is_idle(&self) -> bool {
        self.num_futures.load(atomic::Ordering::SeqCst) <= 1
    }

    /// Spawn the future on the executor.
    ///
    /// This internally queues the future to be executed once `run` is called.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where
        F: Future<Item = (), Error = ()> + 'static,
    {
        self.borrow().spawn_local(Box::new(future), false);
        self
    }

    /// Synchronously waits for the provided `future` to complete.
    ///
    /// This function can be used to synchronously block the current thread
    /// until the provided `future` has resolved either successfully or with an
    /// error. The result of the future is then returned from this function
    /// call.
    ///
    /// Note that this function will **also** execute any spawned futures on the
    /// current thread, but will **not** block until these other spawned futures
    /// have completed.
    ///
    /// The caller is responsible for ensuring that other spawned futures
    /// complete execution.
    pub fn block_on<F>(&mut self, future: F) -> Result<F::Item, BlockError<F::Error>>
    where
        F: Future,
    {
        let mut enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter(&mut enter).block_on(future)
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        let mut enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter(&mut enter).run()
    }

    /// Run the executor to completion, blocking the thread until all
    /// spawned futures have completed **or** `duration` time has elapsed.
    pub fn run_timeout(&mut self, duration: Duration) -> Result<(), RunTimeoutError> {
        let mut enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter(&mut enter).run_timeout(duration)
    }

    /// Perform a single iteration of the event loop.
    ///
    /// This function blocks the current thread even if the executor is idle.
    pub fn turn(&mut self, duration: Option<Duration>) -> Result<Turn, TurnError> {
        let mut enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter(&mut enter).turn(duration)
    }

    /// Bind `CurrentThread` instance with an execution context.
    pub fn enter<'a>(&'a mut self, enter: &'a mut Enter) -> Entered<'a, P> {
        Entered {
            executor: self,
            enter,
        }
    }

    /// Returns a reference to the underlying `Park` instance.
    pub fn get_park(&self) -> &P {
        &self.park
    }

    /// Returns a mutable reference to the underlying `Park` instance.
    pub fn get_park_mut(&mut self) -> &mut P {
        &mut self.park
    }

    fn borrow(&mut self) -> Borrow<P::Unpark> {
        Borrow {
            id: self.id,
            scheduler: &mut self.scheduler,
            num_futures: &*self.num_futures,
        }
    }

    /// Get a new handle to spawn futures on the executor
    ///
    /// Different to the executor itself, the handle can be sent to different
    /// threads and can be used to spawn futures on the executor.
    pub fn handle(&self) -> Handle {
        self.spawn_handle.clone()
    }
}

impl<P: Park> Drop for CurrentThread<P> {
    fn drop(&mut self) {
        // Signal to Handles that no more futures can be spawned by setting LSB.
        //
        // NOTE: this isn't technically necessary since the send on the mpsc will fail once the
        // receiver is dropped, but it's useful to illustrate how clean shutdown will be
        // implemented (e.g., by setting the LSB).
        let pending = self.num_futures.fetch_add(1, atomic::Ordering::SeqCst);

        // TODO: We currently ignore any pending futures at the time we shut down.
        //
        // The "proper" fix for this is to have an explicit shutdown phase (`shutdown_on_idle`)
        // which sets LSB (as above) do make Handle::spawn stop working, and then runs until
        // num_futures.load() == 1.
        let _ = pending;
    }
}

impl tokio_executor::Executor for CurrentThread {
    fn spawn(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError> {
        self.borrow().spawn_local(future, false);
        Ok(())
    }
}

impl<T> tokio_executor::TypedExecutor<T> for CurrentThread
where
    T: Future<Item = (), Error = ()> + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), SpawnError> {
        self.borrow().spawn_local(Box::new(future), false);
        Ok(())
    }
}

impl<P: Park> fmt::Debug for CurrentThread<P> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("CurrentThread")
            .field("scheduler", &self.scheduler)
            .field(
                "num_futures",
                &self.num_futures.load(atomic::Ordering::SeqCst),
            )
            .finish()
    }
}

// ===== impl Entered =====

impl<'a, P: Park> Entered<'a, P> {
    /// Spawn the future on the executor.
    ///
    /// This internally queues the future to be executed once `run` is called.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where
        F: Future<Item = (), Error = ()> + 'static,
    {
        self.executor.borrow().spawn_local(Box::new(future), false);
        self
    }

    /// Synchronously waits for the provided `future` to complete.
    ///
    /// This function can be used to synchronously block the current thread
    /// until the provided `future` has resolved either successfully or with an
    /// error. The result of the future is then returned from this function
    /// call.
    ///
    /// Note that this function will **also** execute any spawned futures on the
    /// current thread, but will **not** block until these other spawned futures
    /// have completed.
    ///
    /// The caller is responsible for ensuring that other spawned futures
    /// complete execution.
    pub fn block_on<F>(&mut self, future: F) -> Result<F::Item, BlockError<F::Error>>
    where
        F: Future,
    {
        let mut future = executor::spawn(future);
        let notify = self.executor.scheduler.notify();

        loop {
            let res = self
                .executor
                .borrow()
                .enter(self.enter, || future.poll_future_notify(&notify, 0));

            match res {
                Ok(Async::Ready(e)) => return Ok(e),
                Err(e) => return Err(BlockError { inner: Some(e) }),
                Ok(Async::NotReady) => {}
            }

            self.tick();

            if let Err(_) = self.executor.park.park() {
                return Err(BlockError { inner: None });
            }
        }
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        self.run_timeout2(None).map_err(|_| RunError { _p: () })
    }

    /// Run the executor to completion, blocking the thread until all
    /// spawned futures have completed **or** `duration` time has elapsed.
    pub fn run_timeout(&mut self, duration: Duration) -> Result<(), RunTimeoutError> {
        self.run_timeout2(Some(duration))
    }

    /// Perform a single iteration of the event loop.
    ///
    /// This function blocks the current thread even if the executor is idle.
    pub fn turn(&mut self, duration: Option<Duration>) -> Result<Turn, TurnError> {
        let res = if self.executor.scheduler.has_pending_futures() {
            self.executor.park.park_timeout(Duration::from_millis(0))
        } else {
            match duration {
                Some(duration) => self.executor.park.park_timeout(duration),
                None => self.executor.park.park(),
            }
        };

        if res.is_err() {
            return Err(TurnError { _p: () });
        }

        let polled = self.tick();

        Ok(Turn { polled })
    }

    /// Returns a reference to the underlying `Park` instance.
    pub fn get_park(&self) -> &P {
        &self.executor.park
    }

    /// Returns a mutable reference to the underlying `Park` instance.
    pub fn get_park_mut(&mut self) -> &mut P {
        &mut self.executor.park
    }

    fn run_timeout2(&mut self, dur: Option<Duration>) -> Result<(), RunTimeoutError> {
        if self.executor.is_idle() {
            // Nothing to do
            return Ok(());
        }

        let mut time = dur.map(|dur| (Instant::now() + dur, dur));

        loop {
            self.tick();

            if self.executor.is_idle() {
                return Ok(());
            }

            match time {
                Some((until, rem)) => {
                    if let Err(_) = self.executor.park.park_timeout(rem) {
                        return Err(RunTimeoutError::new(false));
                    }

                    let now = Instant::now();

                    if now >= until {
                        return Err(RunTimeoutError::new(true));
                    }

                    time = Some((until, until - now));
                }
                None => {
                    if let Err(_) = self.executor.park.park() {
                        return Err(RunTimeoutError::new(false));
                    }
                }
            }
        }
    }

    /// Returns `true` if any futures were processed
    fn tick(&mut self) -> bool {
        // Spawn any futures that were spawned from other threads by manually
        // looping over the receiver stream

        // FIXME: Slightly ugly but needed to make the borrow checker happy
        let (mut borrow, spawn_receiver) = (
            Borrow {
                id: self.executor.id,
                scheduler: &mut self.executor.scheduler,
                num_futures: &*self.executor.num_futures,
            },
            &mut self.executor.spawn_receiver,
        );

        while let Ok(future) = spawn_receiver.try_recv() {
            borrow.spawn_local(future, true);
        }

        // After any pending futures were scheduled, do the actual tick
        borrow
            .scheduler
            .tick(borrow.id, &mut *self.enter, borrow.num_futures)
    }
}

impl<'a, P: Park> fmt::Debug for Entered<'a, P> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Entered")
            .field("executor", &self.executor)
            .field("enter", &self.enter)
            .finish()
    }
}

// ===== impl Handle =====

/// Handle to spawn a future on the corresponding `CurrentThread` instance
#[derive(Clone)]
pub struct Handle {
    sender: mpsc::Sender<Box<dyn Future<Item = (), Error = ()> + Send + 'static>>,
    num_futures: Arc<atomic::AtomicUsize>,
    shut_down: Cell<bool>,
    notify: executor::NotifyHandle,
    thread: thread::ThreadId,

    /// The thread-local ID assigned to this Handle's executor.
    id: u64,
}

// Manual implementation because the Sender does not implement Debug
impl fmt::Debug for Handle {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Handle")
            .field("shut_down", &self.shut_down.get())
            .finish()
    }
}

impl Handle {
    /// Spawn a future onto the `CurrentThread` instance corresponding to this handle
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the `CurrentThread`
    /// instance of the `Handle` does not exist anymore.
    pub fn spawn<F>(&self, future: F) -> Result<(), SpawnError>
    where
        F: Future<Item = (), Error = ()> + Send + 'static,
    {
        if thread::current().id() == self.thread {
            let mut e = TaskExecutor::current();
            if e.id() == Some(self.id) {
                return e.spawn_local(Box::new(future));
            }
        }

        if self.shut_down.get() {
            return Err(SpawnError::shutdown());
        }

        // NOTE: += 2 since LSB is the shutdown bit
        let pending = self.num_futures.fetch_add(2, atomic::Ordering::SeqCst);
        if pending % 2 == 1 {
            // Bring the count back so we still know when the Runtime is idle.
            self.num_futures.fetch_sub(2, atomic::Ordering::SeqCst);

            // Once the Runtime is shutting down, we know it won't come back.
            self.shut_down.set(true);

            return Err(SpawnError::shutdown());
        }

        self.sender
            .send(Box::new(future))
            .expect("CurrentThread does not exist anymore");
        // use 0 for the id, CurrentThread does not make use of it
        self.notify.notify(0);
        Ok(())
    }

    /// Provides a best effort **hint** to whether or not `spawn` will succeed.
    ///
    /// This function may return both false positives **and** false negatives.
    /// If `status` returns `Ok`, then a call to `spawn` will *probably*
    /// succeed, but may fail. If `status` returns `Err`, a call to `spawn` will
    /// *probably* fail, but may succeed.
    ///
    /// This allows a caller to avoid creating the task if the call to `spawn`
    /// has a high likelihood of failing.
    pub fn status(&self) -> Result<(), SpawnError> {
        if self.shut_down.get() {
            return Err(SpawnError::shutdown());
        }

        Ok(())
    }
}

// ===== impl TaskExecutor =====

impl TaskExecutor {
    /// Returns an executor that executes futures on the current thread.
    ///
    /// The user of `TaskExecutor` must ensure that when a future is submitted,
    /// that it is done within the context of a call to `run`.
    ///
    /// For more details, see the [module level](index.html) documentation.
    pub fn current() -> TaskExecutor {
        TaskExecutor {
            _p: ::std::marker::PhantomData,
        }
    }

    /// Get the current executor's thread-local ID.
    fn id(&self) -> Option<u64> {
        CURRENT.with(|current| current.id.get())
    }

    /// Spawn a future onto the current `CurrentThread` instance.
    pub fn spawn_local(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()>>,
    ) -> Result<(), SpawnError> {
        CURRENT.with(|current| match current.spawn.get() {
            Some(spawn) => {
                unsafe { (*spawn).spawn_local(future, false) };
                Ok(())
            }
            None => Err(SpawnError::shutdown()),
        })
    }
}

impl tokio_executor::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError> {
        self.spawn_local(future)
    }
}

impl<F> tokio_executor::TypedExecutor<F> for TaskExecutor
where
    F: Future<Item = (), Error = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), SpawnError> {
        self.spawn_local(Box::new(future))
    }
}

impl<F> Executor<F> for TaskExecutor
where
    F: Future<Item = (), Error = ()> + 'static,
{
    fn execute(&self, future: F) -> Result<(), ExecuteError<F>> {
        CURRENT.with(|current| match current.spawn.get() {
            Some(spawn) => {
                unsafe { (*spawn).spawn_local(Box::new(future), false) };
                Ok(())
            }
            None => Err(ExecuteError::new(ExecuteErrorKind::Shutdown, future)),
        })
    }
}

// ===== impl Borrow =====

impl<'a, U: Unpark> Borrow<'a, U> {
    fn enter<F, R>(&mut self, _: &mut Enter, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        CURRENT.with(|current| {
            current.id.set(Some(self.id));
            current.set_spawn(self, || f())
        })
    }
}

impl<'a, U: Unpark> SpawnLocal for Borrow<'a, U> {
    fn spawn_local(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()>>,
        already_counted: bool,
    ) {
        if !already_counted {
            // NOTE: we have a borrow of the Runtime, so we know that it isn't shut down.
            // NOTE: += 2 since LSB is the shutdown bit
            self.num_futures.fetch_add(2, atomic::Ordering::SeqCst);
        }
        self.scheduler.schedule(future);
    }
}

// ===== impl CurrentRunner =====

impl CurrentRunner {
    fn set_spawn<F, R>(&self, spawn: &mut dyn SpawnLocal, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        struct Reset<'a>(&'a CurrentRunner);

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.0.spawn.set(None);
                self.0.id.set(None);
            }
        }

        let _reset = Reset(self);

        let spawn = unsafe { hide_lt(spawn as *mut dyn SpawnLocal) };
        self.spawn.set(Some(spawn));

        f()
    }
}

unsafe fn hide_lt<'a>(p: *mut (dyn SpawnLocal + 'a)) -> *mut (dyn SpawnLocal + 'static) {
    use std::mem;
    mem::transmute(p)
}

// ===== impl RunTimeoutError =====

impl RunTimeoutError {
    fn new(timeout: bool) -> Self {
        RunTimeoutError { timeout }
    }

    /// Returns `true` if the error was caused by the operation timing out.
    pub fn is_timeout(&self) -> bool {
        self.timeout
    }
}

impl From<tokio_executor::EnterError> for RunTimeoutError {
    fn from(_: tokio_executor::EnterError) -> Self {
        RunTimeoutError::new(false)
    }
}

// ===== impl BlockError =====

impl<T> BlockError<T> {
    /// Returns the error yielded by the future being blocked on
    pub fn into_inner(self) -> Option<T> {
        self.inner
    }
}

impl<T> From<tokio_executor::EnterError> for BlockError<T> {
    fn from(_: tokio_executor::EnterError) -> Self {
        BlockError { inner: None }
    }
}
