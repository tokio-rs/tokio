//! The JavaScript host loop as the host, on `wasm32-unknown-emscripten`
//! without threads. A hosted event loop is the adapter any host writes, in
//! Emscripten's terms: a persistent readiness listener on `event_fd`
//! (`emscripten_epoll_listener_add`), a host timer armed for `next_sleep`
//! after every tick, and an immediate when `next_sleep` is zero, so a busy
//! loop yields a host turn between batches. Each callback ticks the loop.
//!
//! The loop holds the Emscripten runtime alive while it has tasks, as a
//! native one lives while `block_on` runs, so `main` may return with work in
//! flight.

use super::Core;
use crate::runtime::local_runtime::LocalRuntime;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;

type Callback = extern "C" fn(*mut c_void);
type EpollCallback = extern "C" fn(i32, *mut c_void);

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
    /// Persistent readiness listener on an epoll fd: `cb(epfd, user_data)`
    /// runs on the host loop whenever the set has uncollected ready events.
    /// Holds nothing itself.
    fn emscripten_epoll_listener_add(epfd: i32, cb: EpollCallback, user_data: *mut c_void) -> i32;
    fn emscripten_epoll_listener_remove(
        epfd: i32,
        cb: EpollCallback,
        user_data: *mut c_void,
    ) -> i32;
}

/// The event loop's hold on the Emscripten runtime. The readiness listener
/// holds nothing, so the loop holds it itself while it has tasks: taken as
/// soon as a wakeup arrives while idle, through the driver's host signal
/// (a spawn from host context is one), and reconciled with the live task
/// count after every turn. The signal outlives the loop in any `Handle`
/// clone, so once the loop detaches it takes no more.
#[derive(Debug, Default)]
pub(crate) struct Hold {
    held: AtomicBool,
    detached: AtomicBool,
}

impl Hold {
    pub(crate) fn acquire(&self) {
        if !self.detached.load(Relaxed) {
            self.set(true);
        }
    }

    /// The final release: nothing is pushed after it.
    fn detach(&self) {
        self.detached.store(true, Relaxed);
        self.set(false);
    }

    fn set(&self, alive: bool) {
        if self.held.swap(alive, Relaxed) != alive {
            // SAFETY: Emscripten runtime calls; every push is paired with one
            // pop here, the last in `Hosted::drop`.
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

/// The host loop's callbacks into the runtime, and what they hold. Every
/// callback receives the `Hosted` as its argument; it lives in an `Rc` at a
/// fixed address, and detaches on drop before the descriptor closes.
#[derive(Debug)]
pub(super) struct Hosted {
    /// For a deferred callback to find the loop again.
    this: Weak<Hosted>,
    core: Weak<RefCell<Core<LocalRuntime>>>,
    epfd: RawFd,
    /// Tasks per tick: the builder's `event_interval`.
    batch: usize,
    listening: Cell<bool>,
    /// The armed deadline's timeout id.
    deadline: Cell<Option<i32>>,
    /// The follow-up tick for ready work.
    immediate: Cell<Option<i32>>,
    hold: Arc<Hold>,
}

impl Hosted {
    /// Registers with the host loop. The callbacks take the `Hosted`'s
    /// address, so it is placed before it registers.
    pub(super) fn attach(
        core: &Rc<RefCell<Core<LocalRuntime>>>,
        hold: Arc<Hold>,
    ) -> io::Result<Rc<Hosted>> {
        let core_ref = core.borrow();
        let handle = core_ref.handle();
        let Some(epfd) = handle.inner.driver().registry_fd() else {
            return Err(io::Error::other(
                "a hosted event loop needs the I/O driver; call `enable_io` on the runtime builder",
            ));
        };
        let hosted = Rc::new_cyclic(|this| Hosted {
            this: this.clone(),
            core: Rc::downgrade(core),
            epfd: epfd.as_raw_fd(),
            batch: handle.inner.as_current_thread().event_interval(),
            listening: Cell::new(false),
            deadline: Cell::new(None),
            immediate: Cell::new(None),
            hold,
        });
        drop(core_ref);
        hosted.listen(true)?;
        Ok(hosted)
    }

    /// Re-arms the host after a tick or park: a timer for the deadline, an
    /// immediate for ready work, and the hold on the runtime while tasks
    /// are alive.
    pub(super) fn after_turn(&self) {
        let Some(core) = self.core.upgrade() else {
            return;
        };
        let core = core.borrow();
        if let Some(id) = self.deadline.take() {
            // SAFETY: a pending timeout armed below.
            unsafe { emscripten_clear_timeout(id) };
        }
        match core.next_sleep() {
            Some(Duration::ZERO) => self.schedule(),
            Some(after) => {
                let ms = after.as_secs_f64() * 1000.0;
                let this = self as *const Hosted as *mut c_void;
                // SAFETY: `user_data` is this `Hosted`, which clears the
                // timeout in `drop` before the loop drops.
                self.deadline
                    .set(Some(unsafe { emscripten_set_timeout(deadline, ms, this) }));
            }
            None => {}
        }
        self.hold.set(core.handle().inner.num_alive_tasks() > 0);
    }

    /// Adds or removes the readiness listener. It fires every host turn
    /// while the set has uncollected events, so it comes off while a tick
    /// cannot run (see [`tick`](Self::tick)).
    fn listen(&self, on: bool) -> io::Result<()> {
        if self.listening.get() == on {
            return Ok(());
        }
        let this = self as *const Hosted as *mut c_void;
        // SAFETY: the reactor's live epoll fd; `user_data` is this `Hosted`,
        // which removes the listener in `drop` before the loop drops.
        let rc = unsafe {
            if on {
                emscripten_epoll_listener_add(self.epfd, ready, this)
            } else {
                emscripten_epoll_listener_remove(self.epfd, ready, this)
            }
        };
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        self.listening.set(on);
        Ok(())
    }

    /// Schedules a tick on the next host turn, unless one is pending.
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

    /// Ticks the loop from a host callback.
    ///
    /// While a runtime is entered on this thread (a `block_on` suspended
    /// through JSPI) no tick can run; the callback is deferred to that
    /// runtime's exit, and the readiness listener comes off until then
    /// rather than firing every host turn.
    fn tick(&self) {
        let this = self.this.clone();
        let deferred = crate::runtime::jspi::defer_after_runtime_exit(move || {
            if let Some(hosted) = this.upgrade() {
                let _ = hosted.listen(true);
                hosted.schedule();
            }
        });
        if deferred {
            let _ = self.listen(false);
            return;
        }
        let Some(core) = self.core.upgrade() else {
            return;
        };
        // No Rust frame is above the host's callback to catch a panic (a
        // task panic under `UnhandledPanic::ShutdownRuntime`); it would
        // unwind into the host's JavaScript. Abort as an uncaught panic on
        // `main` would.
        let ticked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            core.borrow_mut().tick(self.batch);
            self.after_turn();
        }));
        if ticked.is_err() {
            std::process::abort();
        }
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        if let Some(id) = self.deadline.take() {
            // SAFETY: a pending timeout armed in `after_turn`.
            unsafe { emscripten_clear_timeout(id) };
        }
        if let Some(id) = self.immediate.take() {
            // SAFETY: a pending immediate armed in `schedule`.
            unsafe { emscripten_clear_immediate(id) };
        }
        let _ = self.listen(false);
        self.hold.detach();
    }
}

/// The readiness listener: the set has uncollected events.
extern "C" fn ready(_epfd: i32, user_data: *mut c_void) {
    // SAFETY: `user_data` is the `Hosted` that registered this callback,
    // alive until `drop` unregisters it.
    let hosted = unsafe { &*(user_data as *const Hosted) };
    hosted.tick();
}

/// The deadline timer fired.
extern "C" fn deadline(user_data: *mut c_void) {
    // SAFETY: as `ready`; the timeout is cleared in `drop`.
    let hosted = unsafe { &*(user_data as *const Hosted) };
    hosted.deadline.set(None);
    hosted.tick();
}

/// The follow-up for ready work.
extern "C" fn immediate(user_data: *mut c_void) {
    // SAFETY: as `ready`; the immediate is cleared in `drop`.
    let hosted = unsafe { &*(user_data as *const Hosted) };
    hosted.immediate.set(None);
    hosted.tick();
}
