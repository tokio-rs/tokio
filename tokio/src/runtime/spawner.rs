use crate::future::Future;
use crate::runtime::basic_scheduler;
use crate::runtime::stats::RuntimeStats;
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

    #[cfg_attr(not(all(tokio_unstable, feature = "stats")), allow(dead_code))]
    pub(crate) fn stats(&self) -> &RuntimeStats {
        match self {
            Spawner::Basic(spawner) => spawner.stats(),
            #[cfg(feature = "rt-multi-thread")]
            Spawner::ThreadPool(spawner) => spawner.stats(),
        }
    }
}
