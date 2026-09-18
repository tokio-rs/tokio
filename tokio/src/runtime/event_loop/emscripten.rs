//! The host loop's side of the contract on `wasm32-unknown-emscripten`
//! without threads: no thread can park in the driver, and none needs to,
//! because the JavaScript host is itself the reactor. Its readiness callback
//! on the reactor's epoll set and a host timer for the soonest deadline wake
//! the host waker; the driver's turn runs synchronously inside each drive.
//!
//! A hosted event loop's waker schedules the drive on the host loop itself,
//! so the program registers nothing: it spawns and returns to the host.

use super::Shared;
use crate::runtime::driver::Driver;
use crate::runtime::Handle;

use std::cell::Cell;
use std::ffi::c_void;
use std::io;
use std::rc::Weak;
use std::sync::Arc;
use std::task::{Wake, Waker};
use std::time::Duration;

type Callback = unsafe extern "C-unwind" fn(*mut c_void);

extern "C" {
    /// Runs `cb(user_data)` after `msecs` on the host loop, holding the
    /// Emscripten runtime alive until it fires or is cleared.
    fn emscripten_set_timeout(cb: Callback, msecs: f64, user_data: *mut c_void) -> i32;
    fn emscripten_clear_timeout(id: i32);
    /// Runs `cb(user_data)` on the next host loop turn (`setImmediate`),
    /// holding the runtime alive until then.
    fn emscripten_set_immediate(cb: Callback, user_data: *mut c_void) -> i32;
    fn emscripten_runtime_keepalive_push();
    fn emscripten_runtime_keepalive_pop();
}

#[cfg(feature = "net")]
extern "C" {
    /// Persistent readiness listener on an epoll fd: `cb(user_data)` runs on
    /// the host loop whenever the set has uncollected ready events. Holds
    /// nothing itself.
    fn emscripten_epoll_add_listener(epfd: i32, cb: Callback, user_data: *mut c_void) -> i32;
    fn emscripten_epoll_remove_listener(epfd: i32, cb: Callback) -> i32;
}

/// The host loop's callbacks into the runtime, and what they hold.
#[derive(Debug)]
pub(super) struct Reactor {
    /// The armed deadline's timeout id.
    deadline: Cell<Option<i32>>,
    /// The event loop's hold on the Emscripten runtime: the process lives
    /// while the loop has tasks, as a native one lives while `block_on` runs.
    held: Cell<bool>,
    #[cfg(feature = "net")]
    epfd: Option<i32>,
}

impl Reactor {
    pub(super) fn start(shared: &Shared) -> io::Result<Reactor> {
        let handle = &shared.handle;
        #[cfg(feature = "net")]
        let epfd = handle
            .inner
            .driver()
            .io
            .as_ref()
            .map(|io| io.registry_raw_fd());
        #[cfg(feature = "net")]
        if let Some(epfd) = epfd {
            // SAFETY: the reactor's live epoll fd; `user_data` is the driver
            // handle, which outlives the listener (removed in `stop`).
            let rc = unsafe { emscripten_epoll_add_listener(epfd, wake, driver_ptr(handle)) };
            if rc != 0 {
                return Err(io::Error::from_raw_os_error(rc));
            }
        }
        #[cfg(not(feature = "net"))]
        let _ = handle;
        Ok(Reactor {
            deadline: Cell::new(None),
            held: Cell::new(false),
            #[cfg(feature = "net")]
            epfd,
        })
    }

    /// Arm the host timer for the soonest deadline, and hold the runtime
    /// while the event loop has tasks.
    pub(super) fn after_turn(&self, handle: &Handle) {
        self.arm(handle, next_deadline(handle));
        self.hold(handle.inner.num_alive_tasks() > 0);
    }

    pub(super) fn stop(self, handle: &Handle) -> Option<Driver> {
        self.arm(handle, None);
        self.hold(false);
        #[cfg(feature = "net")]
        if let Some(epfd) = self.epfd {
            // SAFETY: the listener added in `start`, on the still-open fd.
            unsafe { emscripten_epoll_remove_listener(epfd, wake) };
        }
        None
    }

    /// The runtime owns the timer: a changed or dropped deadline never fires
    /// stale.
    fn arm(&self, handle: &Handle, after: Option<Duration>) {
        if let Some(id) = self.deadline.take() {
            // SAFETY: a pending timeout armed below.
            unsafe { emscripten_clear_timeout(id) };
        }
        if let Some(after) = after {
            let ms = after.as_secs_f64() * 1000.0;
            // SAFETY: `user_data` is the driver handle, cleared in `stop`
            // before the runtime drops.
            self.deadline.set(Some(unsafe {
                emscripten_set_timeout(wake, ms, driver_ptr(handle))
            }));
        }
    }

    fn hold(&self, alive: bool) {
        if self.held.replace(alive) != alive {
            // SAFETY: Emscripten runtime calls; every push is paired with one
            // pop here.
            unsafe {
                if alive {
                    emscripten_runtime_keepalive_push();
                } else {
                    emscripten_runtime_keepalive_pop();
                }
            }
        }
    }
}

fn driver_ptr(handle: &Handle) -> *mut c_void {
    handle.inner.driver() as *const crate::runtime::driver::Handle as *mut c_void
}

/// Readiness or a deadline: the host should drive.
unsafe extern "C-unwind" fn wake(user_data: *mut c_void) {
    // SAFETY: `user_data` is the driver handle armed by `Reactor`, which
    // unregisters both callbacks before the runtime drops.
    let driver = unsafe { &*(user_data as *const crate::runtime::driver::Handle) };
    driver.wake_host();
}

#[cfg(feature = "time")]
fn next_deadline(handle: &Handle) -> Option<Duration> {
    let driver = handle.inner.driver();
    let time = driver.time.as_ref()?;
    let tick = time.next_expiration_tick()?;
    let now = time.time_source().now(&driver.clock);
    Some(
        time.time_source()
            .tick_to_duration(tick.saturating_sub(now)),
    )
}

#[cfg(not(feature = "time"))]
fn next_deadline(_handle: &Handle) -> Option<Duration> {
    None
}

/// The waker of a hosted event loop: a wake schedules a drive on the host
/// loop. An immediate rather than a timeout, which the host clamps to a
/// millisecond, and never a microtask, which would run before host timers
/// and let a self-waking task starve them. Pending drives coalesce.
struct Hosted {
    target: Weak<Shared>,
    scheduled: Cell<bool>,
}

// SAFETY: `Waker` requires the bounds, and this module is compiled only for
// `not(target_feature = "atomics")`, where the program has a single thread.
unsafe impl Send for Hosted {}
unsafe impl Sync for Hosted {}

impl Wake for Hosted {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if self.scheduled.replace(true) {
            return;
        }
        self.schedule();
    }
}

impl Hosted {
    fn schedule(self: &Arc<Self>) {
        // SAFETY: the `Arc` is reclaimed in `drive`, which the host calls
        // exactly once per immediate.
        unsafe { emscripten_set_immediate(drive, Arc::into_raw(self.clone()) as *mut c_void) };
    }
}

unsafe extern "C-unwind" fn drive(user_data: *mut c_void) {
    // SAFETY: the `Arc<Hosted>` leaked in `schedule`.
    let hosted = unsafe { Arc::from_raw(user_data as *const Hosted) };
    // A runtime entered on this thread, such as a `block_on` suspended
    // through JSPI, owns it until it returns: try again next turn.
    if crate::runtime::context::runtime_entered() {
        hosted.schedule();
        return;
    }
    // Cleared first: a busy drive re-wakes for the next batch.
    hosted.scheduled.set(false);
    let Some(target) = hosted.target.upgrade() else {
        return;
    };
    // No Rust frame is above this callback to catch a panic (a task panic
    // under `UnhandledPanic::ShutdownRuntime`); it would unwind into the
    // host's JavaScript. Abort as an uncaught panic on `main` would.
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| target.drive())).is_err() {
        std::process::abort();
    }
}

pub(super) fn hosted_waker(target: Weak<Shared>) -> Waker {
    Waker::from(Arc::new(Hosted {
        target,
        scheduled: Cell::new(false),
    }))
}
