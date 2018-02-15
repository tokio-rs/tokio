//! Execute tasks on the current thread
//!
//! This module implements an executor that keeps futures on the same thread
//! that they are submitted on. This allows it to execute futures that are
//! not `Send`.
//!
//! Before being able to spawn futures with this module, an executor
//! context must be setup by calling [`run`]. From within that context [`spawn`]
//! may be called with the future to run in the background.
//!
//! ```
//! # extern crate tokio;
//! # extern crate futures;
//! use tokio::executor::current_thread;
//! use futures::future::lazy;
//!
//! // Calling execute here results in a panic
//! // current_thread::spawn(my_future);
//!
//! # pub fn main() {
//! current_thread::run(|_| {
//!     // The execution context is setup, futures may be executed.
//!     current_thread::spawn(lazy(|| {
//!         println!("called from the current thread executor");
//!         Ok(())
//!     }));
//! });
//! # }
//! ```
//!
//! # Execution model
//!
//! When an execution context is setup with `run` the current thread will block
//! and all the futures managed by the executor are driven to completion.
//! Whenever a future receives a notification, it is pushed to the end of a
//! scheduled list. The executor will drain this list, advancing the state of
//! each future.
//!
//! All futures managed by this module will remain on the current thread,
//! as such, this module is able to safely execute futures that are not `Send`.
//!
//! Once a future is complete, it is dropped. Once all futures are completed,
//! [`run`] will unblock and return.
//!
//! This module makes a best effort to fairly schedule futures that it manages.
//!
//! [`spawn`]: fn.spawn.html
//! [`run`]: fn.run.html

mod scheduler;

use tokio_executor::{self, SpawnError};
use tokio_executor::park::{Park, Unpark, ParkThread};

use futures::Async;
use futures::executor::{self, Spawn};
use futures::future::{Future, Executor, ExecuteError, ExecuteErrorKind};

use std::{fmt, thread};
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Executes tasks on the current thread
pub struct CurrentThread<P: Park = ParkThread> {
    /// Execute futures and receive unpark notifications.
    scheduler: Scheduler<P::Unpark>,

    /// Current number of futures being executed
    num_futures: usize,

    /// Parks the thread, waiting for notifications.
    park: P,
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

/// A context yielded to the closure provided to `run`.
///
/// This context is mostly a future-proofing of the library to add future
/// contextual information into it. Currently it only contains the `Enter`
/// instance used to reserve the current thread for blocking on futures.
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

/// Wraps a spawned boxed future
struct Task(Spawn<Box<Future<Item = (), Error = ()>>>);

/// Alias the scheduler
type Scheduler<T> = scheduler::Scheduler<Task, T>;

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

/// Calls the given closure, then block until all futures submitted for
/// execution complete.
///
/// In more detail, this function will block until:
/// - All executing futures are complete, or
/// - `cancel_all_spawned` is invoked.
pub fn run<F, R>(f: F) -> R
where F: FnOnce(&mut Context) -> R
{
    let mut context = Context {
        cancel: Cell::new(false),
        _p: PhantomData,
    };

    let mut current_thread = CurrentThread::new();
    let ret = current_thread
        .with_context(|| f(&mut context))
        .unwrap();

    if context.cancel.get() {
        return ret;
    }

    current_thread.run().unwrap();
    ret
}

/// Run the executor seeded with the given future.
pub fn run_seeded<F>(f: F) -> Result<F::Item, F::Error>
where F: Future,
{
    let mut current_thread = CurrentThread::new();
    let ret = current_thread.block_on(f);
    current_thread.run().unwrap();
    ret.map_err(|e| e.into_inner().expect("unexpected execution error"))
}

/// Executes a future on the current thread.
///
/// The provided future must complete or be canceled before `run` will return.
///
/// # Panics
///
/// This function can only be invoked from the context of a `run` call; any
/// other use will result in a panic.
pub fn spawn<F>(future: F)
where F: Future<Item = (), Error = ()> + 'static
{
    task_executor().execute(Box::new(future)).unwrap();
}

// ===== impl CurrentThread =====

impl CurrentThread<ParkThread> {
    /// Create a new instance of `CurrentThread`.
    pub fn new() -> Self {
        CurrentThread::new_with_park(ParkThread::new())
    }
}

impl<P> CurrentThread<P>
where P: Park,
{
    /// Create a new instance of `CurrentThread` backed by the given park
    /// handle.
    pub fn new_with_park(park: P) -> Self {
        let unpark = park.unpark();

        CurrentThread {
            scheduler: Scheduler::new(unpark),
            num_futures: 0,
            park,
        }
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
    where F: Future
    {
        let mut future = executor::spawn(future);
        loop {
            match future.poll_future_notify(self.scheduler.notify(), 0) {
                Ok(Async::Ready(e)) => return Ok(e),
                Err(e) => return Err(BlockError { inner: Some(e) }),
                Ok(Async::NotReady) => {}
            }
            self.turn()?;

            if let Err(_) = self.park.park() {
                return Err(BlockError { inner: None });
            }
        }
    }

    /// Spawn the future on the executor.
    ///
    /// This internally queues the future to be executed once `run` is called.
    pub fn spawn<F>(&mut self, future: F)
    where F: Future<Item = (), Error = ()> + 'static,
    {
        self.borrow().spawn_local(Box::new(future));
    }

    /// Run the given closure with the global executor context set.
    ///
    /// From within the given closure, the Tokio global executor context is set,
    /// allowing the usage of the free `spawn` and `spawn_local` functions.
    ///
    /// This function may not be called while currently in an executor context.
    ///
    /// See [tokio-executor] for more details.
    pub fn with_context<F, R>(&mut self, f: F) -> Result<R, tokio_executor::EnterError>
    where F: FnOnce() -> R
    {
        self.borrow().enter(f)
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        self.run_timeout2(None)
            .map_err(|_| RunError { _p: () })
    }

    /// Run the executor to completion, blocking the thread until all
    /// spawned futures have completed **or** `duration` time has elapsed.
    pub fn run_timeout(&mut self, duration: Duration) -> Result<(), RunTimeoutError> {
        self.run_timeout2(Some(duration))
    }

    /// Returns `true` if the executor is currently idle.
    ///
    /// An idle executor is defined by not currently having any spawned tasks.
    pub fn is_idle(&self) -> bool {
        self.num_futures == 0
    }

    fn run_timeout2(&mut self, dur: Option<Duration>) -> Result<(), RunTimeoutError> {
        if self.is_idle() {
            // Nothing to do
            return Ok(());
        }

        let mut time = dur.map(|dur| (Instant::now() + dur, dur));

        loop {
            self.turn()?;

            if self.is_idle() {
                return Ok(());
            }

            match time {
                Some((until, rem)) => {
                    if let Err(_) = self.park.park_timeout(rem) {
                        return Err(RunTimeoutError::new(false));
                    }

                    let now = Instant::now();

                    if now >= until {
                        return Err(RunTimeoutError::new(true));
                    }

                    time = Some((until, until - now));
                }
                None => {
                    if let Err(_) = self.park.park() {
                        return Err(RunTimeoutError::new(false));
                    }
                }
            }
        }
    }

    fn turn(&mut self) -> Result<(), tokio_executor::EnterError> {
        use self::scheduler::Tick;
        // TODO: All this can be cleaned up

        loop {
            let num_futures = &mut self.num_futures;

            // Try to advance the scheduler state
            let res = self.scheduler.tick(|scheduler, spawned, notify| {
                let mut borrow = Borrow {
                    scheduler,
                    num_futures,
                };

                // Enter the executor context
                let res = borrow.enter(|| {
                    // Tick the future
                    match spawned.0.poll_future_notify(notify, 0) {
                        Ok(Async::Ready(_)) | Err(_) => true,
                        Ok(Async::NotReady) => false,
                    }
                });

                match res {
                    Ok(future_completed) => {
                        // A future completed, decrement the future count
                        if future_completed {
                            debug_assert!(*borrow.num_futures > 0);
                            *borrow.num_futures -= 1;
                        }

                        // Signal to the scheduler that it should keep executing.
                        Async::NotReady
                    }
                    Err(e) => {
                        // bubble up the error
                        Async::Ready(e)
                    }
                }
            });

            // Process the result of ticking the scheduler
            match res {
                // A future completed. `is_daemon` is true when the future was
                // submitted as a daemon future.
                Tick::Data(err) => {
                    return Err(err);
                },
                Tick::Empty => {
                    return Ok(());
                }
                Tick::Inconsistent => {
                    // Yield the thread and loop
                    thread::yield_now();
                }
            }
        }
    }

    fn borrow(&mut self) -> Borrow<P::Unpark> {
        Borrow {
            scheduler: &mut self.scheduler,
            num_futures: &mut self.num_futures,
        }
    }
}

impl<P> tokio_executor::Executor for CurrentThread<P>
where P: Park,
{
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
        -> Result<(), SpawnError>
    {
        self.borrow().spawn_local(future);
        Ok(())
    }
}

impl<P> fmt::Debug for CurrentThread<P>
where P: Park + fmt::Debug,
      P::Unpark: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("CurrentThread")
            .field("scheduler", &self.scheduler)
            .field("num_futures", &self.num_futures)
            .field("park", &self.park)
            .finish()
    }
}

// ===== impl TaskExecutor =====

/// Returns an executor that executes futures on the current thread.
///
/// The user of `TaskExecutor` must ensure that when a future is submitted,
/// that it is done within the context of a call to `run`.
///
/// For more details, see the [module level](index.html) documentation.
pub fn task_executor() -> TaskExecutor {
    TaskExecutor {
        _p: ::std::marker::PhantomData,
    }
}

impl tokio_executor::Executor for TaskExecutor
{
    fn spawn(&mut self, future: Box<Future<Item = (), Error = ()> + Send>)
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

impl<'a, U> Borrow<'a, U>
where U: Unpark
{
    fn enter<F, R>(&mut self, f: F) -> Result<R, tokio_executor::EnterError>
    where F: FnOnce() -> R,
    {
        // Enter an execution context.
        let mut _enter = tokio_executor::enter()?;

        CURRENT.with(|current| {
            current.set_spawn(self, || {
                Ok(f())
            })
        })
    }

    fn schedule(&mut self, item: Task) {
        *self.num_futures += 1;
        self.scheduler.schedule(item);
    }
}

impl<'a, U> SpawnLocal for Borrow<'a, U>
where U: Unpark
{
    fn spawn_local(&mut self, future: Box<Future<Item = (), Error = ()>>) {
        self.schedule(Task::new(future));
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

    /// Returns `true` if the error was caused by the operation timeing out.
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

// ===== impl Task =====

impl Task {
    fn new(future: Box<Future<Item = (), Error = ()> + 'static>) -> Self {
        Task(executor::spawn(future))
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Task")
            .finish()
    }
}
