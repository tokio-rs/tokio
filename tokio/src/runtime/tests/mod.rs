use self::joinable_wrapper::joinable;

mod joinable_wrapper {
    use crate::runtime::task::{JoinHandle, Notified, Schedule};

    #[cfg(all(tokio_unstable, feature = "tracing"))]
    pub(crate) fn joinable<T, S>(task: T) -> (Notified<S>, JoinHandle<T::Output>)
    where
        T: std::future::Future + Send + 'static,
        S: Schedule,
    {
        use tracing::Instrument;
        let span = tracing::trace_span!("test_span");
        let (task, handle) = crate::runtime::task::joinable(task.instrument(span));
        (task.into_notified(), handle)
    }

    #[cfg(not(all(tokio_unstable, feature = "tracing")))]
    pub(crate) fn joinable<T, S>(task: T) -> (Notified<S>, JoinHandle<T::Output>)
    where
        T: std::future::Future + Send + 'static,
        S: Schedule,
    {
        let (task, handle) = crate::runtime::task::joinable(task);
        (task.into_notified(), handle)
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
