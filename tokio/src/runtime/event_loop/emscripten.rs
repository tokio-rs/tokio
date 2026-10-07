//! The JavaScript host loop as the host, on `wasm32-unknown-emscripten`
//! without threads. A hosted event loop is the adapter any host writes, in
//! Emscripten's terms: a persistent readiness listener on the reactor's
//! descriptor (`emscripten_epoll_add_listener`), a host timer armed for
//! `next_timeout` after every drive, and an immediate when a drive leaves
//! ready work, so a busy loop yields a host turn between batches. Each
//! callback drives the loop.
//!
//! The loop holds the Emscripten runtime alive while it has tasks, as a
//! native one lives while `block_on` runs, so `main` may return with work in
//! flight.

use super::Shared;

use std::cell::Cell;
use std::ffi::c_void;
use std::io;
use std::os::fd::RawFd;
use std::rc::{Rc, Weak};

type Callback = unsafe extern "C-unwind" fn(*mut c_void);

extern "C" {
    /// Runs `cb(user_data)` after `msecs` on the host loop, holding the
    /// Emscripten runtime alive until it fires or is cleared.
    fn emscripten_set_timeout(cb: Callback, msecs: f64, user_data: *mut c_void) -> i32;
    fn emscripten_clear_timeout(id: i32);
    /// Runs `cb(user_data)` on the next host loop turn (`setImmediate`),
    /// holding the runtime alive until then.
    fn emscripten_set_immediate(cb: Callback, user_data: *mut c_void) -> i32;
    fn emscripten_clear_immediate(id: i32);
    fn emscripten_runtime_keepalive_push();
    fn emscripten_runtime_keepalive_pop();
    /// Persistent readiness listener on an epoll fd: `cb(user_data)` runs on
    /// the host loop whenever the set has uncollected ready events. Holds
    /// nothing itself.
    fn emscripten_epoll_add_listener(epfd: i32, cb: Callback, user_data: *mut c_void) -> i32;
    fn emscripten_epoll_remove_listener(epfd: i32, cb: Callback, user_data: *mut c_void) -> i32;
}

/// The host loop's callbacks into the runtime, and what they hold. Every
/// callback receives the `Hosted` as its argument; it lives in the event
/// loop's shared state at a fixed address, and detaches on drop before
/// the descriptor closes.
#[derive(Debug)]
pub(super) struct Hosted {
    shared: Weak<Shared>,
    epfd: RawFd,
    listening: Cell<bool>,
    /// The armed deadline's timeout id.
    deadline: Cell<Option<i32>>,
    /// The follow-up drive for ready work a batch left.
    immediate: Cell<Option<i32>>,
    /// The event loop's hold on the Emscripten runtime.
    held: Cell<bool>,
}

impl Hosted {
    /// The callbacks take this value's address, so it registers only once
    /// in place: see [`attach`](Self::attach).
    pub(super) fn new(shared: &Rc<Shared>) -> Hosted {
        Hosted {
            shared: Rc::downgrade(shared),
            epfd: shared.reactor,
            listening: Cell::new(false),
            deadline: Cell::new(None),
            immediate: Cell::new(None),
            held: Cell::new(false),
        }
    }

    /// Registers with the host loop, from the `Hosted`'s final address.
    pub(super) fn attach(&self) -> io::Result<()> {
        self.listen(true)
    }

    pub(super) fn after_drive(&self, shared: &Shared, busy: bool) {
        self.arm(shared);
        if busy {
            self.schedule();
        }
        self.hold(shared.handle.inner.num_alive_tasks() > 0);
    }

    /// Adds or removes the readiness listener. It fires every host turn
    /// while the set has uncollected events, so it comes off while a drive
    /// cannot run (see [`ready`]).
    fn listen(&self, on: bool) -> io::Result<()> {
        if self.listening.get() == on {
            return Ok(());
        }
        let this = self as *const Hosted as *mut c_void;
        // SAFETY: the reactor's live epoll fd; `user_data` is this `Hosted`,
        // which removes the listener in `drop` before the loop drops.
        let rc = unsafe {
            if on {
                emscripten_epoll_add_listener(self.epfd, ready, this)
            } else {
                emscripten_epoll_remove_listener(self.epfd, ready, this)
            }
        };
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        self.listening.set(on);
        Ok(())
    }

    /// Arms the host timer for the soonest deadline. The runtime owns the
    /// timer: a changed or dropped deadline never fires stale.
    fn arm(&self, shared: &Shared) {
        if let Some(id) = self.deadline.take() {
            // SAFETY: a pending timeout armed below.
            unsafe { emscripten_clear_timeout(id) };
        }
        if let Some(after) = shared.next_timeout() {
            let ms = after.as_secs_f64() * 1000.0;
            let this = self as *const Hosted as *mut c_void;
            // SAFETY: `user_data` is this `Hosted`, which clears the timeout
            // in `drop` before the loop drops.
            self.deadline
                .set(Some(unsafe { emscripten_set_timeout(deadline, ms, this) }));
        }
    }

    /// Schedules a drive on the next host turn, unless one is pending.
    fn schedule(&self) {
        if self.immediate.get().is_some() {
            return;
        }
        let this = self as *const Hosted as *mut c_void;
        // SAFETY: `user_data` is this `Hosted`, which clears the immediate in
        // `drop` before the loop drops.
        self.immediate
            .set(Some(unsafe { emscripten_set_immediate(immediate, this) }));
    }

    fn hold(&self, alive: bool) {
        if self.held.replace(alive) != alive {
            // SAFETY: Emscripten runtime calls; every push is paired with one
            // pop here or in `drop`.
            unsafe {
                if alive {
                    emscripten_runtime_keepalive_push();
                } else {
                    emscripten_runtime_keepalive_pop();
                }
            }
        }
    }

    /// Drives the loop from a host callback.
    ///
    /// While a runtime is entered on this thread (a `block_on` suspended
    /// through JSPI) no drive can run; the callback is deferred to that
    /// runtime's exit, and the readiness listener comes off until then
    /// rather than firing every host turn.
    fn drive(&self) {
        let shared = self.shared.clone();
        let deferred = crate::runtime::jspi::defer_after_runtime_exit(move || {
            if let Some(shared) = shared.upgrade() {
                if let Some(hosted) = shared.hosted.get() {
                    let _ = hosted.listen(true);
                    hosted.schedule();
                }
            }
        });
        if deferred {
            let _ = self.listen(false);
            return;
        }
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        // No Rust frame is above the host's callback to catch a panic (a
        // task panic under `UnhandledPanic::ShutdownRuntime`); it would
        // unwind into the host's JavaScript. Abort as an uncaught panic on
        // `main` would.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shared.drive())).is_err() {
            std::process::abort();
        }
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        if let Some(id) = self.deadline.take() {
            // SAFETY: a pending timeout armed in `arm`.
            unsafe { emscripten_clear_timeout(id) };
        }
        if let Some(id) = self.immediate.take() {
            // SAFETY: a pending immediate armed in `schedule`.
            unsafe { emscripten_clear_immediate(id) };
        }
        let _ = self.listen(false);
        self.hold(false);
    }
}

/// The readiness listener: the set has uncollected events.
unsafe extern "C-unwind" fn ready(user_data: *mut c_void) {
    // SAFETY: `user_data` is the `Hosted` that registered this callback,
    // alive until `drop` unregisters it.
    let hosted = unsafe { &*(user_data as *const Hosted) };
    hosted.drive();
}

/// The deadline timer fired.
unsafe extern "C-unwind" fn deadline(user_data: *mut c_void) {
    // SAFETY: as `ready`; the timeout is cleared in `drop`.
    let hosted = unsafe { &*(user_data as *const Hosted) };
    hosted.deadline.set(None);
    hosted.drive();
}

/// The follow-up for a busy batch.
unsafe extern "C-unwind" fn immediate(user_data: *mut c_void) {
    // SAFETY: as `ready`; the immediate is cleared in `drop`.
    let hosted = unsafe { &*(user_data as *const Hosted) };
    hosted.immediate.set(None);
    hosted.drive();
}
