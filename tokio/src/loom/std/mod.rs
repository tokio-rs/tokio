#![cfg_attr(any(not(feature = "full"), loom), allow(unused_imports, dead_code))]

mod atomic_u16;
mod atomic_u32;
mod atomic_u64;
mod atomic_usize;
mod barrier;
mod mutex;
#[cfg(feature = "parking_lot")]
mod parking_lot;
mod unsafe_cell;

pub(crate) mod cell {
    pub(crate) use super::unsafe_cell::UnsafeCell;
}

#[cfg(any(
    feature = "net",
    feature = "process",
    feature = "signal",
    feature = "sync",
))]
pub(crate) mod future {
    pub(crate) use crate::sync::AtomicWaker;
}

pub(crate) mod hint {
    pub(crate) use std::hint::spin_loop;
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
    pub(crate) use std::sync::{Arc, Weak};

    // Below, make sure all the feature-influenced types are exported for
    // internal use. Note however that some are not _currently_ named by
    // consuming code.

    #[cfg(feature = "parking_lot")]
    #[allow(unused_imports)]
    pub(crate) use crate::loom::std::parking_lot::{
        Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, WaitTimeoutResult,
    };

    #[cfg(not(feature = "parking_lot"))]
    #[allow(unused_imports)]
    pub(crate) use std::sync::{Condvar, MutexGuard, RwLock, RwLockReadGuard, WaitTimeoutResult};

    #[cfg(not(feature = "parking_lot"))]
    pub(crate) use crate::loom::std::mutex::Mutex;

    pub(crate) mod atomic {
        pub(crate) use crate::loom::std::atomic_u16::AtomicU16;
        pub(crate) use crate::loom::std::atomic_u32::AtomicU32;
        pub(crate) use crate::loom::std::atomic_u64::{AtomicU64, StaticAtomicU64};
        pub(crate) use crate::loom::std::atomic_usize::AtomicUsize;

        pub(crate) use std::sync::atomic::{fence, AtomicBool, AtomicPtr, AtomicU8, Ordering};
    }

    pub(crate) use super::barrier::Barrier;
}

pub(crate) mod sys {
    #[cfg(feature = "rt-multi-thread")]
    pub(crate) fn num_cpus() -> usize {
        const ENV_WORKER_THREADS: &str = "TOKIO_WORKER_THREADS";

        match std::env::var(ENV_WORKER_THREADS) {
            Ok(s) => {
                let n = s.parse().unwrap_or_else(|e| {
                    panic!(
                        "\"{}\" must be usize, error: {}, value: {}",
                        ENV_WORKER_THREADS, e, s
                    )
                });
                assert!(n > 0, "\"{}\" cannot be set to 0", ENV_WORKER_THREADS);
                n
            }
            Err(std::env::VarError::NotPresent) => usize::max(1, num_cpus::get()),
            Err(std::env::VarError::NotUnicode(e)) => {
                panic!(
                    "\"{}\" must be valid unicode, error: {:?}",
                    ENV_WORKER_THREADS, e
                )
            }
        }
    }

    #[cfg(not(feature = "rt-multi-thread"))]
    pub(crate) fn num_cpus() -> usize {
        1
    }
}

pub(crate) mod thread {
    #[inline]
    pub(crate) fn yield_now() {
        std::hint::spin_loop();
    }

    #[allow(unused_imports)]
    pub(crate) use std::thread::{
        current, panicking, park, park_timeout, sleep, spawn, AccessError, Builder, JoinHandle,
        LocalKey, Result, Thread, ThreadId,
    };
}
