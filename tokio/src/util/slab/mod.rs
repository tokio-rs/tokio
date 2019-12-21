//! A lock-free concurrent slab.

mod addr;
pub(crate) use addr::Address;

mod entry;
pub(crate) use entry::Entry;

mod generation;
pub(crate) use generation::Generation;

mod page;

mod shard;
use shard::Shard;

mod slot;
use slot::Slot;

mod stack;
use stack::TransferStack;

#[cfg(all(loom, test))]
mod tests;

use crate::loom::sync::Mutex;
use crate::util::bit;

use std::fmt;

#[cfg(target_pointer_width = "64")]
const MAX_THREADS: usize = 4096;

#[cfg(target_pointer_width = "32")]
const MAX_THREADS: usize = 2048;

/// Max number of pages per slab
const MAX_PAGES: usize = bit::pointer_width() as usize / 4;

cfg_not_loom! {
    /// Size of first page
    const INITIAL_PAGE_SIZE: usize = 32;
}

cfg_loom! {
    const INITIAL_PAGE_SIZE: usize = 2;
}

/// A sharded slab.
pub(crate) struct Slab<T> {
    // Signal shard for now. Eventually there will be more.
    shard: Shard<T>,
    local: Mutex<()>,
}

unsafe impl<T: Send> Send for Slab<T> {}
unsafe impl<T: Sync> Sync for Slab<T> {}

impl<T: Entry> Slab<T> {
    /// Returns a new slab with the default configuration parameters.
    pub(crate) fn new() -> Slab<T> {
        Slab {
            shard: Shard::new(),
            local: Mutex::new(()),
        }
    }

    /// allocs a value into the slab, returning a key that can be used to
    /// access it.
    ///
    /// If this function returns `None`, then the shard for the current thread
    /// is full and no items can be added until some are removed, or the maximum
    /// number of shards has been reached.
    pub(crate) fn alloc(&self) -> Option<Address> {
        // we must lock the slab to alloc an item.
        let _local = self.local.lock().unwrap();
        self.shard.alloc()
    }

    /// Removes the value associated with the given key from the slab.
    pub(crate) fn remove(&self, idx: Address) {
        // try to lock the slab so that we can use `remove_local`.
        let lock = self.local.try_lock();

        // if we were able to lock the slab, we are "local" and can use the fast
        // path; otherwise, we will use `remove_remote`.
        if lock.is_ok() {
            self.shard.remove_local(idx)
        } else {
            self.shard.remove_remote(idx)
        }
    }

    /// Return a reference to the value associated with the given key.
    ///
    /// If the slab does not contain a value for the given key, `None` is
    /// returned instead.
    pub(crate) fn get(&self, token: Address) -> Option<&T> {
        self.shard.get(token)
    }
}

impl<T> fmt::Debug for Slab<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slab").field("shard", &self.shard).finish()
    }
}
