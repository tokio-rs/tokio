//! Minimal JSPI primitives for `wasm32-unknown-emscripten`.
//!
//! Tokio's suspension model is strictly non-reentrant: `#[tokio::test]` claims
//! suspension for its whole promising activation with a [`SuspendGuard`],
//! and [`sleep`] — the one suspending import the runtime issues — parks
//! that activation on a host timer. With never more than one suspension in
//! flight, no spill-stack save/restore is needed: any Wasm re-entered
//! while the activation is parked completes before the resume, leaving the
//! spill stack above the suspended frame untouched.

use std::cell::Cell;
use std::time::Duration;

thread_local! {
    static SUSPENDABLE: Cell<bool> = const { Cell::new(false) };
}

/// Marks the `#[tokio::test]` promising activation as suspendable for the
/// body's extent. JSPI availability is module-wide, but only an activation
/// entered through `WebAssembly.promising` may call a suspending import; this
/// guard records that per-activation capability. Internal to the test
/// expansion, not a user convention.
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
    /// Reports the `ASYNCIFY` build mode: 0 = none, 1 = legacy `Asyncify`,
    /// 2 = JSPI. Only mode 2 supports Tokio's JSPI import.
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
fn ensure_jspi_sleep_linked() {
    // `#[used]` retains the data in its object; this reference also causes
    // the archive member containing the EM_JS body to be linked.
    std::hint::black_box(__em_js____asyncjs__tokio_jspi_sleep.as_ptr());
}

/// Whether JSPI suspension is available: linked with `-sJSPI`.
pub fn jspi_enabled() -> bool {
    emscripten_has_asyncify() == 2
}

/// Suspend the owning activation for `dur` on a host timer.
pub(crate) fn sleep(dur: Duration) {
    ensure_jspi_sleep_linked();
    tokio_jspi_sleep_import(dur.as_secs_f64() * 1000.0);
}
