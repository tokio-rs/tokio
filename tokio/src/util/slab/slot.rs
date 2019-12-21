use crate::loom::cell::CausalCell;
use crate::util::slab::{Entry, Generation};

/// Stores an entry in the slab.
pub(super) struct Slot<T> {
    next: CausalCell<usize>,
    entry: T,
}

impl<T: Entry> Slot<T> {
    /// Initialize a new `Slot` linked to `next`.
    ///
    /// The entry is initialized to a default value.
    pub(super) fn new(next: usize) -> Slot<T> {
        Slot {
            next: CausalCell::new(next),
            entry: T::default(),
        }
    }

    pub(super) fn get(&self) -> &T {
        &self.entry
    }

    pub(super) fn generation(&self) -> Generation {
        self.entry.generation()
    }

    pub(super) fn reset(&self, generation: Generation) -> bool {
        self.entry.reset(generation)
    }

    pub(super) fn next(&self) -> usize {
        self.next.with(|next| unsafe { *next })
    }

    pub(super) fn set_next(&self, next: usize) {
        self.next.with_mut(|n| unsafe {
            (*n) = next;
        })
    }
}
