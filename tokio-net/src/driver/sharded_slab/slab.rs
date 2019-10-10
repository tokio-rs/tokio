use super::*;
use std::fmt;

/// A sharded slab.
pub(crate) struct Slab<T> {
    shards: Box<[Shard<T>]>,
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
pub(super) struct Shard<T> {
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
    shared: Box<[page::Shared<T>]>,
}

impl<T> Slab<T> {
    pub(crate) const MAX_BIT: usize = Tid::SHIFT + Tid::LEN;

    /// Returns a new slab with the default configuration parameters.
    pub(crate) fn new() -> Self {
        let shards = (0..MAX_THREADS).map(Shard::new).collect();
        Self { shards }
    }

    /// Inserts a value into the slab, returning a key that can be used to
    /// access it.
    ///
    /// If this function returns `None`, then the shard for the current thread
    /// is full and no items can be added until some are removed, or the maximum
    /// number of shards has been reached.
    pub(crate) fn insert(&self, value: T) -> Option<usize> {
        let tid = Tid::current();
        #[cfg(test)]
        println!("insert {:?}", tid);
        self.shards[tid.as_usize()]
            .insert(value)
            .map(|idx| tid.pack(idx))
    }

    /// Removes the value associated with the given key from the slab, returning
    /// it.
    ///
    /// If the slab does not contain a value for that key, `None` is returned
    /// instead.
    pub(crate) fn remove(&self, idx: usize) -> Option<T> {
        let tid = Tid::from_packed(idx);
        #[cfg(test)]
        println!("rm {:?}", tid);
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
    pub(crate) fn get(&self, key: usize) -> Option<&T> {
        let tid = Tid::from_packed(key);
        #[cfg(test)]
        println!("get {:?}", tid);
        self.shards.get(tid.as_usize())?.get(key)
    }

    /// Returns an iterator over all the items in the slab.
    pub(crate) fn unique_iter(&mut self) -> iter::UniqueIter<'_, T> {
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

impl<T> Shard<T> {
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

    fn insert(&self, value: T) -> Option<usize> {
        let mut value = Some(value);

        // Can we fit the value into an existing page?
        for (page_idx, page) in self.shared.iter().enumerate() {
            let local = self.local(page_idx);
            #[cfg(test)]
            println!("-> page {}; {:?}; {:?}", page_idx, local, page);

            if let Some(page_offset) = page.insert(local, &mut value) {
                return Some(page_offset);
            }
        }

        None
    }

    #[inline(always)]
    fn get(&self, idx: usize) -> Option<&T> {
        #[cfg(debug_assertions)]
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);

        let addr = page::Addr::from_packed(idx);
        let i = addr.index();
        #[cfg(test)]
        println!("-> {:?}; idx {:?}", addr, i);
        if i > self.shared.len() {
            return None;
        }
        self.shared[i].get(addr)
    }

    /// Remove an item on the shard's local thread.
    fn remove_local(&self, idx: usize) -> Option<T> {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
        }
        let addr = page::Addr::from_packed(idx);
        let page = addr.index();

        #[cfg(test)]
        println!("-> remove_local {:?}; page {:?}", addr, page);

        self.shared.get(page)?.remove_local(self.local(page), addr)
    }

    /// Remove an item, while on a different thread from the shard's local thread.
    fn remove_remote(&self, idx: usize) -> Option<T> {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
            debug_assert!(Tid::current().as_usize() != self.tid);
        }
        let addr = page::Addr::from_packed(idx);
        let page = addr.index();

        #[cfg(test)]
        println!("-> remove_remote {:?}; page {:?}", addr, page);

        self.shared.get(page)?.remove_remote(addr)
    }

    #[inline(always)]
    fn local(&self, i: usize) -> &page::Local {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            Tid::current().as_usize(),
            self.tid,
            "tried to access local data from another thread!"
        );

        &self.local[i]
    }

    pub(super) fn iter<'a>(&'a self) -> std::slice::Iter<'a, page::Shared<T>> {
        self.shared.iter()
    }
}

impl<T: fmt::Debug> fmt::Debug for Slab<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slab")
            .field("shards", &self.shards)
            .finish()
    }
}

unsafe impl<T: Send> Send for Slab<T> {}
unsafe impl<T: Sync> Sync for Slab<T> {}

impl<T: fmt::Debug> fmt::Debug for Shard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Shard");

        #[cfg(debug_assertions)]
        d.field("tid", &self.tid);
        d.field("shared", &self.shared).finish()
    }
}
