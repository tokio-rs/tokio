use crate::loom::sync::atomic::{AtomicU64, Ordering::Relaxed};
use crate::runtime::metrics::WorkerMetrics;

/// Retrieves metrics from the Tokio runtime.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[derive(Debug)]
pub struct RuntimeMetrics {
    /// Number of tasks that are scheduled from outside the runtime.
    remote_schedule_count: AtomicU64,

    /// Tracks per-worker metrics
    workers: Box<[WorkerMetrics]>,
}

impl RuntimeMetrics {
    pub(crate) fn new(worker_threads: usize) -> RuntimeMetrics {
        let mut workers = Vec::with_capacity(worker_threads);

        for _ in 0..worker_threads {
            workers.push(WorkerMetrics {
                park_count: AtomicU64::new(0),
                noop_count: AtomicU64::new(0),
                steal_count: AtomicU64::new(0),
                poll_count: AtomicU64::new(0),
                stolen_count: AtomicU64::new(0),
                overflow_count: AtomicU64::new(0),
                busy_duration_total: AtomicU64::new(0),
                local_schedule_count: AtomicU64::new(0),
            });
        }

        RuntimeMetrics {
            remote_schedule_count: AtomicU64::new(0),
            workers: workers.into_boxed_slice(),
        }
    }

    /// Returns the number of tasks scheduled from **outside** of the runtime.
    ///
    /// Tasks scheduled from outside of the runtime go via the runtime's
    /// injection queue, which is usually is slower.
    pub fn remote_schedule_count(&self) -> u64 {
        self.remote_schedule_count.load(Relaxed)
    }

    /// Returns a slice containing the metrics for each worker thread.
    pub fn workers(&self) -> &[WorkerMetrics] {
        &self.workers
    }

    /// Increment the number of tasks scheduled externally
    pub(crate) fn inc_remote_schedule_count(&self) {
        self.remote_schedule_count.fetch_add(1, Relaxed);
    }

    pub(crate) fn worker(&self, index: usize) -> &WorkerMetrics {
        &self.workers[index]
    }
}
