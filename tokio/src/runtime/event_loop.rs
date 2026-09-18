//! A `current_thread` runtime whose wait belongs to a host event loop.
//!
//! The scheduler drives to a fixed point, then waits. A native runtime waits
//! by parking its thread in the driver; an event loop instead returns to the
//! host, and asks to be called back through a [`Waker`] the host provided.
//! Whatever would have unparked the thread (a spawn, a task woken from
//! another thread, I/O readiness, a timer deadline) wakes the host instead,
//! and the host calls [`LocalEventLoop::drive`].
//!
//! The driver still parks somewhere: a thread owned by the event loop blocks
//! in it exactly as a native runtime's thread would, and readiness reaches
//! tasks through the same cross-thread schedule that wakes the host. Nothing
//! about the platform's reactor or timers is exposed; the host's whole
//! contract is the waker and `drive`.

mod reactor;
use reactor::Reactor;

use crate::runtime::local_runtime::LocalRuntime;
use crate::runtime::{context, Handle};
use crate::task::JoinHandle;
use crate::util::trace::SpawnMeta;

use std::future::Future;
use std::io;
use std::rc::Rc;
use std::sync::OnceLock;
use std::task::Waker;
use std::thread::ThreadId;

/// A [`LocalRuntime`] driven by a host event loop instead of by parking a
/// thread.
///
/// Built with [`Builder::build_local_event_loop`], which takes a [`Waker`]
/// the host owns. Whenever the runtime has work, it wakes that waker, and the
/// host calls [`drive`](Self::drive) in response; the runtime does the rest.
/// Tasks are submitted with [`spawn_local`](Self::spawn_local), or from any
/// thread through the [`Handle`], and run in batches from those drives.
/// [`block_on`](Self::block_on) runs a future only as far as ready work
/// carries it, and errors rather than wait.
///
/// Like `LocalRuntime` the event loop is `!Send`: the host drives it on the
/// thread that built it. [`Handle::block_on`] from another thread works as
/// on a native runtime, since the driver is parked on a thread of its own.
///
/// [`Handle::block_on`]: crate::runtime::Handle::block_on
/// Dropping the `LocalEventLoop` shuts the runtime down as dropping a
/// `LocalRuntime` does. The waker may be woken once more during the drop.
///
/// [`Builder::build_local_event_loop`]: crate::runtime::Builder::build_local_event_loop
#[derive(Debug)]
pub struct LocalEventLoop {
    shared: Rc<Shared>,
}

/// The error returned by [`LocalEventLoop::block_on`] when the future did
/// not complete without waiting.
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

impl LocalEventLoop {
    pub(crate) fn new(runtime: LocalRuntime, waker: Waker) -> io::Result<LocalEventLoop> {
        Ok(LocalEventLoop {
            shared: Shared::new(runtime, waker)?,
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

    /// Runs one batch of ready tasks (at most `event_interval`). If ready work
    /// remains afterwards the host is woken again at once, so it gets a turn
    /// between batches.
    ///
    /// Call this whenever the waker is woken. A wake does not guarantee work:
    /// the runtime wakes the host from every path that would unpark a native
    /// runtime's thread, some of which find nothing to run, so a drive that
    /// does nothing is normal. Calling it without a wake is harmless too.
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime, or on a thread other than the
    /// one that built the event loop, or if a task panicked and the runtime
    /// is configured to [shut down on unhandled panics].
    ///
    /// [shut down on unhandled panics]: crate::runtime::Builder::unhandled_panic
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
    /// Panics if called from within a runtime, or on a thread other than the
    /// one that built the event loop, or if a task panicked and the runtime
    /// is configured to [shut down on unhandled panics].
    ///
    /// [`Runtime::block_on`]: crate::runtime::Runtime::block_on
    /// [shut down on unhandled panics]: crate::runtime::Builder::unhandled_panic
    #[track_caller]
    pub fn block_on<F: Future>(&self, future: F) -> Result<F::Output, WouldBlock> {
        self.shared.block_on(future)
    }
}

#[derive(Debug)]
struct Shared {
    /// Its shutdown needs the driver back, which `Drop` restores before the
    /// fields drop.
    runtime: LocalRuntime,
    handle: Handle,
    reactor: OnceLock<Reactor>,
    tid: ThreadId,
}

impl Shared {
    fn new(runtime: LocalRuntime, waker: Waker) -> io::Result<Rc<Shared>> {
        let handle = runtime.handle().clone();
        let shared = Rc::new(Shared {
            runtime,
            handle,
            reactor: OnceLock::new(),
            tid: std::thread::current().id(),
        });
        shared.handle.inner.driver().set_host(waker);
        let reactor = Reactor::start(&shared)?;
        let _ = shared.reactor.set(reactor);
        Ok(shared)
    }

    fn check_thread(&self) {
        assert_eq!(
            std::thread::current().id(),
            self.tid,
            "a `LocalEventLoop` must be driven on the thread that built it"
        );
    }

    fn after_turn(&self, busy: bool) {
        if busy {
            self.handle.inner.driver().wake_host();
        }
        if let Some(reactor) = self.reactor.get() {
            reactor.after_turn(&self.handle);
        }
    }

    fn drive(&self) {
        self.check_thread();
        let handle = self.handle.inner.as_current_thread();
        let busy = context::enter_runtime(&self.handle.inner, false, |_| {
            self.runtime.current_thread().drive_batch(handle)
        });
        self.after_turn(busy);
    }

    fn block_on<F: Future>(&self, future: F) -> Result<F::Output, WouldBlock> {
        self.check_thread();
        let handle = self.handle.inner.as_current_thread();
        let (ret, busy) = context::enter_runtime(&self.handle.inner, false, |_| {
            self.runtime.current_thread().block_on_ready(handle, future)
        });
        self.after_turn(busy);
        ret
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        if let Some(driver) = self.reactor.take().and_then(|r| r.stop(&self.handle)) {
            let handle = self.handle.inner.as_current_thread();
            self.runtime.current_thread().restore_driver(handle, driver);
        }
    }
}
