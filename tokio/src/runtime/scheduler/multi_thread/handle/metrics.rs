use super::Handle;

cfg_unstable_metrics! {
    use crate::runtime::{SchedulerMetrics, WorkerMetrics};
}

impl Handle {
    pub(crate) fn num_workers(&self) -> usize {
        self.shared.worker_metrics.len()
    }

    cfg_unstable_metrics! {
        pub(crate) fn num_blocking_threads(&self) -> usize {
            // workers are currently spawned using spawn_blocking
            self.blocking_spawner
                .num_threads()
                .saturating_sub(self.num_workers())
        }

        pub(crate) fn num_idle_blocking_threads(&self) -> usize {
            self.blocking_spawner.num_idle_threads()
        }

        pub(crate) fn active_tasks_count(&self) -> usize {
            self.shared.owned.active_tasks_count()
        }

        pub(crate) fn scheduler_metrics(&self) -> &SchedulerMetrics {
            &self.shared.scheduler_metrics
        }

        pub(crate) fn worker_metrics(&self, worker: usize) -> &WorkerMetrics {
            &self.shared.worker_metrics[worker]
        }

        pub(crate) fn injection_queue_depth(&self) -> usize {
            self.shared.injection_queue_depth()
        }

        pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
            self.shared.worker_local_queue_depth(worker)
        }

        pub(crate) fn blocking_queue_depth(&self) -> usize {
            self.blocking_spawner.queue_depth()
        }
    }
}
