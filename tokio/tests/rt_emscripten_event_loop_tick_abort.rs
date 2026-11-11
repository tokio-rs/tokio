//! As `rt_emscripten_event_loop_abort`, for a tick the program calls on its
//! own stack: a JavaScript exception thrown by an import while
//! `LocalEventLoop::tick` polls a task must abort rather than leave the task
//! running for the caller's JavaScript to catch the exception and move on.
//!
//! The binary passes only by aborting (`Module.tokioExpectAbort`, see
//! `rt_emscripten_pre.js`).

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
        let mut el = Builder::new_current_thread()
            .build_local_event_loop(Default::default())
            .unwrap();
        let root = el.spawn_local(async {
            run_js("throw new Error('import threw during a tick')");
        });
        el.tick(64);
        unreachable!("the tick returned, root finished: {}", root.is_finished());
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
