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

#![allow(deprecated)]

mod scheduler;
use self::scheduler::Scheduler;

use tokio_executor::{self, Enter, SpawnError};
use tokio_executor::park::{Park, Unpark, ParkThread};

use futures::{executor, Async, Future};
use futures::future::{self, Executor, ExecuteError, ExecuteErrorKind};

use std::fmt;
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::sync::mpsc;

#[cfg(feature = "unstable-futures")]
use futures2;

/// Executes tasks on the current thread
pub struct CurrentThread<P: Park = ParkThread> {
    /// Execute futures and receive unpark notifications.
    scheduler: Scheduler<P::Unpark>,

    /// Current number of futures being executed
    num_futures: usize,

    /// Thread park handle
    park: P,

    /// Handle for spawning new futures from other threads
    spawn_handle: Handle,

    /// Receiver for futures spawned from other threads
    spawn_receiver: mpsc::Receiver<Box<Future<Item = (), Error = ()> + Send + 'static>>,
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
    polled: bool
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

#[deprecated(since = "0.1.2", note = "use block_on_all instead")]
#[doc(hidden)]
#[derive(Debug)]
pub struct Context<'a> {
    cancel: Cell<bool>,
    _p: PhantomData<&'a ()>,
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    _p: (),
}

/// Error returned by the `run_timeout` function.
#[derive(Debug)]
pub struct RunTimeoutError {
    timeout: bool,
}

/// Error returned by the `turn` function.
#[derive(Debug)]
pub struct TurnError {
    _p: (),
}

/// Error returned by the `block_on` function.
#[derive(Debug)]
pub struct BlockError<T> {
    inner: Option<T>,
}

/// This is mostly split out to make the borrow checker happy.
struct Borrow<'a, U: 'a> {
    scheduler: &'a mut Scheduler<U>,
    num_futures: &'a mut usize,
}

trait SpawnLocal {
    fn spawn_local(&mut self, future: Box<Future<Item = (), Error = ()>>);
}

struct CurrentRunner {
    spawn: Cell<Option<*mut SpawnLocal>>,
}

/// Current thread's task runner. This is set in `TaskRunner::with`
thread_local!(static CURRENT: CurrentRunner = CurrentRunner {
    spawn: Cell::new(None),
});

#[deprecated(since = "0.1.2", note = "use block_on_all instead")]
#[doc(hidden)]
#[allow(deprecated)]
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
where F: Future,
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
where F: Future<Item = (), Error = ()> + 'static
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

        let scheduler = Scheduler::new(unpark);
        let notify = scheduler.notify();

        CurrentThread {
            scheduler: scheduler,
            num_futures: 0,
            park,
            spawn_handle: Handle { sender: spawn_sender, notify: notify },
            spawn_receiver: spawn_receiver,
        }
    }

    /// Returns `true` if the executor is currently idle.
    ///
    /// An idle executor is defined by not currently having any spawned tasks.
    pub fn is_idle(&self) -> bool {
        self.num_futures == 0
    }

    /// Spawn the future on the executor.
    ///
    /// This internally queues the future to be executed once `run` is called.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where F: Future<Item = (), Error = ()> + 'static,
    {
        self.borrow().spawn_local(Box::new(future));
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
    pub fn block_on<F>(&mut self, future: F)
        -> Result<F::Item, BlockError<F::Error>>
    where F: Future
    {
        let mut enter = tokio_executor::enter().unwrap();
        self.enter(&mut enter).block_on(future)
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        let mut enter = tokio_executor::enter().unwrap();
        self.enter(&mut enter).run()
    }

    /// Run the executor to completion, blocking the thread until all
    /// spawned futures have completed **or** `duration` time has elapsed.
    pub fn run_timeout(&mut self, duration: Duration)
        -> Result<(), RunTimeoutError>
    {
        let mut enter = tokio_executor::enter().unwrap();
        self.enter(&mut enter).run_timeout(duration)
    }

    /// Perform a single iteration of the event loop.
    ///
    /// This function blocks the current thread even if the executor is idle.
    pub fn turn(&mut self, duration: Option<Duration>)
        -> Result<Turn, TurnError>
    {
        let mut enter = tokio_executor::enter().unwrap();
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
            scheduler: &mut self.scheduler,
            num_futures: &mut self.num_futures,
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

impl tokio_executor::Executor for CurrentThread {
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        self.borrow().spawn_local(future);
        Ok(())
    }

    #[cfg(feature = "unstable-futures")]
    fn spawn2(&mut self, _future: Box<futures2::Future<Item = (), Error = futures2::Never> + Send>)
        -> Result<(), futures2::executor::SpawnError>
    {
        panic!("Futures 0.2 integration is not available for current_thread");
    }
}

impl<P: Park> fmt::Debug for CurrentThread<P> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("CurrentThread")
            .field("scheduler", &self.scheduler)
            .field("num_futures", &self.num_futures)
            .finish()
    }
}

// ===== impl Entered =====

impl<'a, P: Park> Entered<'a, P> {
    /// Spawn the future on the executor.
    ///
    /// This internally queues the future to be executed once `run` is called.
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where F: Future<Item = (), Error = ()> + 'static,
    {
        self.executor.borrow().spawn_local(Box::new(future));
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
    pub fn block_on<F>(&mut self, future: F)
        -> Result<F::Item, BlockError<F::Error>>
    where F: Future
    {
        let mut future = executor::spawn(future);
        let notify = self.executor.scheduler.notify();

        loop {
            let res = self.executor.borrow().enter(self.enter, || {
                future.poll_future_notify(&notify, 0)
            });

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
        self.run_timeout2(None)
            .map_err(|_| RunError { _p: () })
    }

    /// Run the executor to completion, blocking the thread until all
    /// spawned futures have completed **or** `duration` time has elapsed.
    pub fn run_timeout(&mut self, duration: Duration)
        -> Result<(), RunTimeoutError>
    {
        self.run_timeout2(Some(duration))
    }

    /// Perform a single iteration of the event loop.
    ///
    /// This function blocks the current thread even if the executor is idle.
    pub fn turn(&mut self, duration: Option<Duration>)
        -> Result<Turn, TurnError>
    {
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

    fn run_timeout2(&mut self, dur: Option<Duration>)
        -> Result<(), RunTimeoutError>
    {
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
                scheduler: &mut self.executor.scheduler,
                num_futures: &mut self.executor.num_futures,
            },
            &mut self.executor.spawn_receiver,
        );

        while let Ok(future) = spawn_receiver.try_recv() {
            borrow.spawn_local(future);
        }

        // After any pending futures were scheduled, do the actual tick
        borrow.scheduler.tick(
            &mut *self.enter,
            borrow.num_futures)
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
    sender: mpsc::Sender<Box<Future<Item = (), Error = ()> + Send + 'static>>,
    notify: executor::NotifyHandle,
}

// Manual implementation because the Sender does not implement Debug
impl fmt::Debug for Handle {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Handle")
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
    where F: Future<Item = (), Error = ()> + Send + 'static {
        self.sender.send(Box::new(future))
            .expect("CurrentThread does not exist anymore");
        // use 0 for the id, CurrentThread does not make use of it
        self.notify.notify(0);

        Ok(())
    }
}

// ===== impl TaskExecutor =====

#[deprecated(since = "0.1.2", note = "use TaskExecutor::current instead")]
#[doc(hidden)]
pub fn task_executor() -> TaskExecutor {
    TaskExecutor {
        _p: ::std::marker::PhantomData,
    }
}

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

    /// Spawn a future onto the current `CurrentThread` instance.
    pub fn spawn_local(&mut self, future: Box<Future<Item = (), Error = ()>>)
        -> Result<(), SpawnError>
    {
        CURRENT.with(|current| {
            match current.spawn.get() {
                Some(spawn) => {
                    unsafe { (*spawn).spawn_local(future) };
                    Ok(())
                }
                None => {
                    Err(SpawnError::shutdown())
                }
            }
        })
    }
}

impl tokio_executor::Executor for TaskExecutor {
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        self.spawn_local(future)
    }

    #[cfg(feature = "unstable-futures")]
    fn spawn2(&mut self, _future: Box<futures2::Future<Item = (), Error = futures2::Never> + Send>)
        -> Result<(), futures2::executor::SpawnError>
    {
        panic!("Futures 0.2 integration is not available for current_thread");
    }

    fn status(&self) -> Result<(), SpawnError> {
        CURRENT.with(|current| {
            if current.spawn.get().is_some() {
                Ok(())
            } else {
                Err(SpawnError::shutdown())
            }
        })
    }
}

impl<F> Executor<F> for TaskExecutor
where F: Future<Item = (), Error = ()> + 'static
{
    fn execute(&self, future: F) -> Result<(), ExecuteError<F>> {
        CURRENT.with(|current| {
            match current.spawn.get() {
                Some(spawn) => {
                    unsafe { (*spawn).spawn_local(Box::new(future)) };
                    Ok(())
                }
                None => {
                    Err(ExecuteError::new(ExecuteErrorKind::Shutdown, future))
                }
            }
        })
    }
}

// ===== impl Context =====

impl<'a> Context<'a> {
    /// Cancels *all* executing futures.
    pub fn cancel_all_spawned(&self) {
        self.cancel.set(true);
    }
}

// ===== impl Borrow =====

impl<'a, U: Unpark> Borrow<'a, U> {
    fn enter<F, R>(&mut self, _: &mut Enter, f: F) -> R
    where F: FnOnce() -> R,
    {
        CURRENT.with(|current| {
            current.set_spawn(self, || {
                f()
            })
        })
    }
}

impl<'a, U: Unpark> SpawnLocal for Borrow<'a, U> {
    fn spawn_local(&mut self, future: Box<Future<Item = (), Error = ()>>) {
        *self.num_futures += 1;
        self.scheduler.schedule(future);
    }
}

// ===== impl CurrentRunner =====

impl CurrentRunner {
    fn set_spawn<F, R>(&self, spawn: &mut SpawnLocal, f: F) -> R
    where F: FnOnce() -> R
    {
        struct Reset<'a>(&'a CurrentRunner);

        impl<'a> Drop for Reset<'a> {
            fn drop(&mut self) {
                self.0.spawn.set(None);
            }
        }

        let _reset = Reset(self);

        let spawn = unsafe { hide_lt(spawn as *mut SpawnLocal) };
        self.spawn.set(Some(spawn));

        f()
    }
}

unsafe fn hide_lt<'a>(p: *mut (SpawnLocal + 'a)) -> *mut (SpawnLocal + 'static) {
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
