//! Extra utilities for spawning tasks

#[cfg(all(tokio_unstable, feature = "rt"))]
mod join_map;
mod spawn_pinned;
pub use spawn_pinned::LocalPoolHandle;

#[cfg(all(tokio_unstable, feature = "rt"))]
#[cfg_attr(docsrs, doc(cfg(all(tokio_unstable, feature = "rt"))))]
pub use join_map::JoinMap;
