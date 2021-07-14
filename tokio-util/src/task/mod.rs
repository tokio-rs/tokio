//! Extra utilities for spawning tasks

mod spawn_pinned;
pub use spawn_pinned::{new_local_pool, LocalPoolHandle};
