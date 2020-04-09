#![cfg_attr(any(not(feature = "full"), loom), allow(unused_imports, dead_code))]

mod atomic_ptr;
mod atomic_u16;
mod atomic_u32;
mod atomic_u64;
mod atomic_u8;
mod atomic_usize;
mod unsafe_cell;

pub(crate) mod cell {
    pub(crate) use super::unsafe_cell::UnsafeCell;
}

#[cfg(any(feature = "sync", feature = "io-driver"))]
pub(crate) mod future {
    pub(crate) use crate::sync::AtomicWaker;
}

pub(crate) mod rand {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    use std::sync::atomic::AtomicU32;
    use std::sync::atomic::Ordering::Relaxed;

    static COUNTER: AtomicU32 = AtomicU32::new(1);

    pub(crate) fn seed() -> u64 {
        let rand_state = RandomState::new();

        let mut hasher = rand_state.build_hasher();

        // Hash some unique-ish data to generate some new state
        COUNTER.fetch_add(1, Relaxed).hash(&mut hasher);

        // Get the seed
        hasher.finish()
    }
}

pub(crate) mod sync {
    pub(crate) use std::sync::Arc;

    #[cfg(feature = "parking_lot")]
    mod pl_wrappers;

    // Below, make sure all the feature-influenced types are exported for
    // internal use. Note however that some are not _currently_ named by
    // consuming code.

    #[cfg(feature = "parking_lot")]
    #[allow(unused_imports)]
    pub(crate) use pl_wrappers::{Condvar, Mutex};

    #[cfg(feature = "parking_lot")]
    #[allow(unused_imports)]
    pub(crate) use parking_lot::{MutexGuard, WaitTimeoutResult};

    #[cfg(not(feature = "parking_lot"))]
    #[allow(unused_imports)]
    pub(crate) use std::sync::{Condvar, Mutex, MutexGuard, WaitTimeoutResult};

    pub(crate) mod atomic {
        pub(crate) use crate::loom::std::atomic_ptr::AtomicPtr;
        pub(crate) use crate::loom::std::atomic_u16::AtomicU16;
        pub(crate) use crate::loom::std::atomic_u32::AtomicU32;
        pub(crate) use crate::loom::std::atomic_u64::AtomicU64;
        pub(crate) use crate::loom::std::atomic_u8::AtomicU8;
        pub(crate) use crate::loom::std::atomic_usize::AtomicUsize;

        pub(crate) use std::sync::atomic::{spin_loop_hint, AtomicBool};
    }
}

pub(crate) mod sys {
    #[cfg(feature = "rt-threaded")]
    pub(crate) fn num_cpus() -> usize {
        usize::max(1, num_cpus::get())
    }

    #[cfg(not(feature = "rt-threaded"))]
    pub(crate) fn num_cpus() -> usize {
        1
    }
}

pub(crate) use std::thread;
