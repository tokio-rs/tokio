use super::Pack;
use crate::sync::atomic::{spin_loop_hint, AtomicUsize, Ordering};

pub(super) mod slot;
use self::slot::Slot;
use std::fmt;

/// A page address encodes the location of a slot within a shard (the page
/// number and offset within that page) as a single linear value.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Addr {
    addr: usize,
}

pub(super) const INITIAL_SIZE: usize = 32;

const ADDR_INDEX_SHIFT: usize = INITIAL_SIZE.trailing_zeros() as usize + 1;

impl Addr {
    const NULL: usize = Self::BITS + 1;

    pub(super) fn index(self) -> usize {
        // Since every page is twice as large as the previous page, and all page sizes
        // are powers of two, we can determine the page index that contains a given
        // address by shifting the address down by the smallest page size and
        // looking at how many twos places necessary to represent that number,
        // telling us what power of two page size it fits inside of. We can
        // determine the number of twos places by counting the number of leading
        // zeros (unused twos places) in the number's binary representation, and
        // subtracting that count from the total number of bits in a word.
        super::WIDTH - ((self.addr + INITIAL_SIZE) >> ADDR_INDEX_SHIFT).leading_zeros() as usize
    }

    pub(super) fn offset(self) -> usize {
        self.addr
    }
}

impl Pack for Addr {
    const LEN: usize = super::MAX_PAGES + ADDR_INDEX_SHIFT;

    type Prev = ();

    fn as_usize(&self) -> usize {
        self.addr
    }

    fn from_usize(addr: usize) -> Self {
        debug_assert!(addr <= Self::BITS);
        Self { addr }
    }
}

pub(super) type Iter<'a, T> =
    std::iter::FilterMap<std::slice::Iter<'a, Slot<T>>, fn(&'a Slot<T>) -> Option<&'a T>>;

pub(super) struct Page<T> {
    prev_sz: usize,
    remote_head: AtomicUsize,
    local_head: usize,
    slab: Box<[Slot<T>]>,
}

impl<T> Page<T> {
    pub(super) fn new(size: usize, prev_sz: usize) -> Self {
        let mut slab = Vec::with_capacity(size);
        slab.extend((1..size).map(Slot::new));
        slab.push(Slot::new(Addr::NULL));
        Self {
            prev_sz,
            remote_head: AtomicUsize::new(Addr::NULL),
            local_head: 0,
            slab: slab.into_boxed_slice(),
        }
    }

    #[inline]
    pub(super) fn insert(&mut self, t: &mut Option<T>) -> Option<usize> {
        let head = self.local_head;
        // are there any items on the local free list? (fast path)
        let head = if head < self.slab.len() {
            head
        } else {
            // if the local free list is empty, pop all the items on the remote
            // free list onto the local free list.
            self.remote_head.swap(Addr::NULL, Ordering::Acquire)
        };

        // if the head is still null, both the local and remote free lists are
        // empty --- we can't fit any more items on this page.
        if head == Addr::NULL {
            return None;
        }

        let slot = &mut self.slab[head];
        let gen = slot.insert(t);
        self.local_head = slot.next();
        Some(gen.pack(head + self.prev_sz))
    }

    #[inline]
    pub(super) fn get(&self, addr: Addr, idx: usize) -> Option<&T> {
        let offset = addr.offset() - self.prev_sz;
        self.slab
            .get(offset)?
            .get(slot::Generation::from_packed(idx))
    }

    pub(super) fn remove_local(&mut self, addr: Addr, gen: slot::Generation) -> Option<T> {
        let offset = addr.offset() - self.prev_sz;
        let val = self.slab.get(offset)?.remove(gen, self.local_head);
        self.local_head = offset;
        val
    }

    pub(super) fn remove_remote(&self, addr: Addr, gen: slot::Generation) -> Option<T> {
        let offset = addr.offset() - self.prev_sz;
        let next = self.push_remote(offset);

        self.slab.get(offset)?.remove(gen, next)
    }

    pub(super) fn iter(&self) -> Iter<'_, T> {
        self.slab.iter().filter_map(Slot::value)
    }

    #[inline(always)]
    fn push_remote(&self, offset: usize) -> usize {
        loop {
            let next = self.remote_head.load(Ordering::Relaxed);
            let actual = self
                .remote_head
                .compare_and_swap(next, offset, Ordering::Release);
            if actual == next {
                return next;
            }
            spin_loop_hint();
        }
    }
}

impl<T> fmt::Debug for Page<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field(
                "remote_head",
                &format_args!("{:#0x}", &self.remote_head.load(Ordering::Relaxed)),
            )
            .field("local_head", &format_args!("{:#0x}", &self.local_head))
            .field("prev_sz", &self.prev_sz)
            .field("slab", &self.slab)
            .finish()
    }
}

impl fmt::Debug for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Addr")
            .field("addr", &format_args!("{:#0x}", &self.addr))
            .field("index", &self.index())
            .field("offset", &self.offset())
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn addr_roundtrips(pidx in 0usize..Addr::BITS) {
            let addr = Addr::from_usize(pidx);
            let packed = addr.pack(0);
            assert_eq!(addr, Addr::from_packed(packed));
        }
        #[test]
        fn gen_roundtrips(gen in 0usize..slot::Generation::BITS) {
            let gen = slot::Generation::from_usize(gen);
            let packed = gen.pack(0);
            assert_eq!(gen, slot::Generation::from_packed(packed));
        }

        #[test]
        fn page_roundtrips(
            gen in 0usize..slot::Generation::BITS,
            addr in 0usize..Addr::BITS,
        ) {
            let gen = slot::Generation::from_usize(gen);
            let addr = Addr::from_usize(addr);
            let packed = gen.pack(addr.pack(0));
            assert_eq!(addr, Addr::from_packed(packed));
            assert_eq!(gen, slot::Generation::from_packed(packed));
        }
    }
}
