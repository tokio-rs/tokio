// rt-full implies rt-current-thread

#[cfg(feature = "rt-full")]
mod atomic_u32;
mod atomic_usize;
#[cfg(feature = "rt-current-thread")]
mod causal_cell;

#[cfg(feature = "rt-current-thread")]
pub(crate) mod alloc;

#[cfg(feature = "rt-current-thread")]
pub(crate) mod cell {
    pub(crate) use super::causal_cell::{CausalCell, CausalCheck};
}

pub(crate) mod future {
    pub(crate) use crate::sync::AtomicWaker;
}

#[cfg(feature = "rt-full")]
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
    // #[cfg(any(feature = "blocking", feature = "rt-current-thread"))]
    pub(crate) use std::sync::*;

    pub(crate) mod atomic {
        #[cfg(feature = "rt-full")]
        pub(crate) use crate::loom::std::atomic_u32::AtomicU32;
        #[cfg(feature = "rt-current-thread")]
        pub(crate) use crate::loom::std::atomic_usize::AtomicUsize;

        #[cfg(feature = "rt-full")]
        pub(crate) use std::sync::atomic::spin_loop_hint;
        #[cfg(feature = "rt-current-thread")]
        pub(crate) use std::sync::atomic::{fence, AtomicPtr};
    }
}

#[cfg(feature = "rt-current-thread")]
pub(crate) mod sys {
    #[cfg(feature = "rt-full")]
    pub(crate) fn num_cpus() -> usize {
        usize::max(1, num_cpus::get_physical())
    }

    #[cfg(not(feature = "rt-full"))]
    pub(crate) fn num_cpus() -> usize {
        1
    }
}

#[cfg(any(feature = "blocking", feature = "rt-full"))]
pub(crate) use std::thread;
