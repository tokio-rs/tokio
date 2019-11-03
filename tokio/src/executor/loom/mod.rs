//! Stub out the necessary APIs to model with loom.

#[cfg(not(all(test, loom)))]
pub(crate) mod std;

#[cfg(all(test, loom))]
pub(crate) mod std {
    pub(crate) use loom::{alloc, cell, sync, thread};

    pub(crate) mod rand {
        pub(crate) fn seed() -> u64 {
            1
        }
    }

    pub(crate) mod sys {
        pub(crate) fn num_cpus() -> usize {
            2
        }
    }
}

#[cfg(feature = "rt-full")]
pub(crate) use self::std::rand;
#[cfg(any(feature = "blocking", feature = "rt-current-thread"))]
pub(crate) use self::std::sync;
#[cfg(any(feature = "blocking", feature = "rt-full"))]
pub(crate) use self::std::thread;
#[cfg(feature = "rt-current-thread")]
pub(crate) use self::std::{alloc, cell, sys};
