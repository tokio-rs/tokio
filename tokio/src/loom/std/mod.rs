#![cfg_attr(any(not(feature = "full"), loom), allow(unused_imports, dead_code))]

mod atomic_u16;
mod atomic_u32;
mod atomic_u64;
mod atomic_usize;
mod barrier;
mod mutex;
#[cfg(all(feature = "parking_lot", not(miri)))]
mod parking_lot;
mod rwlock;
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
    use std::hash::BuildHasher;

    pub(crate) fn seed() -> u64 {
        // Every `RandomState::new()` has keys of its own: random ones per
        // thread, which the next call on that thread changes. Hashing the
        // same value with them gives a new seed each time, with no state of
        // ours that differs from one run of a deterministic simulator to the
        // next (a process-wide counter does).
        RandomState::new().hash_one(0u8)
    }

    #[cfg(test)]
    mod tests {
        use super::seed;

        #[test]
        fn every_seed_on_a_thread_is_new() {
            let seeds = [seed(), seed(), seed()];
            assert_ne!(seeds[0], seeds[1]);
            assert_ne!(seeds[1], seeds[2]);
        }

        #[test]
        #[cfg(not(target_os = "wasi"))] // Wasi doesn't support threads
        fn another_thread_has_another_seed() {
            let here = seed();
            let there = std::thread::spawn(seed).join().unwrap();
            assert_ne!(here, there);
        }
    }
}

pub(crate) mod sync {
    pub(crate) use std::sync::{Arc, Weak};

    // Below, make sure all the feature-influenced types are exported for
    // internal use. Note however that some are not _currently_ named by
    // consuming code.

    // Not using parking_lot in Miri due to <https://github.com/Amanieu/parking_lot/issues/477>.
    #[cfg(all(feature = "parking_lot", not(miri)))]
    #[allow(unused_imports)]
    pub(crate) use crate::loom::std::parking_lot::{
        Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, WaitTimeoutResult,
    };

    #[cfg(not(all(feature = "parking_lot", not(miri))))]
    #[allow(unused_imports)]
    pub(crate) use std::sync::{Condvar, MutexGuard, RwLockReadGuard, WaitTimeoutResult};

    #[cfg(not(all(feature = "parking_lot", not(miri))))]
    pub(crate) use crate::loom::std::mutex::Mutex;

    #[cfg(not(all(feature = "parking_lot", not(miri))))]
    pub(crate) use crate::loom::std::rwlock::RwLock;

    pub(crate) mod atomic {
        pub(crate) use crate::loom::std::atomic_u16::AtomicU16;
        pub(crate) use crate::loom::std::atomic_u32::AtomicU32;
        pub(crate) use crate::loom::std::atomic_u64::AtomicU64;
        pub(crate) use crate::loom::std::atomic_usize::AtomicUsize;

        pub(crate) use std::sync::atomic::{fence, AtomicBool, AtomicPtr, AtomicU8, Ordering};
    }

    pub(crate) use super::barrier::Barrier;
}

pub(crate) mod sys {
    #[cfg(feature = "rt-multi-thread")]
    pub(crate) fn num_cpus() -> usize {
        use std::num::NonZeroUsize;

        const ENV_WORKER_THREADS: &str = "TOKIO_WORKER_THREADS";

        match std::env::var(ENV_WORKER_THREADS) {
            Ok(s) => {
                let n = s.parse().unwrap_or_else(|e| {
                    panic!("\"{ENV_WORKER_THREADS}\" must be usize, error: {e}, value: {s}")
                });
                assert!(n > 0, "\"{ENV_WORKER_THREADS}\" cannot be set to 0");
                n
            }
            Err(std::env::VarError::NotPresent) => {
                std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
            }
            Err(std::env::VarError::NotUnicode(e)) => {
                panic!("\"{ENV_WORKER_THREADS}\" must be valid unicode, error: {e:?}")
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
