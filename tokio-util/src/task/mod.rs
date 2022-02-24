//! Extra utilities for spawning tasks

mod spawn_pinned;
pub use spawn_pinned::LocalPoolHandle;

cfg_unstable! {
    mod join_map;
    pub use join_map::JoinMap;
}
