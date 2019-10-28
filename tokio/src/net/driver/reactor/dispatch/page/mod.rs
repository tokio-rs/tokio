use super::{Pack, INITIAL_PAGE_SIZE, WIDTH};
use crate::loom::CausalCell;

pub(crate) mod scheduled_io;
mod stack;
pub(crate) use self::scheduled_io::ScheduledIo;
use self::stack::TransferStack;
use std::fmt;

/// A page address encodes the location of a slot within a shard (the page
/// number and offset within that page) as a single linear value.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct Addr {
    addr: usize,
}

impl Addr {
    const NULL: usize = Self::BITS + 1;
    const INDEX_SHIFT: usize = INITIAL_PAGE_SIZE.trailing_zeros() as usize + 1;

    pub(crate) fn index(self) -> usize {
        // Since every page is twice as large as the previous page, and all page sizes
        // are powers of two, we can determine the page index that contains a given
        // address by shifting the address down by the smallest page size and
        // looking at how many twos places necessary to represent that number,
        // telling us what power of two page size it fits inside of. We can
        // determine the number of twos places by counting the number of leading
        // zeros (unused twos places) in the number's binary representation, and
        // subtracting that count from the total number of bits in a word.
        WIDTH - ((self.addr + INITIAL_PAGE_SIZE) >> Self::INDEX_SHIFT).leading_zeros() as usize
    }

    pub(crate) fn offset(self) -> usize {
        self.addr
    }
}

pub(super) fn size(n: usize) -> usize {
    INITIAL_PAGE_SIZE * 2usize.pow(n as _)
}

impl Pack for Addr {
    const LEN: usize = super::MAX_PAGES + Self::INDEX_SHIFT;

    type Prev = ();

    fn as_usize(&self) -> usize {
        self.addr
    }

    fn from_usize(addr: usize) -> Self {
        debug_assert!(addr <= Self::BITS);
        Self { addr }
    }
}

pub(in crate::net::driver) type Iter<'a> = std::slice::Iter<'a, ScheduledIo>;

pub(crate) struct Local {
    head: CausalCell<usize>,
}

pub(crate) struct Shared {
    remote: TransferStack,
    size: usize,
    prev_sz: usize,
    slab: CausalCell<Option<Box<[ScheduledIo]>>>,
}

impl Local {
    pub(crate) fn new() -> Self {
        Self {
            head: CausalCell::new(0),
        }
    }

    #[inline(always)]
    fn head(&self) -> usize {
        self.head.with(|head| unsafe { *head })
    }

    #[inline(always)]
    fn set_head(&self, new_head: usize) {
        self.head.with_mut(|head| unsafe {
            *head = new_head;
        })
    }
}

impl Shared {
    const NULL: usize = Addr::NULL;

    pub(crate) fn new(size: usize, prev_sz: usize) -> Self {
        Self {
            prev_sz,
            size,
            remote: TransferStack::new(),
            slab: CausalCell::new(None),
        }
    }

    /// Allocates storage for this page if it does not allready exist.
    ///
    /// This requires unique access to the page (e.g. it is called from the
    /// thread that owns the page, or, in the case of `SingleShard`, while the
    /// lock is held). In order to indicate this, a reference to the page's
    /// `Local` data is taken by this function; the `Local` argument is not
    /// actually used, but requiring it ensures that this is only called when
    /// local access is held.
    #[cold]
    fn alloc_page(&self, _: &Local) {
        debug_assert!(self.slab.with(|s| unsafe { (*s).is_none() }));

        let mut slab = Vec::with_capacity(self.size);
        slab.extend((1..self.size).map(ScheduledIo::new));
        slab.push(ScheduledIo::new(Self::NULL));
        self.slab.with_mut(|s| {
            // this mut access is safe — it only occurs to initially
            // allocate the page, which only happens on this thread; if the
            // page has not yet been allocated, other threads will not try
            // to access it yet.
            unsafe {
                *s = Some(slab.into_boxed_slice());
            }
        });
    }

    #[inline]
    pub(crate) fn alloc(&self, local: &Local) -> Option<usize> {
        let head = local.head();

        // are there any items on the local free list? (fast path)
        let head = if head < self.size {
            head
        } else {
            // if the local free list is empty, pop all the items on the remote
            // free list onto the local free list.
            self.remote.pop_all()?
        };

        // if the head is still null, both the local and remote free lists are
        // empty --- we can't fit any more items on this page.
        if head == Self::NULL {
            return None;
        }

        // do we need to allocate storage for this page?
        let page_needs_alloc = self.slab.with(|s| unsafe { (*s).is_none() });
        if page_needs_alloc {
            self.alloc_page(local);
        }

        let gen = self.slab.with(|slab| {
            let slab = unsafe { &*(slab) }
                .as_ref()
                .expect("page must have been allocated to alloc!");
            let slot = &slab[head];
            local.set_head(slot.next());
            slot.alloc()
        });

        let index = head + self.prev_sz;
        Some(gen.pack(index))
    }

    #[inline]
    pub(in crate::net::driver) fn get(&self, addr: Addr) -> Option<&ScheduledIo> {
        let page_offset = addr.offset() - self.prev_sz;
        self.slab
            .with(|slab| unsafe { &*slab }.as_ref()?.get(page_offset))
    }

    pub(crate) fn remove_local(&self, local: &Local, addr: Addr, idx: usize) {
        let offset = addr.offset() - self.prev_sz;
        self.slab.with(|slab| {
            let slab = unsafe { &*slab }.as_ref();
            let slot = if let Some(slot) = slab.and_then(|slab| slab.get(offset)) {
                slot
            } else {
                return;
            };
            if slot.reset(scheduled_io::Generation::from_packed(idx)) {
                slot.set_next(local.head());
                local.set_head(offset);
            }
        })
    }

    pub(crate) fn remove_remote(&self, addr: Addr, idx: usize) {
        let offset = addr.offset() - self.prev_sz;
        self.slab.with(|slab| {
            let slab = unsafe { &*slab }.as_ref();
            let slot = if let Some(slot) = slab.and_then(|slab| slab.get(offset)) {
                slot
            } else {
                return;
            };
            if !slot.reset(scheduled_io::Generation::from_packed(idx)) {
                return;
            }
            self.remote.push(offset, |next| slot.set_next(next));
        })
    }

    pub(in crate::net::driver) fn iter(&self) -> Option<Iter<'_>> {
        let slab = self.slab.with(|slab| unsafe { (&*slab).as_ref() });
        slab.map(|slab| slab.iter())
    }
}

impl fmt::Debug for Local {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.head.with(|head| {
            let head = unsafe { *head };
            f.debug_struct("Local")
                .field("head", &format_args!("{:#0x}", head))
                .finish()
        })
    }
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared")
            .field("remote", &self.remote)
            .field("prev_sz", &self.prev_sz)
            .field("size", &self.size)
            // .field("slab", &self.slab)
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

#[cfg(all(test, not(loom)))]
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
    }
}
