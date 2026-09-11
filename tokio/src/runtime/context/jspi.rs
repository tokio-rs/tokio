//! Minimal JSPI primitives for `wasm32-unknown-emscripten`.
//!
//! [`sleep`] is the one suspending import the runtime issues, parking the
//! calling activation on a host timer.
//!
//! A park leaves the runtime: [`suspended`] swaps the thread's context for the
//! one recorded when the runtime was entered and swaps it back on resume, so
//! sibling promising activations can each drive a runtime on the thread. A
//! suspension Tokio does not issue (a suspending import called from task code)
//! keeps the runtime entered, and a sibling `block_on` during it panics as a
//! nested runtime: it is a blocking call inside a task.

use super::{Context, EnterRuntime, CONTEXT};

use crate::runtime::scheduler;
use crate::util::rand::FastRand;

use std::sync::OnceLock;
use std::time::Duration;

// Emscripten EM_JS convention: `__em_js__<name>` data exports carry JS
// bodies into the objects, and `__asyncjs__` names get
// `WebAssembly.Suspending` treatment under `-sJSPI`. The static must be
// referenced from linked code so its archive member is pulled in, which is
// what `ensure_jspi_sleep_linked` below is for.
//
// A zero-duration park is the scheduler's maintenance yield, and wants the
// cheapest resumption that still lets the host loop reach its timer phase.
// `setTimeout(0)` is clamped to a millisecond, while an immediate resumes
// after the current poll phase and schedules its successor into the next
// iteration, which begins by running expired timers. A microtask-flavoured
// queue (`queueMicrotask`, `process.nextTick`) would not do: those drain
// before the loop advances at all, so host timers could never fire and a
// self-waking task would starve them. Hosts without an immediate keep the
// clamped timeout.
const TOKIO_JSPI_SLEEP: &str = "(ms)<::>{ return Asyncify.handleAsync(async () => { await new Promise((r) => ms === 0 && typeof setImmediate == 'function' ? setImmediate(r) : setTimeout(r, ms)); }); }";

const fn em_js<const N: usize>(s: &str) -> [u8; N] {
    // NUL-terminated: N == s.len() + 1
    let mut a = [0u8; N];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        a[i] = b[i];
        i += 1;
    }
    a
}

#[allow(non_upper_case_globals)]
#[no_mangle]
#[used]
static __em_js____asyncjs__tokio_jspi_sleep: [u8; TOKIO_JSPI_SLEEP.len() + 1] =
    em_js(TOKIO_JSPI_SLEEP);

extern "C" {
    /// Reports the `ASYNCIFY` build mode: 0 = none, 1 = legacy `Asyncify`,
    /// 2 = JSPI. Only mode 2 supports Tokio's JSPI import.
    fn emscripten_has_asyncify() -> i32;
}

// Suspending import: parks on a host timeout. Unit return, never rejects,
// `Asyncify.handleAsync` keeps the runtime alive across the suspension.
#[link(wasm_import_module = "env")]
extern "C-unwind" {
    #[link_name = "__asyncjs__tokio_jspi_sleep"]
    fn tokio_jspi_sleep_import(ms: f64);
}

#[inline(never)]
fn ensure_jspi_sleep_linked() {
    // `#[used]` retains the data in its object; this reference also causes
    // the archive member containing the EM_JS body to be linked.
    std::hint::black_box(__em_js____asyncjs__tokio_jspi_sleep.as_ptr());
}

/// Whether JSPI suspension is available: linked with `-sJSPI`.
pub(crate) fn jspi_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    *ENABLED.get_or_init(|| unsafe { emscripten_has_asyncify() == 2 })
}

/// The context a runtime's dynamic extent writes: what `enter_runtime` and
/// `set_scheduler` set. Poll-scoped state (task id, budget) is idle at a park,
/// and the thread id belongs to the OS thread, shared by every activation.
#[derive(Clone)]
pub(super) struct Snapshot {
    runtime: EnterRuntime,
    rng: Option<FastRand>,
    handle: Option<scheduler::Handle>,
    depth: usize,
    scheduler: *const scheduler::Context,
    entry: Option<Box<Snapshot>>,
}

impl Context {
    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            runtime: self.runtime.get(),
            rng: self.rng.get(),
            handle: self.current.handle.borrow().clone(),
            depth: self.current.depth.get(),
            scheduler: self.scheduler.inner.get(),
            entry: self.entry.borrow().clone(),
        }
    }

    fn restore(&self, s: Snapshot) {
        self.runtime.set(s.runtime);
        self.rng.set(s.rng);
        *self.current.handle.borrow_mut() = s.handle;
        self.current.depth.set(s.depth);
        self.scheduler.inner.set(s.scheduler);
        *self.entry.borrow_mut() = s.entry;
    }
}

/// Runs `f`, which may suspend this activation, with the runtime left: until
/// `f` returns the thread carries the context the runtime was entered from.
/// Restores on unwind too: a JS exception out of the import (such as
/// `SuspendError` from a non-promising activation) unwinds through the
/// `C-unwind` boundary, and the runtime's own guards then unwind cleanly.
/// Outside a runtime there is nothing to leave, so the context is untouched.
fn suspended<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Snapshot>);

    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(mine) = self.0.take() {
                CONTEXT.with(|c| c.restore(mine));
            }
        }
    }

    let _restore = Restore(CONTEXT.with(|c| {
        let mine = c.snapshot();
        let entry = mine.entry.as_deref()?.clone();
        c.restore(entry);
        Some(mine)
    }));
    f()
}

/// Suspend the owning activation for `dur` on a host timer.
pub(crate) fn sleep(dur: Duration) {
    ensure_jspi_sleep_linked();
    let ms = dur.as_secs_f64() * 1000.0;
    // SAFETY: the import takes an `f64` and returns nothing. Under `-sJSPI`
    // it suspends this activation; the caller has checked `jspi_enabled`.
    suspended(|| unsafe { tokio_jspi_sleep_import(ms) })
}

/// The I/O driver's `epoll_wait` of `max_wait` (`None` = no deadline) as a
/// park.
///
/// Under JSPI a non-zero wait suspends on the host loop until readiness or
/// the deadline, but a zero-timeout `epoll_wait` is a synchronous probe, and
/// the host loop is the only producer of readiness, so the scheduler's
/// maintenance park would never let Node deliver socket events. Yield a
/// host turn first, as the zero-duration `ParkThread` park does. Without JSPI
/// `epoll_wait` cannot block at all and returns at once, so a real wait would
/// spin.
#[cfg(feature = "net")]
pub(crate) fn io_wait<R>(max_wait: Option<Duration>, wait: impl FnOnce() -> R) -> R {
    let immediate = max_wait == Some(Duration::ZERO);
    if jspi_enabled() {
        if immediate {
            sleep(Duration::ZERO);
        }
        suspended(wait)
    } else if immediate {
        wait()
    } else {
        panic!(
            "cannot block on wasm32-unknown-emscripten: waiting for I/O \
             readiness needs the build to link `-sJSPI`"
        );
    }
}
