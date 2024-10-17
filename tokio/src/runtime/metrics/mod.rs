//! This module contains information need to view information about how the
//! runtime is performing.
//!
//! **Note**: This is an [unstable API][unstable]. The public API of types in
//! this module may break in 1.x releases. See [the documentation on unstable
//! features][unstable] for details.
//!
//! [unstable]: crate#unstable-features
#![allow(clippy::module_inception)]

mod runtime;
pub use runtime::RuntimeMetrics;

mod worker;
pub(crate) use worker::WorkerMetrics;

mod batch;
pub(crate) use batch::MetricsBatch;

cfg_unstable_metrics! {
    #[allow(unreachable_pub)] // rust-lang/rust#57411
    pub use histogram::HistogramScale;


    mod scheduler;
    pub(crate) use scheduler::SchedulerMetrics;

    cfg_net! {
        mod io;
        pub(crate) use io::IoDriverMetrics;
    }

    mod histogram;
    pub(crate) use histogram::{Histogram, HistogramBatch, HistogramBuilder};
}

cfg_not_unstable_metrics! {
    mod mock;

    pub(crate) use mock::{SchedulerMetrics, Histogram, HistogramBatch, HistogramBuilder};

}
