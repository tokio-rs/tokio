//! A `current_thread` runtime whose wait belongs to a host event loop.
//!
//! The scheduler drives to a fixed point, then waits. A native runtime waits
//! by parking its thread in the driver; an event loop never waits. Instead it
//! exposes what the host needs to wait on its behalf: the reactor's own
//! handle (an `epoll` or `kqueue` descriptor, or an I/O completion port),
//! signaled whenever the driver has pending events, and the time until the
//! soonest timer. The host adds the handle to its own wait set, arms its own
//! timer, and calls [`LocalEventLoop::drive`] from either, which takes one
//! zero-timeout turn of the driver and runs one batch of tasks.
//!
//! Everything that would unpark a native runtime's thread signals the
//! handle: I/O readiness and signals directly, and spawns, wakes from other
//! threads and nearer timers through the driver's waker, which is registered
//! in the same set. No thread is owned, and nothing is polled.
//!
//! On `wasm32-unknown-emscripten` without threads the JavaScript host loop
//! can be the host itself: a *hosted* event loop ([`emscripten`]) registers
//! the descriptor and the timeout with it, so the program spawns and returns
//! to the host.

#[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
mod emscripten;

use crate::runtime::local_runtime::LocalRuntime;
use crate::runtime::{context, Handle};
use crate::task::JoinHandle;
use crate::util::trace::SpawnMeta;

use std::future::Future;
use std::io;
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, RawHandle};
use std::rc::Rc;
use std::thread::ThreadId;
use std::time::Duration;

#[cfg(unix)]
type Reactor = RawFd;
#[cfg(windows)]
type Reactor = RawHandle;

/// A [`LocalRuntime`] driven by a host event loop instead of by parking a
/// thread.
///
/// Built with [`Builder::build_local_event_loop`]. The host waits on the
/// event loop's reactor handle, arms a timer for
/// [`next_timeout`](Self::next_timeout), and calls [`drive`](Self::drive)
/// when either fires. This is the shape of libuv's `uv_backend_fd` /
/// `uv_backend_timeout` / `uv_run(UV_RUN_NOWAIT)`.
///
/// On Unix the handle is the reactor's `epoll` or `kqueue` descriptor
/// (`AsRawFd`), readable while it has pending events: a libuv host would
/// `uv_poll_start` it and `uv_timer_start` the timeout; a host with its own
/// `epoll`/`kqueue` set adds it to that set. On Windows it is the reactor's
/// I/O completion port (`AsRawHandle`), a waitable object that is signaled
/// while it has pending packets and not reset by waiting: the host puts it in
/// its `WaitForMultipleObjects` or `MsgWaitForMultipleObjectsEx` set. A
/// completion port cannot be the target of a wait completion packet or of
/// `RegisterWaitForSingleObject`, and must not be dequeued by anything but
/// `drive`.
///
/// Tasks are submitted with [`spawn_local`](Self::spawn_local), or from any
/// thread through the [`Handle`], and run in batches from those drives.
///
/// Like `LocalRuntime` the event loop is `!Send`: the host drives it on the
/// thread that built it. [`Handle::block_on`] from another thread works as
/// on a native runtime.
///
/// Dropping the `LocalEventLoop` shuts the runtime down as dropping a
/// `LocalRuntime` does; the handle is closed with it, so the host must
/// remove it from its set first.
///
/// On `wasm32-unknown-emscripten` without threads, a *hosted* event loop
/// (`Builder::build_hosted_local_event_loop`) is driven by the JavaScript
/// host loop itself: the program spawns and returns to the host. Such a loop
/// keeps the Emscripten runtime alive while it has tasks. A
/// [`Handle::block_on`] suspended through JSPI on the same thread defers
/// hosted drives until it returns.
///
/// # Example
///
/// A minimal host loop over `poll(2)`:
///
/// ```no_run
/// # #[cfg(unix)]
/// # fn main() -> std::io::Result<()> {
/// use std::os::fd::AsRawFd;
/// use tokio::runtime::Builder;
///
/// let el = Builder::new_current_thread()
///     .enable_all()
///     .build_local_event_loop(Default::default())?;
///
/// let done = el.spawn_local(async {
///     tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// });
///
/// while !done.is_finished() {
///     let mut fd = libc::pollfd { fd: el.as_raw_fd(), events: libc::POLLIN, revents: 0 };
///     let timeout = el.next_timeout().map_or(-1, |t| t.as_millis() as i32);
///     unsafe { libc::poll(&mut fd, 1, timeout) };
///     while el.drive() {}
/// }
/// # Ok(()) }
/// # #[cfg(not(unix))]
/// # fn main() {}
/// ```
///
/// [`Builder::build_local_event_loop`]: crate::runtime::Builder::build_local_event_loop
/// [`Handle::block_on`]: crate::runtime::Handle::block_on
#[derive(Debug)]
pub struct LocalEventLoop {
    shared: Rc<Shared>,
}

#[derive(Debug)]
pub(super) struct Shared {
    /// Detaches from the host before the runtime, and its descriptor,
    /// drop.
    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
    hosted: std::sync::OnceLock<emscripten::Hosted>,
    runtime: LocalRuntime,
    handle: Handle,
    reactor: Reactor,
    tid: ThreadId,
}

impl LocalEventLoop {
    /// `hosted` attaches the loop to the ambient JavaScript host loop, where
    /// there is one.
    pub(crate) fn new(runtime: LocalRuntime, hosted: bool) -> io::Result<LocalEventLoop> {
        let handle = runtime.handle().clone();
        let Some(io) = handle.inner.driver().io.as_ref() else {
            return Err(io::Error::other(
                "a `LocalEventLoop` needs the I/O driver; call `enable_io` on the runtime builder",
            ));
        };
        #[cfg(unix)]
        let reactor = io.registry_raw_fd();
        #[cfg(windows)]
        let reactor = io.registry_raw_handle();
        let shared = Rc::new(Shared {
            #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
            hosted: std::sync::OnceLock::new(),
            runtime,
            handle,
            reactor,
            tid: std::thread::current().id(),
        });
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        if hosted {
            let _ = shared.hosted.set(emscripten::Hosted::new(&shared));
            shared.hosted.get().expect("just set").attach()?;
        }
        #[cfg(not(all(target_os = "emscripten", not(target_feature = "atomics"))))]
        assert!(!hosted, "no ambient host loop on this target");
        Ok(LocalEventLoop { shared })
    }

    /// Spawns a future onto the runtime. It is queued, and the reactor
    /// handle signaled; it never runs before `spawn_local` returns.
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

    /// Time until the soonest timer, if any is registered. `Some(ZERO)` means
    /// a timer is due.
    ///
    /// Re-read this after every `drive`: the batch may have registered a
    /// nearer timer. A nearer timer registered from another thread, or by a
    /// `Handle::block_on` elsewhere, also signals the reactor handle, so a
    /// host that re-reads after every wake stays current.
    pub fn next_timeout(&self) -> Option<Duration> {
        self.shared.next_timeout()
    }

    /// Takes one turn of the driver without waiting (I/O readiness, due
    /// timers, signals), then runs one batch of ready tasks (at most
    /// `event_interval`). Returns whether ready work remains, in which case
    /// the host should call again before it sleeps.
    ///
    /// Call this whenever the reactor handle is signaled or `next_timeout`
    /// has elapsed. A signal does not guarantee work (a wake for a timer that
    /// was then cancelled, or a cancelled registration), so a drive that does
    /// nothing is normal, and calling it without a wake is harmless.
    ///
    /// # Panics
    ///
    /// Panics if called from within a runtime, or on a thread other than the
    /// one that built the event loop, or if a task panicked and the runtime
    /// is configured to [shut down on unhandled panics].
    ///
    /// [shut down on unhandled panics]: crate::runtime::Builder::unhandled_panic
    pub fn drive(&self) -> bool {
        self.shared.drive()
    }
}

impl Shared {
    fn next_timeout(&self) -> Option<Duration> {
        let driver = self.handle.inner.driver();
        #[cfg(feature = "time")]
        {
            driver.time.as_ref()?.next_timeout(&driver.clock)
        }
        #[cfg(not(feature = "time"))]
        {
            let _ = driver;
            None
        }
    }

    fn drive(&self) -> bool {
        assert_eq!(
            std::thread::current().id(),
            self.tid,
            "a `LocalEventLoop` must be driven on the thread that built it"
        );
        let handle = self.handle.inner.as_current_thread();
        let drive = || {
            context::enter_runtime(&self.handle.inner, false, |_| {
                self.runtime.current_thread().drive(handle)
            })
        };
        #[cfg(not(all(target_os = "emscripten", not(target_feature = "atomics"))))]
        let busy = drive();
        // The driver's turn must neither yield to nor suspend on the host
        // loop: a host callback already has the turn.
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        let busy = crate::runtime::jspi::host_turn(drive);
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        if let Some(hosted) = self.hosted.get() {
            hosted.after_drive(self, busy);
        }
        busy
    }
}

#[cfg(unix)]
impl AsRawFd for LocalEventLoop {
    /// The reactor's descriptor: readable while the driver has pending
    /// events. Level-triggered; a drive consumes them.
    fn as_raw_fd(&self) -> RawFd {
        self.shared.reactor
    }
}

#[cfg(unix)]
impl AsFd for LocalEventLoop {
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the reactor owns the descriptor for the lifetime of the
        // runtime, which `self` holds.
        unsafe { BorrowedFd::borrow_raw(self.shared.reactor) }
    }
}

#[cfg(windows)]
impl AsRawHandle for LocalEventLoop {
    /// The reactor's completion port: signaled while the driver has pending
    /// packets, and not reset by waiting. A drive dequeues them.
    fn as_raw_handle(&self) -> RawHandle {
        self.shared.reactor
    }
}

#[cfg(windows)]
impl AsHandle for LocalEventLoop {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        // SAFETY: the reactor owns the port for the lifetime of the runtime,
        // which `self` holds.
        unsafe { BorrowedHandle::borrow_raw(self.shared.reactor) }
    }
}
