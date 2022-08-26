use crate::future::Future;
use crate::runtime::scheduler::current_thread;
use crate::runtime::task::Id;
use crate::runtime::HandleInner;
use crate::task::JoinHandle;

cfg_rt_multi_thread! {
    use crate::runtime::scheduler::multi_thread;
}

#[derive(Debug, Clone)]
pub(crate) enum Spawner {
    CurrentThread(current_thread::Spawner),
    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
    MultiThread(multi_thread::Spawner),
}

impl Spawner {
    pub(crate) fn shutdown(&mut self) {
        #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
        {
            if let Spawner::MultiThread(spawner) = self {
                spawner.shutdown();
            }
        }
    }

    pub(crate) fn spawn<F>(&self, future: F, id: Id) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match self {
            Spawner::CurrentThread(spawner) => spawner.spawn(future, id),
            #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
            Spawner::MultiThread(spawner) => spawner.spawn(future, id),
        }
    }

    pub(crate) fn as_handle_inner(&self) -> &HandleInner {
        match self {
            Spawner::CurrentThread(spawner) => spawner.as_handle_inner(),
            #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
            Spawner::MultiThread(spawner) => spawner.as_handle_inner(),
        }
    }
}

cfg_metrics! {
    use crate::runtime::{SchedulerMetrics, WorkerMetrics};

    impl Spawner {
        pub(crate) fn num_workers(&self) -> usize {
            match self {
                Spawner::CurrentThread(_) => 1,
                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Spawner::MultiThread(spawner) => spawner.num_workers(),
            }
        }

        pub(crate) fn scheduler_metrics(&self) -> &SchedulerMetrics {
            match self {
                Spawner::CurrentThread(spawner) => spawner.scheduler_metrics(),
                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Spawner::MultiThread(spawner) => spawner.scheduler_metrics(),
            }
        }

        pub(crate) fn worker_metrics(&self, worker: usize) -> &WorkerMetrics {
            match self {
                Spawner::CurrentThread(spawner) => spawner.worker_metrics(worker),
                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Spawner::MultiThread(spawner) => spawner.worker_metrics(worker),
            }
        }

        pub(crate) fn injection_queue_depth(&self) -> usize {
            match self {
                Spawner::CurrentThread(spawner) => spawner.injection_queue_depth(),
                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Spawner::MultiThread(spawner) => spawner.injection_queue_depth(),
            }
        }

        pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
            match self {
                Spawner::CurrentThread(spawner) => spawner.worker_metrics(worker).queue_depth(),
                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Spawner::MultiThread(spawner) => spawner.worker_local_queue_depth(worker),
            }
        }
    }
}
