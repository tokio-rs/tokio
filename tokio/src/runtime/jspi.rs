//! Minimal JSPI primitives for `wasm32-unknown-emscripten`, over Emscripten's
//! promise and event loop C APIs.
//!
//! [`park`] suspends the calling activation on a promise, settled by a host
//! timer at the deadline or by [`unpark`] from a later activation (a host
//! callback entering tokio). Both the runtime driver and the thread parker
//! behind `blocking_recv` and friends park through here, so neither needs the
//! `rt` feature. The runtime stays entered while parked, so a `block_on` from
//! another activation on the thread during the park panics as a nested
//! runtime.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering::SeqCst};
use std::sync::OnceLock;
use std::time::Duration;

/// `em_promise_t`: an index into the host's promise table, never null.
type Promise = *mut c_void;

const EM_PROMISE_FULFILL: i32 = 0;

/// Minimum Emscripten version required for JSPI (`emscripten#27720` fixed
/// `emscripten_clear_timeout` and `emscripten_clear_immediate` keep-alive
/// handling in 6.0.10).
const MIN_EMSCRIPTEN_VERSION: (u32, u32, u32) = (6, 0, 10);

#[repr(C)]
struct UtsName {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

extern "C" {
    /// Reports the `ASYNCIFY` build mode: 0 = none, 1 = legacy `Asyncify`,
    /// 2 = JSPI. Only mode 2 supports suspension.
    fn emscripten_has_asyncify() -> i32;

    fn uname(buf: *mut UtsName) -> i32;

    fn emscripten_promise_create() -> Promise;
    fn emscripten_promise_destroy(promise: Promise);
    fn emscripten_promise_resolve(promise: Promise, result: i32, value: *mut c_void);

    fn emscripten_set_timeout(
        cb: extern "C" fn(*mut c_void),
        msecs: f64,
        user_data: *mut c_void,
    ) -> i32;
    fn emscripten_clear_timeout(id: i32);
    fn emscripten_set_immediate(cb: extern "C" fn(*mut c_void), user_data: *mut c_void) -> i32;
    fn emscripten_clear_immediate(id: i32);
}

/// `em_settled_result_t`, returned by value.
#[repr(C)]
struct SettledResult {
    result: i32,
    value: *mut c_void,
}

extern "C-unwind" {
    // Suspending import (Emscripten marks it `__async`): resolves to the
    // promise's settled result. Under `-sJSPI` the wrapper is
    // `Asyncify.handleAsync`, which keeps the runtime alive across the
    // suspension. Linkable without JSPI, where it aborts if reached. Present
    // in every Emscripten the Rust target supports, unlike the newer
    // `emscripten_promise_await_unchecked`.
    //
    // Suspension needs a `WebAssembly.promising` activation on the stack.
    // From any other activation (a plain host callback into the module) the
    // engine throws `WebAssembly.SuspendError` out of this import instead. It
    // is a foreign exception to Rust: drops run as it unwinds, `catch_unwind`
    // does not catch it, and it aborts at the first `extern "C"` frame.
    fn emscripten_promise_await(promise: Promise) -> SettledResult;
}

#[derive(Copy, Clone)]
enum JspiState {
    Disabled,
    Enabled,
    UnsupportedVersion(u32, u32, u32),
}

fn emscripten_version() -> Option<(u32, u32, u32)> {
    let mut uts = UtsName {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };
    // SAFETY: `uts` is a valid `struct utsname` buffer. Emscripten's `uname`
    // populates `uts.release` with `"__EMSCRIPTEN_MAJOR__.__EMSCRIPTEN_MINOR__.__EMSCRIPTEN_TINY__"`.
    if unsafe { uname(&mut uts) } != 0 {
        return None;
    }
    let len = uts
        .release
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(uts.release.len());
    let release = std::str::from_utf8(&uts.release[..len]).ok()?;
    let mut parts = release.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let tiny = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, tiny))
}

fn detect_jspi() -> JspiState {
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    if unsafe { emscripten_has_asyncify() } != 2 {
        return JspiState::Disabled;
    }
    match emscripten_version() {
        Some((major, minor, tiny)) if (major, minor, tiny) < MIN_EMSCRIPTEN_VERSION => {
            JspiState::UnsupportedVersion(major, minor, tiny)
        }
        // All existing versions use `major.minor.tiny`, so an unrecognised
        // format is assumed to be a newer version.
        _ => JspiState::Enabled,
    }
}

/// Whether JSPI suspension is available: linked with `-sJSPI`.
pub(crate) fn jspi_enabled() -> bool {
    static STATE: OnceLock<JspiState> = OnceLock::new();
    match *STATE.get_or_init(detect_jspi) {
        JspiState::Disabled => false,
        JspiState::Enabled => true,
        JspiState::UnsupportedVersion(major, minor, tiny) => panic!(
            "cannot use JSPI on wasm32-unknown-emscripten with Emscripten \
             {major}.{minor}.{tiny}: Emscripten 6.0.10 or later is required"
        ),
    }
}

/// The promise a parked activation is suspended on, null while not parked.
///
/// Set before the suspension and cleared on resumption, with nothing running
/// in between on this thread, so an [`unpark`] that observes it holds a live
/// handle.
#[derive(Debug)]
pub(crate) struct Slot(AtomicPtr<c_void>);

impl Slot {
    pub(crate) const fn new() -> Self {
        Self(AtomicPtr::new(ptr::null_mut()))
    }
}

/// A zero-duration park is the scheduler's maintenance yield, and wants the
/// cheapest resumption that still lets the host loop reach its timer phase.
/// `setTimeout(0)` is clamped to a millisecond, while an immediate resumes
/// after the current poll phase and schedules its successor into the next
/// iteration, which begins by running expired timers. A microtask-flavoured
/// queue would not do: those drain before the loop advances at all, so host
/// timers could never fire and a self-waking task would starve them. Timeouts
/// above the host's 32-bit millisecond limit would be clamped to one, so they
/// are capped; a spurious resume at the cap re-parks.
enum Timer {
    Timeout(i32),
    Immediate(i32),
}

impl Timer {
    fn set(promise: Promise, dur: Duration) -> Self {
        // SAFETY: the callback outlives the timer, which `Drop` clears
        // before the promise it resolves is destroyed.
        unsafe {
            if dur.is_zero() {
                Timer::Immediate(emscripten_set_immediate(resolve, promise))
            } else {
                let ms = (dur.as_secs_f64() * 1000.0).min(0x7fff_ffff as f64);
                Timer::Timeout(emscripten_set_timeout(resolve, ms, promise))
            }
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // SAFETY: ids come from the matching `set` call. Clearing a fired
        // timer is a no-op.
        unsafe {
            match *self {
                Timer::Timeout(id) => emscripten_clear_timeout(id),
                Timer::Immediate(id) => emscripten_clear_immediate(id),
            }
        }
    }
}

extern "C" fn resolve(promise: *mut c_void) {
    // SAFETY: the timer holding this pointer is cleared before the promise
    // is destroyed, so it is live.
    unsafe { emscripten_promise_resolve(promise, EM_PROMISE_FULFILL, ptr::null_mut()) }
}

struct Park<'a> {
    slot: &'a Slot,
    promise: Promise,
    timer: Option<Timer>,
}

impl Drop for Park<'_> {
    fn drop(&mut self) {
        self.slot.0.store(ptr::null_mut(), SeqCst);
        drop(self.timer.take());
        // SAFETY: created by `park` and not yet destroyed; the timer that
        // could resolve it has been cleared.
        unsafe { emscripten_promise_destroy(self.promise) }
    }
}

/// Suspend the owning activation until [`unpark`] is called on `slot`, or
/// `dur` elapses on a host timer if given.
pub(crate) fn park(slot: &Slot, dur: Option<Duration>) {
    // A host activation entering tokio during the suspension shares this
    // thread's locals but is not on the runtime: with the scheduler context
    // left set, its wakes would take the on-runtime shortcut and never
    // unpark.
    #[cfg(feature = "rt")]
    let _scheduler = crate::runtime::context::clear_scheduler();

    // SAFETY: creates a fresh promise handle; destroyed by `Park`'s drop.
    let promise = unsafe { emscripten_promise_create() };
    let park = Park {
        slot,
        promise,
        timer: dur.map(|dur| Timer::set(promise, dur)),
    };
    let prev = slot.0.swap(promise, SeqCst);
    debug_assert!(prev.is_null(), "parker already parked");

    // SAFETY: the handle is live. Under `-sJSPI` this suspends the
    // activation; the caller has checked `jspi_enabled`. A `SuspendError`
    // unwinding out of it drops `park`, clearing the timer and handle. The
    // promise is only ever fulfilled, so the result carries nothing.
    let _ = unsafe { emscripten_promise_await(park.promise) };
}

/// Resume the activation parked on `slot`, if any.
pub(crate) fn unpark(slot: &Slot) {
    let promise = slot.0.load(SeqCst);
    if !promise.is_null() {
        // SAFETY: a non-null slot is a live handle (see `Slot`). Resolving an
        // already-settled promise is a no-op, so a race with the timer is
        // harmless. Does not suspend.
        unsafe { emscripten_promise_resolve(promise, EM_PROMISE_FULFILL, ptr::null_mut()) }
    }
}
