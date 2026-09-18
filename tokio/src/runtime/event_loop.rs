//! A `current_thread` runtime whose wait belongs to a host event loop.
//!
//! The scheduler drives to a fixed point, then waits. A native runtime waits
//! by parking its thread in the driver; an event loop instead returns to the
//! host, and asks to be called back through a [`Waker`] the host provided.
//! Whatever would have unparked the thread (a spawn, a task woken from
//! another thread, I/O readiness, a timer deadline) wakes the host instead,
//! and the host calls [`EventLoop::drive`].
//!
//! The driver still parks somewhere: a thread owned by the event loop blocks
//! in it exactly as a native runtime's thread would, and readiness reaches
//! tasks through the same cross-thread schedule that wakes the host. Nothing
//! about the platform's reactor or timers is exposed; the host's whole
//! contract is the waker and `drive`.

use crate::runtime::driver::Driver;
use crate::runtime::local_runtime::LocalRuntime;
use crate::runtime::scheduler::current_thread::CurrentThread;
use crate::runtime::{context, Handle, Runtime};
use crate::task::JoinHandle;
use crate::util::trace::SpawnMeta;

use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Waker;
use std::thread::ThreadId;

/// A [`Runtime`] driven by a host event loop instead of by parking a thread.
///
/// Built with [`Builder::build_event_loop`], which takes a [`Waker`] the host
/// owns. Whenever the runtime has work, it wakes that waker, and the host
/// calls [`drive`](Self::drive) in response; the runtime does the rest. Tasks
/// are submitted with [`spawn`](Self::spawn) and run in batches from those
/// drives. [`block_on`](Self::block_on) runs a future only as far as ready
/// work carries it, and errors rather than wait.
///
/// The runtime is single-threaded (`current_thread`), but like `Runtime` it
/// is `Send + Sync`: it may be driven from any thread, one at a time.
///
/// Dropping the `EventLoop` shuts the runtime down as dropping a `Runtime`
/// does. The waker may be woken once more during the drop.
///
/// [`Builder::build_event_loop`]: crate::runtime::Builder::build_event_loop
#[derive(Debug)]
pub struct EventLoop {
    shared: Shared<Runtime>,
}

/// A [`LocalRuntime`] driven by a host event loop instead of by parking a
/// thread.
///
/// As [`EventLoop`], but tasks need not be `Send` and are submitted with
/// [`spawn_local`](Self::spawn_local). The host must drive it on the thread
/// that built it.
///
/// [`Builder::build_local_event_loop`]: crate::runtime::Builder::build_local_event_loop
#[derive(Debug)]
pub struct LocalEventLoop {
    shared: Shared<LocalRuntime>,
}

/// The error returned by [`EventLoop::block_on`] when the future did not
/// complete without waiting.
///
/// The future has been dropped. Progress it made, and any tasks it spawned,
/// remain: the tasks continue from later drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WouldBlock(pub(crate) ());

impl std::fmt::Display for WouldBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the future did not complete without blocking")
    }
}

impl std::error::Error for WouldBlock {}

impl EventLoop {
    pub(crate) fn new(runtime: Runtime, waker: Waker) -> io::Result<EventLoop> {
        Ok(EventLoop {
            shared: Shared::new(runtime, waker, None)?,
        })
    }

    /// Spawns a future onto the runtime. It is queued and the host woken; it
    /// never runs before `spawn` returns.
    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.shared.handle.spawn(future)
    }

    /// Returns a handle to the runtime.
    pub fn handle(&self) -> &Handle {
        &self.shared.handle
    }

    /// Runs one batch of ready tasks (at most `event_interval`). If ready work
    /// remains afterwards the host is woken again at once, so it gets a turn
    /// between batches.
    ///
    /// Call this whenever the waker is woken. Calling it at other times is
    /// harmless.
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime.
    pub fn drive(&self) {
        self.shared.drive();
    }

    /// Runs `future` to completion on the runtime, along with any tasks that
    /// become ready, without ever waiting. Where a [`Runtime::block_on`]
    /// would park the thread, this returns [`WouldBlock`] and drops the
    /// future instead; spawned tasks it left behind continue from later
    /// drives.
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime.
    #[track_caller]
    pub fn block_on<F: Future>(&self, future: F) -> Result<F::Output, WouldBlock> {
        self.shared.block_on(future)
    }
}

impl LocalEventLoop {
    pub(crate) fn new(runtime: LocalRuntime, waker: Waker) -> io::Result<LocalEventLoop> {
        let tid = std::thread::current().id();
        Ok(LocalEventLoop {
            shared: Shared::new(runtime, waker, Some(tid))?,
        })
    }

    /// Spawns a future onto the runtime. It is queued and the host woken; it
    /// never runs before `spawn_local` returns.
    #[track_caller]
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let meta = SpawnMeta::new_unnamed(std::mem::size_of::<F>());
        // SAFETY: `LocalEventLoop` is `!Send`, so this is the thread that
        // built the runtime, and `drive` polls only on that thread.
        unsafe { self.shared.handle.spawn_local_named(future, meta) }
    }

    /// Returns a handle to the runtime.
    pub fn handle(&self) -> &Handle {
        &self.shared.handle
    }

    /// Runs one batch; see [`EventLoop::drive`].
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime, or on a thread other than the
    /// one that built the event loop.
    pub fn drive(&self) {
        self.shared.drive();
    }

    /// Runs `future` as far as ready work carries it; see
    /// [`EventLoop::block_on`].
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime, or on a thread other than the
    /// one that built the event loop.
    #[track_caller]
    pub fn block_on<F: Future>(&self, future: F) -> Result<F::Output, WouldBlock> {
        self.shared.block_on(future)
    }
}

/// Either runtime kind, as the `current_thread` scheduler it wraps.
pub(crate) trait Scheduler {
    fn current_thread(&self) -> &CurrentThread;
    fn handle(&self) -> &Handle;
}

#[derive(Debug)]
struct Shared<R: Scheduler> {
    /// Dropped last: its shutdown needs the driver back.
    runtime: R,
    handle: Handle,
    reactor: Option<Reactor>,
    local_tid: Option<ThreadId>,
}

impl<R: Scheduler> Shared<R> {
    fn new(runtime: R, waker: Waker, local_tid: Option<ThreadId>) -> io::Result<Shared<R>> {
        let handle = runtime.handle().clone();
        handle.inner.driver().set_host(waker);
        let reactor = Reactor::start(&runtime, &handle)?;
        Ok(Shared {
            runtime,
            handle,
            reactor: Some(reactor),
            local_tid,
        })
    }

    fn check_thread(&self) {
        if let Some(tid) = self.local_tid {
            assert_eq!(
                std::thread::current().id(),
                tid,
                "a `LocalEventLoop` must be driven on the thread that built it"
            );
        }
    }

    fn drive(&self) {
        self.check_thread();
        let handle = self.handle.inner.as_current_thread();
        let busy = context::enter_runtime(&self.handle.inner, false, |_| {
            self.runtime.current_thread().drive_batch(handle)
        });
        if busy {
            self.handle.inner.driver().wake_host();
        }
    }

    fn block_on<F: Future>(&self, future: F) -> Result<F::Output, WouldBlock> {
        self.check_thread();
        let handle = self.handle.inner.as_current_thread();
        context::enter_runtime(&self.handle.inner, false, |_| {
            self.runtime.current_thread().block_on_ready(handle, future)
        })
    }
}

impl<R: Scheduler> Drop for Shared<R> {
    fn drop(&mut self) {
        if let Some(driver) = self.reactor.take().and_then(|r| r.stop(&self.handle)) {
            let handle = self.handle.inner.as_current_thread();
            self.runtime
                .current_thread()
                .restore_driver(handle, driver);
        }
    }
}

/// The thread that parks in the driver on the runtime's behalf.
///
/// It runs the same `park` a native runtime's thread would, so I/O
/// readiness, timer deadlines and signal delivery all happen here, and reach
/// the scheduler through the cross-thread schedule, which wakes the host.
/// A nearer timer registered from a drive unparks it to re-arm, as on a
/// multi-thread runtime.
#[derive(Debug)]
struct Reactor {
    thread: std::thread::JoinHandle<Driver>,
    stop: Arc<AtomicBool>,
}

impl Reactor {
    fn start<R: Scheduler>(runtime: &R, handle: &Handle) -> io::Result<Reactor> {
        let scheduler = handle.inner.as_current_thread();
        let mut driver = runtime
            .current_thread()
            .take_driver(scheduler)
            .expect("driver missing");
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let handle = handle.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("tokio-event-loop-driver".into())
                .spawn(move || {
                    while !stop.load(Ordering::Acquire) {
                        driver.park(handle.inner.driver());
                    }
                    driver
                })?
        };
        Ok(Reactor { thread, stop })
    }

    fn stop(self, handle: &Handle) -> Option<Driver> {
        self.stop.store(true, Ordering::Release);
        handle.inner.driver().unpark();
        self.thread.join().ok()
    }
}
