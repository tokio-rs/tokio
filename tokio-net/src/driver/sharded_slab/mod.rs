//! A lock-free concurrent slab.
mod page;
mod tid;
pub(crate) use tid::Tid;
mod iter;

use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};
use page::{slot, Addr, Page};
use std::fmt;

#[cfg(target_pointer_width = "64")]
const MAX_THREADS: usize = 4096;
#[cfg(target_pointer_width = "32")]
const MAX_THREADS: usize = 2048;

const MAX_PAGES: usize = 16;
const RESERVED_BITS: usize = 5;

/// A sharded, lock-free slab.
pub(crate) struct Slab<T> {
    shards: Box<[CausalCell<Shard<T>>]>,
}

struct Shard<T> {
    tid: usize,
    sz: usize,
    len: AtomicUsize,
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
    pages: Vec<Page<T>>,
}

impl<T> Slab<T> {
    pub(crate) const MAX_BIT: usize = slot::Generation::LEN + slot::Generation::SHIFT;

    /// Returns a new slab.
    pub(crate) fn new() -> Self {
        let mut shards = Vec::with_capacity(MAX_THREADS);
        let mut idx = 0;
        shards.resize_with(MAX_THREADS, || {
            let shard = Shard::new(idx);
            idx += 1;
            CausalCell::new(shard)
        });
        Slab {
            shards: shards.into_boxed_slice(),
        }
    }

    /// Inserts a value into the slab, returning a key that can be used to
    /// access it.
    ///
    /// If this function returns `None`, then the shard for the current thread
    /// is full and no items can be added until some are removed, or the maximum
    /// number of shards has been reached.
    pub(crate) fn insert(&self, value: T) -> Option<usize> {
        let tid = Tid::current();
        self.shards[tid.as_usize()]
            .with_mut(|shard| unsafe {
                // we are guaranteed to only mutate the shard while on its thread.
                (*shard).insert(value)
            })
            .map(|idx| tid.pack(idx))
    }

    /// Removes the value associated with the given key from the slab, returning
    /// it.
    ///
    /// If the slab does not contain a value for that key, `None` is returned
    /// instead.
    pub(crate) fn remove(&self, idx: usize) -> Option<T> {
        let tid = Tid::from_packed(idx);
        if tid.is_current() {
            self.shards[tid.as_usize()].with_mut(|shard| unsafe {
                // only called if this is the current shard
                (*shard).remove_local(idx)
            })
        } else {
            self.shards[tid.as_usize()].with(|shard| unsafe { (*shard).remove_remote(idx) })
        }
    }

    /// Return a reference to the value associated with the given key.
    ///
    /// If the slab does not contain a value for the given key, `None` is
    /// returned instead.
    pub(crate) fn get(&self, key: usize) -> Option<&T> {
        let tid = Tid::from_packed(key);
        self.shards
            .get(tid.as_usize())?
            .with(|shard| unsafe { (*shard).get(key) })
    }

    /// Returns the number of items currently stored in the slab.
    pub(crate) fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.with(|shard| unsafe { (*shard).len() }))
            .sum()
    }

    /// Returns an iterator over all the items in the slab.
    pub(crate) fn unique_iter<'a>(&'a mut self) -> iter::UniqueIter<'a, T> {
        let mut shards = self.shards.iter_mut();
        let shard = shards.next().expect("must be at least 1 shard");
        let mut pages = shard.with(|shard| unsafe { (*shard).iter() });
        let slots = pages.next().expect("must be at least 1 page").iter();
        iter::UniqueIter {
            shards,
            slots,
            pages,
        }
    }
}

impl<T> Shard<T> {
    fn new(tid: usize) -> Self {
        Self {
            tid,
            sz: page::INITIAL_SIZE,
            len: AtomicUsize::new(0),
            pages: vec![Page::new(page::INITIAL_SIZE, 0)],
        }
    }

    fn insert(&mut self, value: T) -> Option<usize> {
        debug_assert_eq!(Tid::current().as_usize(), self.tid);

        let mut value = Some(value);

        // Can we fit the value into an existing page?
        for page in self.pages.iter_mut() {
            if let Some(poff) = page.insert(&mut value) {
                self.len.fetch_add(1, Ordering::Relaxed);
                return Some(poff);
            }
        }

        // If not, can we allocate a new page?
        let pidx = self.pages.len();
        if pidx >= MAX_PAGES {
            // out of pages!
            return None;
        }

        // Add new page
        let sz = page::INITIAL_SIZE * 2usize.pow(pidx as u32);
        let mut page = Page::new(sz, self.sz);
        // Increment the total size of the shard, for the next time a new page
        // allocated.
        self.sz += sz;
        let poff = page.insert(&mut value).expect("new page should be empty");
        self.len.fetch_add(1, Ordering::Relaxed);
        self.pages.push(page);

        Some(poff)
    }

    #[inline]
    fn get(&self, idx: usize) -> Option<&T> {
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
        let addr = Addr::from_packed(idx);
        self.pages.get(addr.index())?.get(addr, idx)
    }

    /// Remove an item on the shard's local thread.
    fn remove_local(&mut self, idx: usize) -> Option<T> {
        debug_assert_eq!(Tid::current().as_usize(), self.tid);
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
        let addr = Addr::from_packed(idx);

        self.pages
            .get_mut(addr.index())?
            .remove_local(addr, slot::Generation::from_packed(idx))
            .map(|item| {
                self.len.fetch_sub(1, Ordering::Relaxed);
                item
            })
    }

    /// Remove an item, while on a different thread from the shard's local thread.
    fn remove_remote(&self, idx: usize) -> Option<T> {
        debug_assert_eq!(Tid::from_packed(idx).as_usize(), self.tid);
        debug_assert!(Tid::current().as_usize() != self.tid);
        let addr = Addr::from_packed(idx);
        self.pages
            .get(addr.index())?
            .remove_remote(addr, slot::Generation::from_packed(idx))
            .map(|item| {
                self.len.fetch_sub(1, Ordering::Relaxed);
                item
            })
    }

    fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    fn iter<'a>(&'a self) -> std::slice::Iter<'a, Page<T>> {
        self.pages.iter()
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
        d.field("sz", &self.sz)
            .field("len", &self.len())
            .field("pages", &self.pages)
            .finish()
    }
}

trait Pack: Sized {
    const LEN: usize;

    const BITS: usize = make_mask(Self::LEN);
    const SHIFT: usize = Self::Prev::SHIFT + Self::Prev::LEN;
    const MASK: usize = Self::BITS << Self::SHIFT;

    type Prev: Pack;

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

const WIDTH: usize = std::mem::size_of::<usize>() * 8;

const fn make_mask(bits: usize) -> usize {
    let shift = 1 << (bits - 1);
    shift | (shift - 1)
}

impl Pack for () {
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
