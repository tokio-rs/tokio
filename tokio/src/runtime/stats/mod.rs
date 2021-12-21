//! This module contains information need to view information about how the
//! runtime is performing.
//!
//! **Note**: This is an [unstable API][unstable]. The public API of types in
//! this module may break in 1.x releases. See [the documentation on unstable
//! features][unstable] for details.
//!
//! [unstable]: crate#unstable-features
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
