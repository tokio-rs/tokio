mod runtime;

mod options;

#[cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    tokio_unstable
))]
mod event_loop_runtime;
#[cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    tokio_unstable
))]
pub use event_loop_runtime::EventLoopRuntime;

pub use options::LocalOptions;
pub use runtime::LocalRuntime;
pub(crate) use runtime::LocalRuntimeScheduler;
