use crate::future::Future;
use crate::runtime::SchedulerMetrics;
use crate::runtime::basic_scheduler;
use crate::task::JoinHandle;

cfg_rt_multi_thread! {
    use crate::runtime::thread_pool;
}

#[derive(Debug, Clone)]
pub(crate) enum Spawner {
    Basic(basic_scheduler::Spawner),
    #[cfg(feature = "rt-multi-thread")]
    ThreadPool(thread_pool::Spawner),
}

impl Spawner {
    pub(crate) fn shutdown(&mut self) {
        #[cfg(feature = "rt-multi-thread")]
        {
            if let Spawner::ThreadPool(spawner) = self {
                spawner.shutdown();
            }
        }
    }

    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match self {
            Spawner::Basic(spawner) => spawner.spawn(future),
            #[cfg(feature = "rt-multi-thread")]
            Spawner::ThreadPool(spawner) => spawner.spawn(future),
        }
    }
}

cfg_metrics! {
    impl Spawner {
        pub(crate) fn metrics(&self) -> &SchedulerMetrics {
            match self {
                Spawner::Basic(spawner) => spawner.metrics(),
                #[cfg(feature = "rt-multi-thread")]
                Spawner::ThreadPool(spawner) => spawner.metrics(),
            }
        }
    }
}
