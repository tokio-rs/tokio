#[cfg(not(all(tokio_unstable, feature = "tracing")))]
use crate::runtime::task::joinable;

#[cfg(all(tokio_unstable, feature = "tracing"))]
use self::joinable_wrapper::joinable;

#[cfg(all(tokio_unstable, feature = "tracing"))]
mod joinable_wrapper {
    use crate::runtime::task::{JoinHandle, Notified, Schedule};
    use tracing::Instrument;

    pub(crate) fn joinable<T, S>(task: T) -> (Notified<S>, JoinHandle<T::Output>)
    where
        T: std::future::Future + Send + 'static,
        S: Schedule,
    {
        let span = tracing::trace_span!("test_span");
        crate::runtime::task::joinable(task.instrument(span))
    }
}

cfg_loom! {
    mod loom_basic_scheduler;
    mod loom_blocking;
    mod loom_oneshot;
    mod loom_pool;
    mod loom_queue;
    mod loom_shutdown_join;
}

cfg_not_loom! {
    mod queue;

    #[cfg(miri)]
    mod task;
}
