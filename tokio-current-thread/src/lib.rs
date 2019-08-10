#![doc(html_root_url = "https://docs.rs/tokio-current-thread/0.2.0-alpha.1")]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! A single-threaded executor which executes tasks on the same thread from which
//! they are spawned.
//!
//! [`CurrentThread`] is the main type of this crate. It executes tasks on the
//! current thread.  The easiest way to start a new [`CurrentThread`] executor
//! is to call [`block_on_all`] with an initial task to seed the executor.  All
//! tasks that are being managed by a [`CurrentThread`] executor are able to
//! spawn additional tasks by calling [`spawn`].
//!
//! Application authors will not use this crate directly. Instead, they will use
//! the `tokio` crate. Library authors should only depend on
//! `tokio-current-thread` if they are building a custom task executor.
//!
//! [`CurrentThread`]: struct.CurrentThread.html
//! [`spawn`]: fn.spawn.html
//! [`block_on_all`]: fn.block_on_all.html

mod scheduler;

use crate::scheduler::Scheduler;
use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{atomic, Arc};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::{Duration, Instant};
use tokio_executor::park::{Park, ParkThread, Unpark};
use tokio_executor::SpawnError;

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
    spawn_receiver: crossbeam_channel::Receiver<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,

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
pub struct Entered<'a, P: Park> {
    executor: &'a mut CurrentThread<P>,
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    _p: (),
}

impl fmt::Display for RunError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Run error")
    }
}

impl Error for RunError {}

/// Error returned by the `run_timeout` function.
#[derive(Debug)]
pub struct RunTimeoutError {
    timeout: bool,
}

impl fmt::Display for RunTimeoutError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let descr = if self.timeout {
            "Run timeout error (timeout)"
        } else {
            "Run timeout error (not timeout)"
        };
        write!(fmt, "{}", descr)
    }
}

impl Error for RunTimeoutError {}

/// Error returned by the `turn` function.
#[derive(Debug)]
pub struct TurnError {
    _p: (),
}

impl fmt::Display for TurnError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Turn error")
    }
}

impl Error for TurnError {}

/// Error returned by the `block_on` function.
#[derive(Debug)]
pub struct BlockError<T> {
    inner: Option<T>,
}

impl<T> fmt::Display for BlockError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Block error")
    }
}

impl<T: fmt::Debug> Error for BlockError<T> {}

/// This is mostly split out to make the borrow checker happy.
struct Borrow<'a, U> {
    id: u64,
    scheduler: &'a mut Scheduler<U>,
    num_futures: &'a atomic::AtomicUsize,
}

trait SpawnLocal {
    fn spawn_local(&mut self, future: Pin<Box<dyn Future<Output = ()>>>, already_counted: bool);
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
pub fn block_on_all<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut current_thread = CurrentThread::new();

    let ret = current_thread.block_on(future);
    current_thread.run().unwrap();
    ret
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
    F: Future<Output = ()> + 'static,
{
    TaskExecutor::current()
        .spawn_local(Box::pin(future))
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

        let (spawn_sender, spawn_receiver) = crossbeam_channel::unbounded();
        let thread = thread::current().id();
        let id = EXECUTOR_ID.with(|idc| {
            let id = idc.get();
            idc.set(id + 1);
            id
        });

        let scheduler = Scheduler::new(unpark);
        let waker = scheduler.waker();

        let num_futures = Arc::new(atomic::AtomicUsize::new(0));

        CurrentThread {
            scheduler,
            num_futures: num_futures.clone(),
            park,
            id,
            spawn_handle: Handle {
                sender: spawn_sender,
                num_futures,
                waker,
                thread,
                id,
            },
            spawn_receiver,
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
        F: Future<Output = ()> + 'static,
    {
        self.borrow().spawn_local(Box::pin(future), false);
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
    pub fn block_on<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        let _enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter().block_on(future)
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        let _enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter().run()
    }

    /// Run the executor to completion, blocking the thread until all
    /// spawned futures have completed **or** `duration` time has elapsed.
    pub fn run_timeout(&mut self, duration: Duration) -> Result<(), RunTimeoutError> {
        let _enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter().run_timeout(duration)
    }

    /// Perform a single iteration of the event loop.
    ///
    /// This function blocks the current thread even if the executor is idle.
    pub fn turn(&mut self, duration: Option<Duration>) -> Result<Turn, TurnError> {
        let _enter = tokio_executor::enter().expect("failed to start `current_thread::Runtime`");
        self.enter().turn(duration)
    }

    /// Bind `CurrentThread` instance with an execution context.
    fn enter(&mut self) -> Entered<'_, P> {
        Entered { executor: self }
    }

    /// Returns a reference to the underlying `Park` instance.
    pub fn get_park(&self) -> &P {
        &self.park
    }

    /// Returns a mutable reference to the underlying `Park` instance.
    pub fn get_park_mut(&mut self) -> &mut P {
        &mut self.park
    }

    fn borrow(&mut self) -> Borrow<'_, P::Unpark> {
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
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), SpawnError> {
        self.borrow().spawn_local(future, false);
        Ok(())
    }
}

impl<T> tokio_executor::TypedExecutor<T> for CurrentThread
where
    T: Future<Output = ()> + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), SpawnError> {
        self.borrow().spawn_local(Box::pin(future), false);
        Ok(())
    }
}

impl<P: Park> fmt::Debug for CurrentThread<P> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("CurrentThread")
            .field("scheduler", &self.scheduler)
            .field(
                "num_futures",
                &self.num_futures.load(atomic::Ordering::SeqCst),
            )
            .finish()
    }
}

impl<P: Park + Default> Default for CurrentThread<P> {
    fn default() -> Self {
        CurrentThread::new_with_park(P::default())
    }
}

// ===== impl Entered =====

impl<P: Park> Entered<'_, P> {
    /// Spawn the future on the executor.
    ///
    /// This internally queues the future to be executed once `run` is called.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where
        F: Future<Output = ()> + 'static,
    {
        self.executor.borrow().spawn_local(Box::pin(future), false);
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
    ///
    /// # Panics
    ///
    /// This function will panic if the `Park` call returns an error.
    pub fn block_on<F>(&mut self, mut future: F) -> F::Output
    where
        F: Future,
    {
        // Safety: we shadow the original `future`, so it will never move
        // again.
        let mut future = unsafe { Pin::new_unchecked(&mut future) };
        let waker = self.executor.scheduler.waker();
        let mut cx = Context::from_waker(&waker);

        loop {
            let res = self
                .executor
                .borrow()
                .enter(|| future.as_mut().poll(&mut cx));

            match res {
                Poll::Ready(e) => return e,
                Poll::Pending => {}
            }

            self.tick();

            if self.executor.park.park().is_err() {
                panic!("block_on park failed");
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
                    if self.executor.park.park_timeout(rem).is_err() {
                        return Err(RunTimeoutError::new(false));
                    }

                    let now = Instant::now();

                    if now >= until {
                        return Err(RunTimeoutError::new(true));
                    }

                    time = Some((until, until - now));
                }
                None => {
                    if self.executor.park.park().is_err() {
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
        borrow.scheduler.tick(borrow.id, borrow.num_futures)
    }
}

impl<P: Park> fmt::Debug for Entered<'_, P> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Entered")
            .field("executor", &self.executor)
            .finish()
    }
}

// ===== impl Handle =====

/// Handle to spawn a future on the corresponding `CurrentThread` instance
#[derive(Clone)]
pub struct Handle {
    sender: crossbeam_channel::Sender<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
    num_futures: Arc<atomic::AtomicUsize>,
    /// Waker to the Scheduler
    waker: Waker,
    thread: thread::ThreadId,

    /// The thread-local ID assigned to this Handle's executor.
    id: u64,
}

// Manual implementation because the Sender does not implement Debug
impl fmt::Debug for Handle {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Handle")
            .field("shut_down", &self.is_shut_down())
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
        F: Future<Output = ()> + Send + 'static,
    {
        if thread::current().id() == self.thread {
            let mut e = TaskExecutor::current();
            if e.id() == Some(self.id) {
                return e.spawn_local(Box::pin(future));
            }
        }

        // NOTE: += 2 since LSB is the shutdown bit
        let pending = self.num_futures.fetch_add(2, atomic::Ordering::SeqCst);
        if pending % 2 == 1 {
            // Bring the count back so we still know when the Runtime is idle.
            self.num_futures.fetch_sub(2, atomic::Ordering::SeqCst);

            return Err(SpawnError::shutdown());
        }

        self.sender
            .send(Box::pin(future))
            .expect("CurrentThread does not exist anymore");
        self.waker.wake_by_ref();
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
        if self.is_shut_down() {
            return Err(SpawnError::shutdown());
        }

        Ok(())
    }

    fn is_shut_down(&self) -> bool {
        // LSB of "num_futures" is the shutdown bit
        let num_futures = self.num_futures.load(atomic::Ordering::SeqCst);
        num_futures % 2 == 1
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
        future: Pin<Box<dyn Future<Output = ()>>>,
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
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Result<(), SpawnError> {
        self.spawn_local(future)
    }
}

impl<F> tokio_executor::TypedExecutor<F> for TaskExecutor
where
    F: Future<Output = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), SpawnError> {
        self.spawn_local(Box::pin(future))
    }
}

// ===== impl Borrow =====

impl<U: Unpark> Borrow<'_, U> {
    fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        CURRENT.with(|current| {
            current.id.set(Some(self.id));
            current.set_spawn(self, || f())
        })
    }
}

impl<U: Unpark> SpawnLocal for Borrow<'_, U> {
    fn spawn_local(&mut self, future: Pin<Box<dyn Future<Output = ()>>>, already_counted: bool) {
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

        impl Drop for Reset<'_> {
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
    // false positive: https://github.com/rust-lang/rust-clippy/issues/2906
    #[allow(clippy::transmute_ptr_to_ptr)]
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
