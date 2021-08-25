//! This module contains information need to view information about how the
//! runtime is performing.
#![allow(clippy::module_inception)]

cfg_metrics! {
    mod metrics;

    mod counter_duration;

    pub use self::metrics::{RuntimeMetrics, WorkerMetrics};
    pub(crate) use self::metrics::WorkerMetricsBatcher;
}

cfg_not_metrics! {
    #[path = "mock.rs"]
    mod metrics;

    pub(crate) use self::metrics::{RuntimeMetrics, WorkerMetricsBatcher};
}
