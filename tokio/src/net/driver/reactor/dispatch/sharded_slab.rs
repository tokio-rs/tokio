use super::*;
use std::fmt;

use crate::loom::sync::Mutex;

/// A sharded slab.
pub(crate) struct Slab {
    shards: Box<[Shard]>,
}

/// A slab implemented with a single shard.
// TODO(eliza): once worker threads are available, this type will be
// unnecessary and can be removed.
#[derive(Debug)]
pub(crate) struct SingleShard {
    shard: Shard,
    local: Mutex<()>,
}

// ┌─────────────┐      ┌────────┐
// │ page 1      │      │        │
// ├─────────────┤ ┌───▶│  next──┼─┐
// │ page 2      │ │    ├────────┤ │
// │             │ │    │XXXXXXXX│ │
// │ local_free──┼─┘    ├────────┤ │
// │ global_free─┼─┐    │        │◀┘
// ├─────────────┤ └───▶│  next──┼─┐
// │   page 3    │      ├────────┤ │
// └─────────────┘      │XXXXXXXX│ │
//       ...            ├────────┤ │
// ┌─────────────┐      │XXXXXXXX│ │
// │ page n      │      ├────────┤ │
// └─────────────┘      │        │◀┘
//                      │  next──┼───▶
//                      ├────────┤
//                      │XXXXXXXX│
//                      └────────┘
//                         ...
pub(super) struct Shard {
    #[cfg(debug_assertions)]
    tid: usize,
    /// The local free list for each page.
    ///
    /// These are only ever accessed from this shard's thread, so they are
    /// stored separately from the shared state for the page that can be
    /// accessed concurrently, to minimize false sharing.
    local: Box<[page::Local]>,
    /// The shared state for each page in this shard.
    ///
    /// This consists of the page's metadata (size, previous size), remote free
    /// list, and a pointer to the actual array backing that page.
    shared: Box<[page::Shared]>,
}

pub(crate) const TOKEN_SHIFT: usize = Tid::SHIFT + Tid::LEN;
pub(crate) const MAX_SOURCES: usize = (1 << TOKEN_SHIFT) - 1;

#[allow(dead_code)] // coming back soon!
impl Slab {
    /// Returns a new slab with the default configuration parameters.
    pub(crate) fn new() -> Self {
        Self::with_max_threads(MAX_THREADS)
    }

    pub(crate) fn with_max_threads(max_threads: usize) -> Self {
        // Round the max number of threads to the next power of two and clamp to
        // the maximum representable number.
        let max = max_threads.next_power_of_two().min(MAX_THREADS);
        let shards = (0..max).map(Shard::new).collect();
        Self { shards }
    }

    /// allocs a value into the slab, returning a key that can be used to
    /// access it.
    ///
    /// If this function returns `None`, then the shard for the current thread
    /// is full and no items can be added until some are removed, or the maximum
    /// number of shards has been reached.
    pub(crate) fn alloc(&self) -> Option<usize> {
        let tid = Tid::current();
        self.shards[tid.as_usize()].alloc().map(|idx| tid.pack(idx))
    }

    /// Removes the value associated with the given key from the slab.
    pub(crate) fn remove(&self, idx: usize) {
        let tid = Tid::from_packed(idx);
        let shard = &self.shards[tid.as_usize()];
        if tid.is_current() {
            shard.remove_local(idx)
        } else {
            shard.remove_remote(idx)
        }
    }

    /// Return a reference to the value associated with the given key.
    ///
    /// If the slab does not contain a value for the given key, `None` is
    /// returned instead.
    pub(in crate::net::driver) fn get(&self, token: usize) -> Option<&page::ScheduledIo> {
        let tid = Tid::from_packed(token);
        self.shards.get(tid.as_usize())?.get(token)
    }

    /// Returns an iterator over all the items in the slab.
    pub(in crate::net::driver::reactor) fn unique_iter(&mut self) -> iter::UniqueIter<'_> {
        let mut shards = self.shards.iter_mut();
        let shard = shards.next().expect("must be at least 1 shard");
        let mut pages = shard.iter();
        let slots = pages.next().and_then(page::Shared::iter);
        iter::UniqueIter {
            shards,
            slots,
            pages,
        }
    }
}

impl SingleShard {
    /// Returns a new slab with the default configuration parameters.
    pub(crate) fn new() -> Self {
        Self {
            shard: Shard::new(0),
            local: Mutex::new(()),
        }
    }

    /// allocs a value into the slab, returning a key that can be used to
    /// access it.
    ///
    /// If this function returns `None`, then the shard for the current thread
    /// is full and no items can be added until some are removed, or the maximum
    /// number of shards has been reached.
    pub(crate) fn alloc(&self) -> Option<usize> {
        // we must lock the slab to alloc an item.
        let _local = self.local.lock().unwrap();
        self.shard.alloc()
    }

    /// Removes the value associated with the given key from the slab.
    pub(crate) fn remove(&self, idx: usize) {
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
    pub(in crate::net::driver) fn get(&self, token: usize) -> Option<&page::ScheduledIo> {
        self.shard.get(token)
    }

    /// Returns an iterator over all the items in the slab.
    pub(in crate::net::driver::reactor) fn unique_iter(&mut self) -> iter::ShardIter<'_> {
        let mut pages = self.shard.iter_mut();
        let slots = pages.next().and_then(|pg| pg.iter());
        iter::ShardIter { slots, pages }
    }
}

impl Shard {
    fn new(_idx: usize) -> Self {
        let mut total_sz = 0;
        let shared = (0..MAX_PAGES)
            .map(|page_num| {
                let sz = page::size(page_num);
                let prev_sz = total_sz;
                total_sz += sz;
                page::Shared::new(sz, prev_sz)
            })
            .collect();
        let local = (0..MAX_PAGES).map(|_| page::Local::new()).collect();
        Self {
            #[cfg(debug_assertions)]
            tid: _idx,
            local,
            shared,
        }
    }

    fn alloc(&self) -> Option<usize> {
        // Can we fit the value into an existing page?
        for (page_idx, page) in self.shared.iter().enumerate() {
            let local = self.local(page_idx);
            if let Some(page_offset) = page.alloc(local) {
                return Some(page_offset);
            }
        }

        None
    }

    #[inline(always)]
    fn get(&self, idx: usize) -> Option<&page::ScheduledIo> {
        #[cfg(debug_assertions)]
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);

        let addr = page::Addr::from_packed(idx);
        let i = addr.index();

        if i > self.shared.len() {
            return None;
        }
        self.shared[i].get(addr)
    }

    /// Remove an item on the shard's local thread.
    fn remove_local(&self, idx: usize) {
        #[cfg(debug_assertions)]
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
        let addr = page::Addr::from_packed(idx);
        let page_idx = addr.index();

        if let Some(page) = self.shared.get(page_idx) {
            page.remove_local(self.local(page_idx), addr, idx);
        }
    }

    /// Remove an item, while on a different thread from the shard's local thread.
    fn remove_remote(&self, idx: usize) {
        #[cfg(debug_assertions)]
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
        let addr = page::Addr::from_packed(idx);
        let page_idx = addr.index();

        if let Some(page) = self.shared.get(page_idx) {
            page.remove_remote(addr, idx);
        }
    }

    #[inline(always)]
    fn local(&self, i: usize) -> &page::Local {
        &self.local[i]
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, page::Shared> {
        self.shared.iter()
    }

    fn iter_mut(&mut self) -> std::slice::IterMut<'_, page::Shared> {
        self.shared.iter_mut()
    }
}

impl fmt::Debug for Slab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slab")
            .field("shards", &self.shards)
            .finish()
    }
}

unsafe impl Send for Slab {}
unsafe impl Sync for Slab {}

unsafe impl Send for SingleShard {}
unsafe impl Sync for SingleShard {}

impl fmt::Debug for Shard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Shard");

        #[cfg(debug_assertions)]
        d.field("tid", &self.tid);
        d.field("shared", &self.shared).finish()
    }
}
