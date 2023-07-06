#[cfg(not(loom))]
pub(crate) use std::sync;

#[cfg(loom)]
pub(crate) use loom::sync;
