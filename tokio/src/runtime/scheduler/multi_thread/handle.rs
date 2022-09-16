use crate::runtime::scheduler::multi_thread::Spawner;
use crate::runtime::{blocking, driver};
use crate::util::RngSeedGenerator;

/// Handle to the multi thread scheduler
#[derive(Debug)]
pub(crate) struct Handle {
    /// Task spawner
    pub(crate) spawner: Spawner,

    /// Resource driver handles
    pub(crate) driver: driver::Handle,

    /// Blocking pool spawner
    pub(crate) blocking_spawner: blocking::Spawner,

    /// Current random number generator seed
    pub(crate) seed_generator: RngSeedGenerator,
}

cfg_metrics! {
    use crate::runtime::{SchedulerMetrics, WorkerMetrics};

    impl Handle {
        pub(crate) fn num_workers(&self) -> usize {
            self.spawner.shared.worker_metrics.len()
        }

        pub(crate) fn scheduler_metrics(&self) -> &SchedulerMetrics {
            &self.spawner.shared.scheduler_metrics
        }

        pub(crate) fn worker_metrics(&self, worker: usize) -> &WorkerMetrics {
            &self.spawner.shared.worker_metrics[worker]
        }

        pub(crate) fn injection_queue_depth(&self) -> usize {
            self.spawner.shared.injection_queue_depth()
        }

        pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
            self.spawner.shared.worker_local_queue_depth(worker)
        }
    }
}
