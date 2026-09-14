//! Minimal JSPI primitives for `wasm32-unknown-emscripten`.
//!
//! [`sleep`] is the runtime's own suspending import, parking the calling
//! activation on a host timer; with `net`, Emscripten's `epoll_wait` suspends
//! as well. The runtime stays entered while parked, so a `block_on` from
//! another promising activation on the thread during the park panics as a
//! nested runtime.

use std::sync::OnceLock;
use std::time::Duration;

// Emscripten EM_JS convention: the `__em_js__<name>` data export carries the
// JS body, and an `__asyncjs__` name gets `WebAssembly.Suspending` treatment
// under `-sJSPI`. `#[used]` is what exports it: on this target LLVM marks
// `llvm.used` symbols exported (the `EMSCRIPTEN_KEEPALIVE` mechanism), while
// rustc keeps `#[no_mangle]` statics out of the linker's export list.
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
#[allow(non_upper_case_globals)]
#[no_mangle]
#[used]
static __em_js____asyncjs__tokio_jspi_sleep: [u8; 169] = *b"(ms)<::>{ return Asyncify.handleAsync(async () => { await new Promise((r) => ms === 0 && typeof setImmediate == 'function' ? setImmediate(r) : setTimeout(r, ms)); }); }\0";

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

/// Whether JSPI suspension is available: linked with `-sJSPI`.
pub(crate) fn jspi_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    *ENABLED.get_or_init(|| unsafe { emscripten_has_asyncify() == 2 })
}

/// Suspend the owning activation for `dur` on a host timer.
pub(crate) fn sleep(dur: Duration) {
    let ms = dur.as_secs_f64() * 1000.0;
    // SAFETY: the import takes an `f64` and returns nothing. Under `-sJSPI`
    // it suspends this activation; the caller has checked `jspi_enabled`.
    unsafe { tokio_jspi_sleep_import(ms) }
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
        wait()
    } else if immediate {
        wait()
    } else {
        panic!(
            "cannot block on wasm32-unknown-emscripten: waiting for I/O \
             readiness needs the build to link `-sJSPI`"
        );
    }
}
