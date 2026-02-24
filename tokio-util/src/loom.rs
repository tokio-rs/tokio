//! This module abstracts over `loom` and `std::sync` types depending on whether we
//! are running loom tests or not.

pub(crate) mod sync {
    #[cfg(all(test, loom))]
    pub(crate) use loom::sync::{Arc, Mutex, MutexGuard};
    #[cfg(not(all(test, loom)))]
    pub(crate) use std::sync::{Arc, Mutex, MutexGuard};

    pub(crate) mod atomic {
        #[cfg(all(test, loom))]
        pub(crate) use loom::sync::atomic::{AtomicBool, Ordering};
        #[cfg(not(all(test, loom)))]
        pub(crate) use std::sync::atomic::{AtomicBool, Ordering};
    }
}
