//! Stub out the necessary APIs to model with loom.

mod atomic_u32;
mod atomic_usize;
mod causal_cell;

pub(crate) mod alloc {
    pub(crate) use std::alloc::{alloc, dealloc};

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

pub(crate) mod cell {
    pub(crate) use super::causal_cell::{CausalCell, CausalCheck};
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
    pub(crate) use std::sync::{Arc, Condvar, Mutex};

    pub(crate) mod atomic {
        pub(crate) use crate::loom::atomic_u32::AtomicU32;
        pub(crate) use crate::loom::atomic_usize::AtomicUsize;

        pub(crate) use std::sync::atomic::{fence, spin_loop_hint, AtomicPtr, Ordering};
    }
}

pub(crate) mod sys {
    pub(crate) fn num_cpus() -> usize {
        usize::max(1, num_cpus::get_physical())
    }
}

pub(crate) use std::thread;
