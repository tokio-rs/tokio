use crate::runtime::Handle;

use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// TODO: dox
#[derive(Clone, Debug)]
pub struct RuntimeMetrics {
    handle: Handle,
}

impl RuntimeMetrics {
    pub(crate) fn new(handle: Handle) -> RuntimeMetrics {
        RuntimeMetrics { handle }
    }

    /// Returns the number of worker threads used by the runtime.
    pub fn num_workers(&self) -> usize {
        todo!();
    }

    /// Returns the number of tasks scheduled from **outside** of the runtime.
    ///
    /// Tasks scheduled from outside of the runtime go via the runtime's
    /// injection queue, which is usually is slower.
    pub fn remote_schedule_count(&self) -> u64 {
        self.handle
            .spawner
            .scheduler_metrics()
            .remote_schedule_count
            .load(Relaxed)
    }

    /// Returns the total number of times this worker thread has parked.
    pub fn worker_park_count(&self, worker: usize) -> u64 {
        self.handle
            .spawner
            .worker_metrics(worker)
            .park_count
            .load(Relaxed)
    }

    /// Returns the number of times this worker unparked but performed no work.
    ///
    /// This is the false-positive wake count.
    pub fn worker_noop_count(&self, worker: usize) -> u64 {
        self.handle
            .spawner
            .worker_metrics(worker)
            .noop_count
            .load(Relaxed)
    }

    /// Returns the number of tasks this worker has stolen from other worker
    /// threads.
    pub fn worker_steal_count(&self, worker: usize) -> u64 {
        self.handle
            .spawner
            .worker_metrics(worker)
            .steal_count
            .load(Relaxed)
    }

    /// Returns the number of times this worker has polled a task.
    pub fn worker_poll_count(&self, worker: usize) -> u64 {
        self.handle
            .spawner
            .worker_metrics(worker)
            .poll_count
            .load(Relaxed)
    }

    /// Returns the total amount of time this worker has been busy for.
    pub fn worker_total_busy_duration(&self, worker: usize) -> Duration {
        let nanos = self
            .handle
            .spawner
            .worker_metrics(worker)
            .busy_duration_total
            .load(Relaxed);
        Duration::from_nanos(nanos)
    }

    /// TODO
    pub fn worker_local_schedule_count(&self, worker: usize) -> u64 {
        self.handle
            .spawner
            .worker_metrics(worker)
            .local_schedule_count
            .load(Relaxed)
    }

    /// Returns the number of tasks moved from this worker's local queue to the
    /// remote queue.
    pub fn worker_overflow_count(&self, worker: usize) -> u64 {
        self.handle
            .spawner
            .worker_metrics(worker)
            .overflow_count
            .load(Relaxed)
    }

    /// Returns the number of tasks currently in the worker's local queue
    pub fn worker_local_queue_depth(&self, worker: usize) -> usize {
        self.handle.spawner.worker_local_queue_depth(worker)
    }
}
