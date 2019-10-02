//! A lock-free concurrent slab.
//!
//! Slabs provide pre-allocated storage for many instances of a single data
//! type. When a large number of values of a single type are required,
//! this can be more efficient than allocating each item individually. Since the
//! allocated items are the same size, memory fragmentation is reduced, and
//! creating and removing new items can be very cheap.
//!
//! This crate implements a lock-free concurrent slab, indexed by `usize`s.
//!
//! # Examples
//!
//! Inserting an item into the slab, returning an index:
//! ```rust
//! # use sharded_slab::Slab;
//! let slab = Slab::new();
//!
//! let key = slab.insert("hello world");
//! assert_eq!(slab.get(key), Some("hello world"));
//! ```
//!
//! # Configuration
//!
//! For performance reasons, several values used by the slab are calculated as
//! constants. In order to allow users to tune the slab's parameters, we provide
//! a [`Config`] trait which defines these parameters as associated `consts`.
//! The `Slab` type is generic over a `C: Config` parameter.
//!
//! [`Config`]: trait.Config.html
//!
//! # Comparison with Similar Crates
//!
//! - [`slab`]: Carl Lerche's `slab` crate provides a slab implementation with a
//!   similar API, implemented by storing all data in a single vector.
//!
//!   Unlike `sharded_slab`, inserting and removing elements from the slab
//!   requires  mutable access. This means that if the slab is accessed
//!   concurrently by multiple threads, it is necessary for it to be protected
//!   by a `Mutex` or `RwLock`. Items may not be inserted or removed (or
//!   accessed, if a `Mutex` is used) concurrently, even when they are
//!   unrelated. In many cases, the lock can become a significant bottleneck. On
//!   the other hand, this crate allows separate indices in the slab to be
//!   accessed, inserted, and removed concurrently without requiring a global
//!   lock. Therefore, when the slab is shared across multiple threads, this
//!   crate offers significantly better performance than `slab`.
//!
//!   However, the lock free slab introduces some additional constant-factor
//!   overhead. This means that in use-cases where a slab is _not_ shared by
//!   multiple threads and locking is not required, this crate will likely offer
//!   slightly worse performance.
//!
//!   In summary: `sharded-slab` offers significantly improved performance in
//!   concurrent use-cases, while `slab` should be preferred in single-threaded
//!   use-cases.
//!
//! [`slab`]: https://crates.io/crates/loom
//!
// //! # Design
// TODO(eliza) write this section
//!
//! # Safety and Correctness
//!
//! Most implementations of lock-free data structures in Rust require some
//! amount of unsafe code, and this crate is not an exception. In order to catch
//! potential bugs in this unsafe code, we make use of [`loom`], a
//! permutation-testing tool for concurrent Rust programs. All `unsafe` blocks
//! this crate occur in accesses to `loom` `CausalCell`s. This means that when
//! those accesses occur in this crate's tests, `loom` will assert that they are
//! valid under the C11 memory model across multiple permutations of concurrent
//! executions of those tests.
//!
//! In order to guard against the [ABA problem][aba], this crate makes use of
//! _generational indices_. Each slot in the slab tracks a generation counter
//! which is incremented every time a value is inserted into that slot, and the
//! indices returned by [`Slab::insert`] include the generation of the slot when
//! the value was inserted, packed into the high-order bits of the index. This
//! ensures that if a value is inserted, removed,  and a new value is inserted
//! into the same slot in the slab, the key returned by the first call to
//! `insert` will not map to the new value.
//!
//! Since a fixed number of bits are set aside to use for storing the generation
//! counter, the counter will wrap  around after being incremented a number of
//! times. To avoid situations where a returned index lives long enough to see the
//! generation counter wrap around to the same value, it is good to be fairly
//! generous when configuring the allocation of index bits.
//!
//! [`loom`]: https://crates.io/crates/loom
//! [aba]: https://en.wikipedia.org/wiki/ABA_problem
//! [`Slab::insert`]: struct.Slab.html#method.insert
//!

mod page;
mod tid;
pub(crate) use tid::Tid;
pub(crate) mod cfg;
mod iter;
use cfg::CfgPrivate;
pub(crate) use cfg::{Config, DefaultConfig};

use crate::sync::{
    atomic::{AtomicUsize, Ordering},
    CausalCell,
};
use page::Page;
use std::{fmt, marker::PhantomData};

/// A sharded slab.
///
/// See the [crate-level documentation](index.html) for details on using this type.
pub(crate) struct Slab<T, C: cfg::Config = DefaultConfig> {
    shards: Box<[CausalCell<Shard<T, C>>]>,
    _cfg: PhantomData<C>,
}

struct Shard<T, C: cfg::Config> {
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
    pages: Vec<Page<T, C>>,
}

impl<T> Slab<T> {
    /// Returns a new slab with the default configuration parameters.
    pub(crate) fn new() -> Self {
        Self::new_with_config()
    }

    /// Returns a new slab with the provided configuration parameters.
    pub(crate) fn new_with_config<C: cfg::Config>() -> Slab<T, C> {
        C::validate();
        let mut shards = Vec::with_capacity(C::MAX_SHARDS);
        let mut idx = 0;
        shards.resize_with(C::MAX_SHARDS, || {
            let shard = Shard::new(idx);
            idx += 1;
            CausalCell::new(shard)
        });
        Slab {
            shards: shards.into_boxed_slice(),
            _cfg: PhantomData,
        }
    }
}

impl<T, C: cfg::Config> Slab<T, C> {
    /// The number of bits in each index which are used by the slab.
    ///
    /// If other data is packed into the `usize` indices returned by
    /// [`Slab::insert`], user code is free to use any bits higher than the
    /// `USED_BITS`-th bit freely.
    ///
    /// This is determined by the [`Config`] type that configures the slab's
    /// parameters. By default, all bits are used; this can be changed by
    /// overriding the [`Config::RESERVED_BITS`][res] constant.
    ///
    /// [`Config`]: trait.Config.html
    /// [res]: trait.Config.html#associatedconstant.RESERVED_BITS
    /// [`Slab::insert`]: struct.Slab.html#method.insert
    pub(crate) const USED_BITS: usize = C::USED_BITS;

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
    /// let key = slab.insert("hello world");
    /// assert_eq!(slab.get(key), Some("hello world"));
    /// ```
    pub(crate) fn insert(&self, value: T) -> Option<usize> {
        let tid = Tid::<C>::current();
        #[cfg(test)]
        println!("insert {:?}", tid);
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
        let tid = C::unpack_tid(idx);
        #[cfg(test)]
        println!("rm {:?}", tid);
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
        self.shards
            .get(tid.as_usize())?
            .with(|shard| unsafe { (*shard).get(key) })
    }

    /// Returns `true` if the slab contains a value for the given key.
    ///
    /// # Examples
    ///
    /// ```
    /// let slab = sharded_slab::Slab::new();
    ///
    /// let key = slab.insert("hello world").unwrap();
    /// assert!(slab.contains(key));
    ///
    /// slab.remove(key).unwrap();
    /// assert!(!slab.contains(key));
    /// ```
    pub(crate) fn contains(&self, key: usize) -> bool {
        self.get(key).is_some()
    }

    /// Returns the number of items currently stored in the slab.
    pub(crate) fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.with(|shard| unsafe { (*shard).len() }))
            .sum()
    }

    /// Returns the current number of items which may be stored in the slab
    /// without allocating.
    pub(crate) fn capacity(&self) -> usize {
        self.total_capacity() - self.len()
    }

    /// Returns an iterator over all the items in the slab.
    pub(crate) fn unique_iter<'a>(&'a mut self) -> iter::UniqueIter<'a, T, C> {
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

    fn total_capacity(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.with(|shard| unsafe { (*shard).total_capacity() }))
            .sum()
    }
}

impl<T, C: cfg::Config> Shard<T, C> {
    fn new(tid: usize) -> Self {
        Self {
            tid,
            sz: C::INITIAL_SZ,
            len: AtomicUsize::new(0),
            pages: vec![Page::new(C::INITIAL_SZ, 0)],
        }
    }

    fn insert(&mut self, value: T) -> Option<usize> {
        debug_assert_eq!(Tid::<C>::current().as_usize(), self.tid);

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
        if pidx >= C::MAX_PAGES {
            #[cfg(test)]
            println!("max pages (len={}, max={})", self.pages.len(), C::MAX_PAGES);
            // out of pages!
            return None;
        }

        // Add new page
        let sz = C::page_size(pidx);
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
        debug_assert_eq!(Tid::<C>::from_packed(idx).as_usize(), self.tid);
        let addr = C::unpack_addr(idx);
        let i = addr.index();
        #[cfg(test)]
        println!("-> {:?}; idx {:?}", addr, i);
        self.pages.get(i)?.get(addr, idx)
    }

    /// Remove an item on the shard's local thread.
    fn remove_local(&mut self, idx: usize) -> Option<T> {
        debug_assert_eq!(Tid::<C>::current().as_usize(), self.tid);
        debug_assert_eq!(Tid::<C>::from_packed(idx).as_usize(), self.tid);
        let addr = C::unpack_addr(idx);

        #[cfg(test)]
        println!("-> remove_local {:?}", addr);
        self.pages
            .get_mut(addr.index())?
            .remove_local(addr, C::unpack_gen(idx))
            .map(|item| {
                self.len.fetch_sub(1, Ordering::Relaxed);
                item
            })
    }

    /// Remove an item, while on a different thread from the shard's local thread.
    fn remove_remote(&self, idx: usize) -> Option<T> {
        debug_assert_eq!(Tid::<C>::from_packed(idx).as_usize(), self.tid);
        debug_assert!(Tid::<C>::current().as_usize() != self.tid);
        let addr = C::unpack_addr(idx);

        #[cfg(test)]
        println!("-> remove_remote {:?}", addr);
        self.pages
            .get(addr.index())?
            .remove_remote(addr, C::unpack_gen(idx))
            .map(|item| {
                self.len.fetch_sub(1, Ordering::Relaxed);
                item
            })
    }

    fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    fn total_capacity(&self) -> usize {
        self.iter().map(Page::total_capacity).sum()
    }

    fn iter<'a>(&'a self) -> std::slice::Iter<'a, Page<T, C>> {
        self.pages.iter()
    }
}

impl<T: fmt::Debug, C: cfg::Config> fmt::Debug for Slab<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Slab")
            // .field("shards", &self.shards)
            .field("Config", &C::debug())
            .finish()
    }
}

unsafe impl<T: Send, C: cfg::Config> Send for Slab<T, C> {}
unsafe impl<T: Sync, C: cfg::Config> Sync for Slab<T, C> {}

impl<T: fmt::Debug, C: cfg::Config> fmt::Debug for Shard<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shard")
            .field("tid", &self.tid)
            .field("len", &self.len())
            .field("pages", &self.pages)
            .finish()
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
