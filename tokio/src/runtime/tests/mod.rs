use self::joinable_wrapper::joinable;

mod joinable_wrapper {
    use crate::runtime::blocking::NoopSchedule;
    use crate::runtime::task::{JoinHandle, Notified};

    #[cfg(all(tokio_unstable, feature = "tracing"))]
    pub(crate) fn joinable<T>(task: T) -> (Notified<NoopSchedule>, JoinHandle<T::Output>)
    where
        T: std::future::Future + Send + 'static,
    {
        use tracing::Instrument;
        let span = tracing::trace_span!("test_span");
        let task = task.instrument(span);
        let (task, handle) = crate::runtime::task::joinable(task, NoopSchedule);
        (task, handle)
    }

    #[cfg(not(all(tokio_unstable, feature = "tracing")))]
    pub(crate) fn joinable<T>(task: T) -> (Notified<NoopSchedule>, JoinHandle<T::Output>)
    where
        T: std::future::Future + Send + 'static,
    {
        let (task, handle) = crate::runtime::task::joinable(task, NoopSchedule);
        (task, handle)
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
