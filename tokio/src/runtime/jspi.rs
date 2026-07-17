//! Minimal JSPI primitives for `wasm32-unknown-emscripten`.
//!
//! Tokio's suspension model is strictly non-reentrant: `#[tokio::test]` claims
//! suspension for its whole promising activation with a [`SuspendGuard`],
//! and [`sleep`] — the one suspending import the runtime issues — parks
//! that activation on a host timer. With never more than one suspension in
//! flight, no spill-stack save/restore is needed: any wasm re-entered
//! while the activation is parked completes before the resume, leaving the
//! spill stack above the suspended frame untouched.

use std::cell::Cell;
use std::time::Duration;

thread_local! {
    static SUSPENDABLE: Cell<bool> = const { Cell::new(false) };
}

/// Marks the `#[tokio::test]` promising activation as suspendable for the
/// body's extent; `Drop` clears the flag across panic unwinds. Internal to
/// the test expansion, not a user convention.
#[derive(Debug)]
pub struct SuspendGuard(());

impl SuspendGuard {
    /// Marks the current activation suspendable until drop.
    #[allow(clippy::new_without_default)]
    pub fn new() -> SuspendGuard {
        SUSPENDABLE.set(true);
        SuspendGuard(())
    }
}

impl Drop for SuspendGuard {
    fn drop(&mut self) {
        SUSPENDABLE.set(false);
    }
}

/// Whether the park leaf may suspend: a [`SuspendGuard`] is live.
pub(crate) fn can_suspend() -> bool {
    SUSPENDABLE.get()
}

// Emscripten EM_JS convention: `__em_js__<name>` data exports carry JS
// bodies into the objects, and `__asyncjs__` names get
// `WebAssembly.Suspending` treatment under `-sJSPI`. The static must be
// referenced from linked code (`anchor`) so its archive member is pulled
// in.
const TOKIO_JSPI_SLEEP: &str = "(ms)<::>{ return Asyncify.handleAsync(async () => { await new Promise((r) => setTimeout(r, ms)); }); }";

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

unsafe extern "C" {
    /// Reports the ASYNCIFY build mode: 0 = none, 1 = asyncify, 2 = JSPI.
    safe fn emscripten_has_asyncify() -> i32;
}

// Suspending import: parks on a host timeout. Unit return, never rejects,
// `Asyncify.handleAsync` keeps the runtime alive across the suspension.
#[link(wasm_import_module = "env")]
unsafe extern "C-unwind" {
    #[link_name = "__asyncjs__tokio_jspi_sleep"]
    safe fn tokio_jspi_sleep_import(ms: f64);
}

#[inline(never)]
fn anchor() {
    std::hint::black_box(__em_js____asyncjs__tokio_jspi_sleep.as_ptr());
}

/// Whether JSPI suspension is available: linked with `-sJSPI`.
pub fn jspi_enabled() -> bool {
    emscripten_has_asyncify() == 2
}

/// Suspend the owning activation for `dur` on a host timer.
pub(crate) fn sleep(dur: Duration) {
    anchor();
    tokio_jspi_sleep_import(dur.as_secs_f64() * 1000.0);
}
