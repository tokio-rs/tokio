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

pub(crate) use self::std::sync;
#[cfg(any(feature = "blocking", feature = "thread-pool"))]
pub(crate) use self::std::thread;
#[cfg(feature = "thread-pool")]
pub(crate) use self::std::{alloc, cell, rand, sys};
