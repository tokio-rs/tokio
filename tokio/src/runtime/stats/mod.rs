//! This module contains information need to view information about how the
//! runtime is performing.
#![allow(clippy::module_inception)]

cfg_stats! {
    mod stats;

    pub use self::stats::{RuntimeStats, WorkerStats};
    pub(crate) use self::stats::WorkerStatsBatcher;
}

cfg_not_stats! {
    #[path = "mock.rs"]
    mod stats;

    pub(crate) use self::stats::{RuntimeStats, WorkerStatsBatcher};
}
