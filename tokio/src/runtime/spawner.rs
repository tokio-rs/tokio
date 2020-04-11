cfg_rt_core! {
    use crate::runtime::basic_scheduler;
    use crate::task::JoinHandle;

    use std::future::Future;
}

cfg_rt_threaded! {
    use crate::runtime::thread_pool;
}

#[derive(Debug, Clone)]
pub(crate) enum Spawner {
    Shell,
    #[cfg(feature = "rt-core")]
    Basic(basic_scheduler::Spawner),
    #[cfg(feature = "rt-threaded")]
    ThreadPool(thread_pool::Spawner),
}

cfg_rt_core! {
    impl Spawner {
        /// Spawns the `future` on the scheduler.
        ///
        /// If `must_awake` is `true`, and throttling is activated,
        /// the scheduler is woken up if it was asleep.
        pub(crate) fn spawn<F>(&self, future: F, must_awake: bool) -> JoinHandle<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            match self {
                Spawner::Shell => panic!("spawning not enabled for runtime"),
                #[cfg(feature = "rt-core")]
                Spawner::Basic(spawner) => spawner.spawn(future, must_awake),
                #[cfg(feature = "rt-threaded")]
                Spawner::ThreadPool(spawner) => spawner.spawn(future),
            }
        }
    }
}
