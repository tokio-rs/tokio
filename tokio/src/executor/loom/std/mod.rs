#[cfg(feature = "rt-full")]
mod atomic_u32;
mod atomic_usize;
#[cfg(feature = "rt-full")]
mod causal_cell;

#[cfg(feature = "rt-full")]
pub(crate) mod alloc {
    #[derive(Debug)]
    pub(crate) struct Track<T> {
        value: T,
    }

    impl<T> Track<T> {
        pub(crate) fn new(value: T) -> Track<T> {
            Track { value }
        }

        pub(crate) fn get_mut(&mut self) -> &mut T {
            &mut self.value
        }

        pub(crate) fn into_inner(self) -> T {
            self.value
        }
    }
}

#[cfg(feature = "rt-full")]
pub(crate) mod cell {
    pub(crate) use super::causal_cell::{CausalCell, CausalCheck};
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
    pub(crate) use std::sync::{Arc, Condvar, Mutex};

    pub(crate) mod atomic {
        #[cfg(feature = "rt-full")]
        pub(crate) use crate::executor::loom::std::atomic_u32::AtomicU32;
        pub(crate) use crate::executor::loom::std::atomic_usize::AtomicUsize;

        #[cfg(feature = "rt-full")]
        pub(crate) use std::sync::atomic::{fence, spin_loop_hint, AtomicPtr};
    }
}

#[cfg(feature = "rt-full")]
pub(crate) mod sys {
    pub(crate) fn num_cpus() -> usize {
        usize::max(1, num_cpus::get_physical())
    }
}

#[cfg(any(feature = "blocking", feature = "rt-full"))]
pub(crate) use std::thread;
