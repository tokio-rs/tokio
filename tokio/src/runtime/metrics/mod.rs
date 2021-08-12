cfg_metrics! {
    mod metrics;
}

cfg_not_metrics! {
    #[path = "mock.rs"]
    mod metrics;
}

pub(crate) use self::metrics::{RuntimeMetrics, WorkerMetricsBatcher};
