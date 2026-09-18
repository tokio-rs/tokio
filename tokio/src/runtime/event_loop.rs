//! A `current_thread` runtime whose wait belongs to a host event loop.
//!
//! The scheduler drives to a fixed point, then waits. A native runtime waits
//! by parking its thread in the driver; an event loop instead returns to the
//! host, and asks to be called back through a [`Waker`] the host provided.
//! Whatever would have unparked the thread (a spawn, a task woken from
//! another thread, I/O readiness, a timer deadline) wakes the host instead,
//! and the host calls [`LocalEventLoop::drive`].
//!
//! The driver still parks somewhere. On targets with threads, a thread owned
//! by the event loop blocks in it exactly as a native runtime's thread
//! would, and readiness reaches tasks through the same cross-thread schedule
//! that wakes the host. On `wasm32-unknown-emscripten` without threads the
//! JavaScript host is the reactor: its callbacks stand in for that thread,
//! and the driver's turn runs inside each drive ([`reactor`]). Either way
//! nothing about the platform's reactor or timers is exposed; the host's
//! whole contract is the waker and `drive`.
//!
//! A *hosted* event loop is one whose host is the ambient JavaScript loop:
//! it supplies its own waker, which schedules the drive on that loop, so the
//! program spawns and returns to the host.

#[cfg_attr(
    all(target_os = "emscripten", not(target_feature = "atomics")),
    path = "event_loop/emscripten.rs"
)]
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
/// carries it, and panics rather than wait.
///
/// Like `LocalRuntime` the event loop is `!Send`: the host drives it on the
/// thread that built it. [`Handle::block_on`] from another thread works as
/// on a native runtime, since the driver is parked on a thread of its own.
///
/// Dropping the `LocalEventLoop` shuts the runtime down as dropping a
/// `LocalRuntime` does. The waker may be woken once more during the drop.
///
/// On `wasm32-unknown-emscripten` without threads, a *hosted* event loop
/// (`Builder::build_hosted_local_event_loop`) needs no waker: the JavaScript
/// host loop drives it, so the program spawns and returns to the host. Such
/// a loop keeps the Emscripten runtime alive while it has tasks. A
/// [`Handle::block_on`] suspended through JSPI on the same thread defers
/// hosted drives until it returns, and timers it registers itself are armed
/// only by the next drive.
///
/// [`Builder::build_local_event_loop`]: crate::runtime::Builder::build_local_event_loop
/// [`Handle::block_on`]: crate::runtime::Handle::block_on
#[derive(Debug)]
pub struct LocalEventLoop {
    shared: Rc<Shared>,
}

impl LocalEventLoop {
    /// `None` for the waker means the runtime's own host on this target.
    pub(crate) fn new(runtime: LocalRuntime, waker: Option<Waker>) -> io::Result<LocalEventLoop> {
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
    /// become ready, without ever waiting. The wait belongs to the host event
    /// loop: where a [`Runtime::block_on`] would park the thread, nothing
    /// could wake this future from here, so the future is dropped and this
    /// panics. Tasks the future spawned continue from later drives.
    ///
    /// That includes waiting on I/O that is already readable, since readiness
    /// arrives through the driver, not through a turn inside `block_on`.
    ///
    /// # Panics
    ///
    /// Panics if the future is still pending once no ready work remains, if
    /// called from within a runtime, or on a thread other than the one that
    /// built the event loop, or if a task panicked and the runtime is
    /// configured to [shut down on unhandled panics].
    ///
    /// [`Runtime::block_on`]: crate::runtime::Runtime::block_on
    /// [shut down on unhandled panics]: crate::runtime::Builder::unhandled_panic
    #[track_caller]
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.shared.block_on(future)
    }
}

#[derive(Debug)]
pub(super) struct Shared {
    /// Its shutdown needs the driver back, which `Drop` restores before the
    /// fields drop.
    runtime: LocalRuntime,
    handle: Handle,
    reactor: OnceLock<Reactor>,
    tid: ThreadId,
}

impl Shared {
    fn new(runtime: LocalRuntime, waker: Option<Waker>) -> io::Result<Rc<Shared>> {
        let handle = runtime.handle().clone();
        let shared = Rc::new(Shared {
            runtime,
            handle,
            reactor: OnceLock::new(),
            tid: std::thread::current().id(),
        });
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        let waker = waker.unwrap_or_else(|| reactor::hosted_waker(Rc::downgrade(&shared)));
        #[cfg(not(all(target_os = "emscripten", not(target_feature = "atomics"))))]
        let waker = waker.expect("no ambient host loop on this target");
        shared.handle.inner.driver().set_host(waker);
        let reactor = Reactor::start(&shared)?;
        let _ = shared.reactor.set(reactor);
        shared.reactor.get().expect("just set").attach()?;
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

    pub(super) fn drive(&self) {
        self.check_thread();
        let handle = self.handle.inner.as_current_thread();
        let busy = context::enter_runtime(&self.handle.inner, false, |_| {
            self.runtime.current_thread().drive_batch(handle)
        });
        self.after_turn(busy);
    }

    #[track_caller]
    fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.check_thread();
        let handle = self.handle.inner.as_current_thread();
        let (ret, busy) = context::enter_runtime(&self.handle.inner, false, |_| {
            self.runtime.current_thread().block_on_ready(handle, future)
        });
        // The dropped future's timers and registrations are gone; settle the
        // host's side before reporting.
        self.after_turn(busy);
        match ret {
            Some(out) => out,
            None => panic!(
                "`LocalEventLoop::block_on` cannot wait: the future is still pending \
                 with no ready work, and its wait belongs to the host event loop, so \
                 nothing could wake it from here"
            ),
        }
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
