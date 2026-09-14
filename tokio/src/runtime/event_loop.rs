//! Host glue for [`EventLoopRuntime`]: the runtime's wait, lowered to the
//! host event loop.
//!
//! The scheduler drives to a fixed point, then waits. On this target a wait
//! is either a JSPI suspension (`block_on`, resumed on the same stack) or a
//! return to the host to be called back. This module is the latter: the same
//! `current_thread` scheduler and drivers, pumped to the fixed point by
//! [`drive`](EventLoopState::drive), with the wait lowered to "arm one host
//! timer for the soonest deadline and return". Socket readiness needs no
//! arming: the epoll readiness listener is persistent.
//!
//! Host callbacks only run on an empty stack, and a drive never yields
//! mid-drive, so a callback re-enters the drive inline. Armed callbacks hold
//! `Weak` refs: dropping the [`EventLoopRuntime`] drops the runtime with
//! native `Runtime::drop` semantics, and a late callback upgrades to nothing.
//!
//! [`EventLoopRuntime`]: crate::runtime::EventLoopRuntime

use crate::runtime::local_runtime::LocalRuntimeScheduler;
use crate::runtime::{context, LocalRuntime};

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::{Arc, Weak};

type Callback = unsafe extern "C-unwind" fn(*mut c_void);

extern "C" {
    /// Runs `cb(user_data)` after `msecs` on the host loop, holding the
    /// Emscripten runtime keepalive until it fires. `emscripten_clear_timeout`
    /// leaks that keepalive, so timers here are never cleared.
    fn emscripten_set_timeout(cb: Option<Callback>, msecs: f64, user_data: *mut c_void) -> i32;
    /// Runs `cb(user_data)` on the host loop's next check phase (`setImmediate`
    /// where available), after pending timers, holding the keepalive.
    fn emscripten_set_immediate(cb: Option<Callback>, user_data: *mut c_void) -> i32;
}

#[cfg(feature = "net")]
extern "C" {
    /// Persistent readiness listener on an epoll fd: `cb(user_data)` runs on
    /// the host loop whenever the set has uncollected ready events. Removed
    /// with the last fd to the epoll instance.
    fn emscripten_epoll_add_listener(
        epfd: i32,
        cb: Option<Callback>,
        user_data: *mut c_void,
    ) -> i32;
}

/// An event-loop runtime and its continuation state: the armed deadline
/// timer and pending immediate drive.
#[derive(Debug)]
pub(crate) struct EventLoopState {
    runtime: LocalRuntime,
    /// The armed deadline timer's target tick and epoch. Timers are
    /// fire-only, so a superseded arm stays pending and fires as a stale
    /// no-op: the epoch tells a live arm from a stale one.
    armed: Cell<Option<(u64, u64)>>,
    epoch: Cell<u64>,
    /// An immediate (0 ms) drive is armed.
    drive_armed: Cell<bool>,
    /// A drive is on the stack: wakes it produces are absorbed by its own
    /// fixed point and arming.
    driving: Cell<bool>,
}

// SAFETY: this module is compiled only without `atomics`, where the target
// has a single thread of execution; the impls only satisfy the auto-trait
// bounds of the driver handles the `Weak` hooks live in.
unsafe impl Send for EventLoopState {}
unsafe impl Sync for EventLoopState {}

impl EventLoopState {
    pub(crate) fn new(runtime: LocalRuntime) -> Arc<EventLoopState> {
        Arc::new(EventLoopState {
            runtime,
            armed: Cell::new(None),
            epoch: Cell::new(0),
            drive_armed: Cell::new(false),
            driving: Cell::new(false),
        })
    }

    pub(crate) fn runtime(&self) -> &LocalRuntime {
        &self.runtime
    }

    /// Run one scheduler batch (`event_interval` tasks, then a driver turn),
    /// then arm the continuation: an immediate drive if work remains, so the
    /// host loop gets a turn between batches as it does under a JSPI
    /// `block_on`, and the host timer for the soonest deadline.
    pub(crate) fn drive(self: &Arc<Self>) {
        let (scheduler, rt_handle) = self.runtime.parts();
        let LocalRuntimeScheduler::CurrentThread(exec) = scheduler;
        let handle = rt_handle.inner.as_current_thread();

        struct Driving<'a>(&'a Cell<bool>);
        impl Drop for Driving<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let driving = Driving(&self.driving);
        self.driving.set(true);
        let busy = context::enter_runtime(&rt_handle.inner, false, |_| exec.drive_batch(handle));
        drop(driving);

        if busy {
            self.arm_drive();
        }
        self.arm_timer(next_deadline_tick(handle));
    }

    fn arm_timer(self: &Arc<Self>, next: Option<u64>) {
        if self.armed.get().map(|(tick, _)| tick) == next {
            return;
        }
        let epoch = self.epoch.get() + 1;
        self.epoch.set(epoch);
        let Some(tick) = next else {
            self.armed.set(None);
            return;
        };
        self.armed.set(Some((tick, epoch)));
        let ms = ms_until(self.runtime.parts().1.inner.as_current_thread(), tick);
        let arm = Box::into_raw(Box::new(TimerArm {
            state: Arc::downgrade(self),
            epoch,
        }));
        // SAFETY: `timer_entry` reclaims the box exactly once; the timer is
        // never cleared.
        unsafe { emscripten_set_timeout(Some(timer_entry), ms, arm.cast()) };
    }

    /// Arm an immediate drive for a wake from host context, so `Waker::wake`
    /// stays cheap and never runs tasks on the stack of the caller. A wake
    /// during a drive is absorbed by that drive.
    pub(crate) fn arm_drive(self: &Arc<Self>) {
        if self.driving.get() || self.drive_armed.replace(true) {
            return;
        }
        let weak = Weak::into_raw(Arc::downgrade(self));
        // SAFETY: `drive_entry` reclaims the `Weak` exactly once.
        unsafe { emscripten_set_immediate(Some(drive_entry), weak as *mut c_void) };
    }

    /// Whether a drive of this runtime is on the stack.
    #[cfg(feature = "net")]
    pub(crate) fn is_driving(&self) -> bool {
        self.driving.get()
    }

    /// Arm the epoll readiness listener: `on_ready(user_data)` re-enters the
    /// drive from the host loop.
    #[cfg(feature = "net")]
    pub(crate) fn add_epoll_listener(epfd: i32, on_ready: Callback, user_data: *mut c_void) {
        // SAFETY: `epfd` is the reactor's live epoll fd; `user_data` outlives
        // it, and the listener is removed with the fd.
        let rc = unsafe { emscripten_epoll_add_listener(epfd, Some(on_ready), user_data) };
        assert_eq!(rc, 0, "emscripten_epoll_add_listener failed: errno {rc}");
    }
}

struct TimerArm {
    state: Weak<EventLoopState>,
    epoch: u64,
}

#[cfg(feature = "time")]
fn next_deadline_tick(handle: &crate::runtime::scheduler::current_thread::Handle) -> Option<u64> {
    handle
        .driver
        .time
        .as_ref()
        .and_then(|time| time.next_expiration_tick())
}

#[cfg(not(feature = "time"))]
fn next_deadline_tick(_handle: &crate::runtime::scheduler::current_thread::Handle) -> Option<u64> {
    None
}

#[cfg(feature = "time")]
fn ms_until(handle: &crate::runtime::scheduler::current_thread::Handle, tick: u64) -> f64 {
    let time = handle.driver.time.as_ref().expect("time driver");
    let now = time.time_source().now(&handle.driver.clock);
    let until = time
        .time_source()
        .tick_to_duration(tick.saturating_sub(now));
    until.as_secs_f64() * 1000.0
}

#[cfg(not(feature = "time"))]
fn ms_until(_handle: &crate::runtime::scheduler::current_thread::Handle, _tick: u64) -> f64 {
    unreachable!("no time driver, no deadline")
}

/// The deadline timer fired: drive, unless a later arm superseded this one.
unsafe extern "C-unwind" fn timer_entry(user_data: *mut c_void) {
    // SAFETY: `user_data` is the `TimerArm` box from `arm_timer`.
    let arm = unsafe { Box::from_raw(user_data as *mut TimerArm) };
    let Some(state) = arm.state.upgrade() else {
        return;
    };
    if state.epoch.get() != arm.epoch {
        return;
    }
    state.armed.set(None);
    state.drive();
}

/// The immediate drive fired.
unsafe extern "C-unwind" fn drive_entry(user_data: *mut c_void) {
    // SAFETY: `user_data` is the raw `Weak` from `arm_drive`.
    let weak = unsafe { Weak::from_raw(user_data as *const EventLoopState) };
    let Some(state) = weak.upgrade() else {
        return;
    };
    state.drive_armed.set(false);
    state.drive();
}
