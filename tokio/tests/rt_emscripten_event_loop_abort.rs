//! A JavaScript exception thrown by an import while a hosted tick polls a
//! task unwinds the Rust frames, running their drops, but `catch_unwind`
//! catches only Rust panics. The task would be left running, with nothing
//! to complete it or wake its `JoinHandle`; the tick must abort instead.
//! (The host's callbacks are `extern "C"`, so the exception would abort
//! there too, as a panic in a function that cannot unwind.)
//!
//! `harness = false`, so `main` can return into the host loop, whose
//! callback ticks the loop. The binary passes only by aborting
//! (`Module.tokioExpectAbort`, see `rt_emscripten_pre.js`).

#[cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    tokio_unstable,
    feature = "rt"
))]
mod emscripten {
    use tokio::runtime::Builder;

    extern "C" {
        fn emscripten_run_script(script: *const std::ffi::c_char);
    }

    fn run_js(script: &str) {
        let script = std::ffi::CString::new(script).unwrap();
        // SAFETY: a NUL-terminated script evaluated on the host.
        unsafe { emscripten_run_script(script.as_ptr()) }
    }

    pub(super) fn main() {
        run_js("Module.tokioExpectAbort = true");
        let el = Box::leak(Box::new(
            Builder::new_current_thread()
                .build_hosted_local_event_loop(Default::default())
                .unwrap(),
        ));
        // The spawn schedules a host tick, which polls the task.
        el.spawn_local(async {
            run_js("throw new Error('import threw during a hosted tick')");
        });
    }
}

fn main() {
    #[cfg(all(
        target_os = "emscripten",
        not(target_feature = "atomics"),
        tokio_unstable,
        feature = "rt"
    ))]
    emscripten::main();
}
