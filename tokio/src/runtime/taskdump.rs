//! Serializable structures for task dumps. 

use super::scheduler::{self, current_thread};
use crate::loom::sync::Arc;
use serde::Serialize;

#[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
use scheduler::multi_thread;

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize)]
#[serde(tag = "flavor")]
/// A taskdump of a Tokio runtime.
pub(super) enum Runtime {
    /// A taskdump of a current-thread runtime.
    CurrentThread(CurrentThread),
    /// A taskdump of a multi-thread runtime.
    MultiThread(MultiThread),
}

impl Runtime {
    pub(super) fn from(handle: &super::Handle) -> Self {
        let metrics = handle.metrics();
        match &handle.inner {
            scheduler::Handle::CurrentThread(h) => {
                Runtime::CurrentThread(CurrentThread::from(h.clone(), metrics))
            }
            #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
            scheduler::Handle::MultiThread(h) => {
                Runtime::MultiThread(MultiThread::from(h.clone(), metrics))
            }
        }
    }
}

/// A taskdump of a current-thread runtime.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize)]
pub(super) struct CurrentThread {
    /// Runtime metrics.
    metrics: CurrentThreadMetrics,
}

impl CurrentThread {
    fn from(_handle: Arc<current_thread::Handle>, metrics: super::RuntimeMetrics) -> Self {
        Self {
            metrics: CurrentThreadMetrics {
                remote_schedule_count: metrics.remote_schedule_count(),
                #[cfg(feature = "net")]
                io_driver: Some(IODriverMetrics {
                    fd_registered_count: metrics.io_driver_fd_registered_count(),
                    fd_deregistered_count: metrics.io_driver_fd_deregistered_count(),
                }),
                #[cfg(not(feature = "net"))]
                io_driver: None,
            },
        }
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize)]
pub(super) struct MultiThread {
    /// Runtime metrics.
    metrics: MultiThreadMetrics,
}

impl MultiThread {
    pub(super) fn from(_handle: Arc<multi_thread::Handle>, metrics: super::RuntimeMetrics) -> Self {
        Self {
            metrics: MultiThreadMetrics {
                num_workers: metrics.num_workers(),
                num_blocking_threads: metrics.num_blocking_threads(),
                num_idle_blocking_threads: metrics.num_idle_blocking_threads(),
                remote_schedule_count: metrics.remote_schedule_count(),
                #[cfg(feature = "net")]
                io_driver: Some(IODriverMetrics {
                    fd_registered_count: metrics.io_driver_fd_registered_count(),
                    fd_deregistered_count: metrics.io_driver_fd_deregistered_count(),
                }),
                #[cfg(not(feature = "net"))]
                io_driver: None,
            },
        }
    }
}

/// Metrics for a current-thread runtime.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize)]
struct CurrentThreadMetrics {
    /// The total number of tasks scheduled from outside of the runtime.
    remote_schedule_count: u64,
    /// IO driver metrics.
    io_driver: Option<IODriverMetrics>,
}

/// Metrics for a multi-thread runtime.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(serde::Serialize)]
struct MultiThreadMetrics {
    /// The number of worker threads used by the runtime.
    num_workers: usize,
    /// The number of additional threads spawned by the runtime.
    num_blocking_threads: usize,
    /// The number of idle threads, which have spawned for spawn_blocking calls.
    num_idle_blocking_threads: usize,
    /// The total number of tasks scheduled from outside of the runtime.
    remote_schedule_count: u64,
    /// IO driver metrics.
    io_driver: Option<IODriverMetrics>,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize)]
struct IODriverMetrics {
    /// The number of file descriptors that have been registered with the runtime’s I/O driver.
    fd_registered_count: u64,
    /// The number of file descriptors that have been deregistered by the runtime’s I/O driver
    fd_deregistered_count: u64,
}
