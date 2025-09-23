//! This module abstracts over `loom` and `std::sync` types depending on whether we
//! are running loom tests or not.

pub(crate) mod sync {
    #[cfg(loom)]
    pub(crate) use loom::sync::{Arc, Mutex, MutexGuard};
    #[cfg(not(loom))]
    pub(crate) use std::sync::{Arc, Mutex, MutexGuard};
}
