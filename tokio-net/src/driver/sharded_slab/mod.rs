//! A lock-free concurrent slab.
mod cfg;
mod page;
mod tid;
pub(crate) use tid::Tid;
mod iter;

use page::slot;
use std::fmt;

/// A sharded slab.
///
/// See the [crate-level documentation](index.html) for details on using this type.
pub(crate) struct Slab<T, C: cfg::Config = cfg::DefaultConfig> {
    shards: Box<[Shard<T, C>]>,
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
struct Shard<T, C: cfg::Config> {
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
    shared: Box<[page::Shared<T, C>]>,
}

impl<T> Slab<T> {
    /// Returns a new slab with the default configuration parameters.
    pub(crate) fn new() -> Self {
        Self::new_with_config()
    }

    /// Returns a new slab with the provided configuration parameters.
    pub(crate) fn new_with_config<C: cfg::Config>() -> Slab<T, C> {
        C::validate();
        let shards = (0..C::MAX_SHARDS).map(Shard::new).collect();
        Slab { shards }
    }
}

impl<T, C: cfg::Config> Slab<T, C> {
    pub(crate) const MAX_BIT: usize = slot::Generation::<C>::LEN + slot::Generation::<C>::SHIFT;

    /// Inserts a value into the slab, returning a key that can be used to
    /// access it.
    ///
    /// If this function returns `None`, then the shard for the current thread
    /// is full and no items can be added until some are removed, or the maximum
    /// number of shards has been reached.
    ///
    /// # Examples
    /// ```rust
    /// # use sharded_slab::Slab;
    /// let slab = Slab::new();
    ///
    /// let key = slab.insert("hello world").unwrap();
    /// assert_eq!(slab.get(key), Some(&"hello world"));
    /// ```
    pub(crate) fn insert(&self, value: T) -> Option<usize> {
        let tid = Tid::<C>::current();
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
        let tid = C::unpack_tid(idx);
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
    ///
    /// # Examples
    ///
    /// ```
    /// let slab = sharded_slab::Slab::new();
    /// let key = slab.insert("hello world").unwrap();
    ///
    /// assert_eq!(slab.get(key), Some(&"hello world"));
    /// assert_eq!(slab.get(12345), None);
    /// ```
    pub(crate) fn get(&self, key: usize) -> Option<&T> {
        let tid = C::unpack_tid(key);
        #[cfg(test)]
        println!("get {:?}", tid);
        self.shards.get(tid.as_usize())?.get(key)
    }

    /// Returns an iterator over all the items in the slab.
    pub(crate) fn unique_iter<'a>(&'a mut self) -> iter::UniqueIter<'a, T, C> {
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

impl<T, C: cfg::Config> Shard<T, C> {
    fn new(_idx: usize) -> Self {
        let mut total_sz = 0;
        let shared = (0..C::MAX_PAGES)
            .map(|page_num| {
                let sz = C::page_size(page_num);
                let prev_sz = total_sz;
                total_sz += sz;
                page::Shared::new(sz, prev_sz)
            })
            .collect();
        let local = (0..C::MAX_PAGES).map(|_| page::Local::new()).collect();
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
        debug_assert_eq!(Tid::<C>::from_packed(idx).as_usize(), self.tid);
        let addr = C::unpack_addr(idx);
        let i = addr.index();
        #[cfg(test)]
        println!("-> {:?}; idx {:?}", addr, i);
        if i > self.shared.len() {
            return None;
        }
        self.shared[i].get(addr, idx)
    }

    /// Remove an item on the shard's local thread.
    fn remove_local(&self, idx: usize) -> Option<T> {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(Tid::<C>::from_packed(idx).as_usize(), self.tid);
        }
        let addr = C::unpack_addr(idx);
        let page = addr.index();

        #[cfg(test)]
        println!("-> remove_local {:?}; page {:?}", addr, page);

        self.shared
            .get(page)?
            .remove_local(self.local(page), addr, C::unpack_gen(idx))
    }

    /// Remove an item, while on a different thread from the shard's local thread.
    fn remove_remote(&self, idx: usize) -> Option<T> {
        #[cfg(debug_assertions)]
        {
            debug_assert_eq!(Tid::<C>::from_packed(idx).as_usize(), self.tid);
            debug_assert!(Tid::<C>::current().as_usize() != self.tid);
        }
        let addr = C::unpack_addr(idx);
        let page = addr.index();

        #[cfg(test)]
        println!("-> remove_remote {:?}; page {:?}", addr, page);

        self.shared
            .get(page)?
            .remove_remote(addr, C::unpack_gen(idx))
    }

    #[inline(always)]
    fn local(&self, i: usize) -> &page::Local {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            Tid::<C>::current().as_usize(),
            self.tid,
            "tried to access local data from another thread!"
        );

        &self.local[i]
    }

    fn iter<'a>(&'a self) -> std::slice::Iter<'a, page::Shared<T, C>> {
        self.shared.iter()
    }
}

impl<T: fmt::Debug, C: cfg::Config> fmt::Debug for Slab<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slab")
            .field("shards", &self.shards)
            .field("Config", &C::debug())
            .finish()
    }
}

unsafe impl<T: Send, C: cfg::Config> Send for Slab<T, C> {}
unsafe impl<T: Sync, C: cfg::Config> Sync for Slab<T, C> {}

impl<T: fmt::Debug, C: cfg::Config> fmt::Debug for Shard<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Shard");

        #[cfg(debug_assertions)]
        d.field("tid", &self.tid);
        d.field("shared", &self.shared).finish()
    }
}

pub(crate) trait Pack<C: cfg::Config>: Sized {
    const LEN: usize;

    const BITS: usize;
    const SHIFT: usize = Self::Prev::SHIFT + Self::Prev::LEN;
    const MASK: usize = Self::BITS << Self::SHIFT;

    type Prev: Pack<C>;

    fn as_usize(&self) -> usize;
    fn from_usize(val: usize) -> Self;

    #[inline(always)]
    fn pack(&self, to: usize) -> usize {
        let value = self.as_usize();
        debug_assert!(value <= Self::BITS);

        (to & !Self::MASK) | (value << Self::SHIFT)
    }

    #[inline(always)]
    fn from_packed(from: usize) -> Self {
        let value = (from & Self::MASK) >> Self::SHIFT;
        debug_assert!(value <= Self::BITS);
        Self::from_usize(value)
    }
}

impl<C: cfg::Config> Pack<C> for () {
    const BITS: usize = 0;
    const LEN: usize = 0;
    const SHIFT: usize = 0;
    const MASK: usize = 0;

    type Prev = ();

    fn as_usize(&self) -> usize {
        unreachable!()
    }
    fn from_usize(_val: usize) -> Self {
        unreachable!()
    }

    fn pack(&self, _to: usize) -> usize {
        unreachable!()
    }

    fn from_packed(_from: usize) -> Self {
        unreachable!()
    }
}

#[cfg(test)]
mod tests;
