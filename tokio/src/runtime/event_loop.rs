//! A `current_thread` runtime whose wait is a host event loop.
//!
//! The scheduler drives to a fixed point, then waits. A native runtime waits
//! by parking the thread in the reactor; an event loop instead returns to the
//! host, which watches the reactor's file descriptor and calls
//! [`EventLoop::drive`] when it is readable. Everything the runtime can wait
//! for is readiness on that one descriptor:
//!
//! * I/O: the sockets registered with the reactor.
//! * Timers: the soonest timer deadline is armed on a timer descriptor in the
//!   reactor's set after each drive ([`deadline`]).
//! * Wakes from outside a drive (a spawn, a task woken from another thread):
//!   the reactor's waker, written by the driver's unpark as on a native
//!   runtime.
//! * More ready work after a batch: the runtime writes its own waker, so the
//!   host gets a turn between batches.
//!
//! The runtime owns the timer, so arming, re-arming and cancelling never
//! involve the host, and dropping the [`EventLoop`] drops it with everything
//! else, with native `Runtime::drop` semantics.

mod deadline;

use crate::loom::sync::Mutex;
use crate::runtime::local_runtime::LocalRuntime;
use crate::runtime::{context, Handle, Runtime};
use crate::task::JoinHandle;
use crate::util::trace::SpawnMeta;

use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;
use std::thread::ThreadId;

/// A [`Runtime`] driven by a host event loop instead of by parking a thread.
///
/// Built with [`Builder::build_event_loop`]. The host watches the reactor's
/// file descriptor ([`AsFd`]) for readability and calls
/// [`drive`](Self::drive) when it is; the runtime does the rest, including
/// arming a timer descriptor in the same set for its soonest deadline. Tasks
/// are submitted with [`spawn`](Self::spawn) and run in batches from those
/// drives, so there is no `block_on`: a result is received by awaiting the
/// [`JoinHandle`] from another task, or through any completion the embedder
/// chooses.
///
/// The runtime is single-threaded (`current_thread`), but like `Runtime` it
/// is `Send + Sync`: it may be driven from any thread, one at a time.
///
/// Dropping the `EventLoop` shuts the runtime down as dropping a `Runtime`
/// does. The descriptor is closed with it, so the host must stop watching
/// it first.
///
/// [`Builder::build_event_loop`]: crate::runtime::Builder::build_event_loop
#[derive(Debug)]
pub struct EventLoop {
    state: Arc<EventLoopState>,
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
    state: Arc<EventLoopState>,
    _not_send: PhantomData<*mut u8>,
}

impl EventLoop {
    pub(crate) fn new(runtime: Runtime) -> io::Result<EventLoop> {
        let handle = runtime.handle().clone();
        Ok(EventLoop {
            state: EventLoopState::new(Inner::Runtime(runtime), handle, None)?,
        })
    }

    /// Spawns a future onto the runtime. It is queued and the reactor's
    /// descriptor becomes readable; it never runs before `spawn` returns.
    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.state.handle.spawn(future)
    }

    /// Returns a handle to the runtime.
    pub fn handle(&self) -> &Handle {
        &self.state.handle
    }

    /// Runs one batch: ready tasks, then a non-blocking reactor turn (I/O
    /// readiness, due timers, deferred wakers). If ready work remains
    /// afterwards the descriptor is readable again at once; otherwise the
    /// timer descriptor is armed for the soonest deadline, if any.
    ///
    /// Call this whenever the descriptor is readable. Calling it at other
    /// times is harmless.
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime.
    pub fn drive(&self) {
        self.state.drive();
    }
}

impl AsFd for EventLoop {
    /// The reactor's descriptor: readable when a drive is needed.
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.state.fd()
    }
}

impl LocalEventLoop {
    pub(crate) fn new(runtime: LocalRuntime) -> io::Result<LocalEventLoop> {
        let handle = runtime.handle().clone();
        let tid = std::thread::current().id();
        Ok(LocalEventLoop {
            state: EventLoopState::new(Inner::Local(runtime), handle, Some(tid))?,
            _not_send: PhantomData,
        })
    }

    /// Spawns a future onto the runtime. It is queued and the reactor's
    /// descriptor becomes readable; it never runs before `spawn_local`
    /// returns.
    #[track_caller]
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let meta = SpawnMeta::new_unnamed(std::mem::size_of::<F>());
        // SAFETY: `LocalEventLoop` is `!Send`, so this is the thread that
        // built the runtime, and `drive` polls only on that thread.
        unsafe { self.state.handle.spawn_local_named(future, meta) }
    }

    /// Returns a handle to the runtime.
    pub fn handle(&self) -> &Handle {
        &self.state.handle
    }

    /// Runs one batch; see [`EventLoop::drive`].
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime, or on a thread other than the
    /// one that built the event loop.
    pub fn drive(&self) {
        self.state.drive();
    }
}

impl AsFd for LocalEventLoop {
    /// The reactor's descriptor: readable when a drive is needed.
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.state.fd()
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        self.state.inner.lock().take();
    }
}

impl Drop for LocalEventLoop {
    fn drop(&mut self) {
        self.state.inner.lock().take();
    }
}

#[derive(Debug)]
enum Inner {
    Runtime(Runtime),
    Local(LocalRuntime),
}

pub(crate) struct EventLoopState {
    /// Taken on drop of the owning event loop, so the runtime is dropped
    /// there.
    inner: Mutex<Option<Inner>>,
    handle: Handle,
    deadline: deadline::Deadline,
    local_tid: Option<ThreadId>,
}

// SAFETY: `Inner::Local` is `!Send` only by marker. It is polled by `drive`,
// which checks the owning thread first, and dropped by `LocalEventLoop`,
// which is itself `!Send`; every other field is `Send + Sync`.
unsafe impl Send for EventLoopState {}
unsafe impl Sync for EventLoopState {}

impl std::fmt::Debug for EventLoopState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventLoopState")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

impl EventLoopState {
    fn new(
        inner: Inner,
        handle: Handle,
        local_tid: Option<ThreadId>,
    ) -> io::Result<Arc<EventLoopState>> {
        let deadline = deadline::Deadline::new(&handle)?;
        Ok(Arc::new(EventLoopState {
            inner: Mutex::new(Some(inner)),
            handle,
            deadline,
            local_tid,
        }))
    }

    fn fd(&self) -> BorrowedFd<'_> {
        use std::os::fd::AsRawFd;
        let raw = self.handle.inner.driver().io().registry_raw_fd();
        // SAFETY: the reactor lives as long as the handle borrowed here.
        unsafe { BorrowedFd::borrow_raw(raw.as_raw_fd()) }
    }

    /// One batch (`event_interval` tasks, then the non-blocking reactor
    /// turn), then the continuation: the waker if work remains, so the host
    /// gets a turn between batches, and the timer for the soonest deadline.
    pub(crate) fn drive(self: &Arc<Self>) {
        if let Some(tid) = self.local_tid {
            assert_eq!(
                std::thread::current().id(),
                tid,
                "a `LocalEventLoop` must be driven on the thread that built it"
            );
        }
        let handle = self.handle.inner.as_current_thread();

        let busy = context::enter_runtime(&self.handle.inner, false, |_| {
            let inner = self.inner.lock();
            match &*inner {
                Some(Inner::Runtime(rt)) => rt.current_thread().drive_batch(handle),
                Some(Inner::Local(rt)) => rt.current_thread().drive_batch(handle),
                None => false,
            }
        });

        let next = next_deadline(handle);
        // A deadline already due is a wake, not a timer: a zero `it_value`
        // would disarm.
        let due = matches!(next, Some(d) if d.is_zero());
        if busy || due {
            handle.driver.unpark();
        }
        self.deadline.arm(next.filter(|_| !due));
    }
}

#[cfg(feature = "time")]
fn next_deadline(
    handle: &crate::runtime::scheduler::current_thread::Handle,
) -> Option<std::time::Duration> {
    let time = handle.driver.time.as_ref()?;
    let tick = time.next_expiration_tick()?;
    let now = time.time_source().now(&handle.driver.clock);
    Some(
        time.time_source()
            .tick_to_duration(tick.saturating_sub(now)),
    )
}

#[cfg(not(feature = "time"))]
fn next_deadline(
    _handle: &crate::runtime::scheduler::current_thread::Handle,
) -> Option<std::time::Duration> {
    None
}
