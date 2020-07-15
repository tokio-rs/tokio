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
        pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            match self {
                Spawner::Shell => panic!("spawning not enabled for runtime"),
                #[cfg(feature = "rt-core")]
                Spawner::Basic(spawner) => spawner.spawn(future),
                #[cfg(feature = "rt-threaded")]
                Spawner::ThreadPool(spawner) => spawner.spawn(future),
            }
        }
    }
}
