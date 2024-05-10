//! Extra utilities for spawning tasks

#[cfg(tokio_unstable)]
mod join_map;
mod spawn_pinned;
pub use spawn_pinned::LocalPoolHandle;

#[cfg(tokio_unstable)]
#[cfg_attr(docsrs, doc(cfg(all(tokio_unstable, feature = "rt"))))]
pub use join_map::{JoinMap, JoinMapKeys};

pub mod task_tracker;
pub use task_tracker::TaskTracker;
