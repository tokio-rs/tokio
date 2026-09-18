//! Minimal JSPI primitives for `wasm32-unknown-emscripten`.
//!
//! [`park`] suspends the calling activation on a promise held in a per-parker
//! slot on the JS side, settled by a host timer at the deadline or by
//! [`unpark`] from a later activation (a host callback entering tokio). The
//! runtime stays entered while parked, so a `block_on` from another
//! activation on the thread during the park panics as a nested runtime.

use std::sync::OnceLock;
use std::time::Duration;

// Emscripten EM_JS convention: the `__em_js__<name>` data export carries the
// JS body, and an `__asyncjs__` name gets `WebAssembly.Suspending` treatment
// under `-sJSPI`. `#[used]` is what exports it: on this target LLVM marks
// `llvm.used` symbols exported (the `EMSCRIPTEN_KEEPALIVE` mechanism), while
// rustc keeps `#[no_mangle]` statics out of the linker's export list.
//
// `ms < 0` is a park with no deadline. A zero-duration park is the
// scheduler's maintenance yield, and wants the cheapest resumption that
// still lets the host loop reach its timer phase. `setTimeout(0)` is clamped
// to a millisecond, while an immediate resumes after the current poll phase
// and schedules its successor into the next iteration, which begins by
// running expired timers. A microtask-flavoured queue (`queueMicrotask`,
// `process.nextTick`) would not do: those drain before the loop advances at
// all, so host timers could never fire and a self-waking task would starve
// them. Hosts without an immediate keep the clamped timeout. Timeouts above
// the host's 32-bit millisecond limit would be clamped to one, so they are
// capped; a spurious resume at the cap re-parks.
#[allow(non_upper_case_globals)]
#[no_mangle]
#[used]
static __em_js____asyncjs__tokio_jspi_park: [u8; 508] = *b"(id, ms)<::>{ return Asyncify.handleAsync(async () => { const parks = Module.tokioParks || (Module.tokioParks = new Map()); await new Promise((resolve) => { const immediate = ms === 0 && typeof setImmediate == 'function'; const done = () => { parks.delete(id); resolve(); }; const timer = ms < 0 ? undefined : immediate ? setImmediate(done) : setTimeout(done, Math.min(ms, 0x7fffffff)); parks.set(id, () => { if (timer !== undefined) (immediate ? clearImmediate : clearTimeout)(timer); done(); }); }); }); }\0";

#[allow(non_upper_case_globals)]
#[no_mangle]
#[used]
static __em_js__tokio_jspi_unpark: [u8; 91] = *b"(id)<::>{ const wake = Module.tokioParks && Module.tokioParks.get(id); if (wake) wake(); }\0";

extern "C" {
    /// Reports the `ASYNCIFY` build mode: 0 = none, 1 = legacy `Asyncify`,
    /// 2 = JSPI. Only mode 2 supports Tokio's JSPI imports.
    fn emscripten_has_asyncify() -> i32;
}

#[link(wasm_import_module = "env")]
extern "C-unwind" {
    // Suspending import: parks on the slot for `id`. Unit return, never
    // rejects, `Asyncify.handleAsync` keeps the runtime alive across the
    // suspension.
    #[link_name = "__asyncjs__tokio_jspi_park"]
    fn tokio_jspi_park_import(id: usize, ms: f64);

    // Settles the slot for `id`, if parked. The resumption is a microtask,
    // so it runs once the calling activation has returned to the host.
    #[link_name = "tokio_jspi_unpark"]
    fn tokio_jspi_unpark_import(id: usize);
}

/// Whether JSPI suspension is available: linked with `-sJSPI`.
pub(crate) fn jspi_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    // SAFETY: an Emscripten libc query with no arguments and no side effects.
    *ENABLED.get_or_init(|| unsafe { emscripten_has_asyncify() == 2 })
}

/// Suspend the owning activation until [`unpark`] is called for `id`, or
/// `dur` elapses on a host timer if given.
pub(crate) fn park(id: usize, dur: Option<Duration>) {
    let ms = dur.map_or(-1.0, |dur| dur.as_secs_f64() * 1000.0);
    // A host activation entering tokio during the suspension shares this
    // thread's locals but is not on the runtime: with the scheduler context
    // left set, its wakes would take the on-runtime shortcut and never
    // unpark.
    super::CONTEXT.with(|c| {
        let scheduler = c.scheduler.inner.replace(std::ptr::null());
        // SAFETY: the import takes plain scalars and returns nothing. Under
        // `-sJSPI` it suspends this activation; the caller has checked
        // `jspi_enabled`.
        unsafe { tokio_jspi_park_import(id, ms) }
        c.scheduler.inner.set(scheduler);
    });
}

/// Resume the activation parked under `id`, if any.
pub(crate) fn unpark(id: usize) {
    // SAFETY: the import takes a plain scalar and returns nothing, and does
    // not suspend.
    unsafe { tokio_jspi_unpark_import(id) }
}
