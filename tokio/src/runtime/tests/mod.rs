//! Testing utilities

#[cfg(loom)]
pub(crate) mod loom_oneshot;

#[cfg(not(loom))]
pub(crate) mod mock_park;
