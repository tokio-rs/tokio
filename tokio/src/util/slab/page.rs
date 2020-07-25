use crate::loom::cell::UnsafeCell;
use crate::util::slab::{Address, Entry, Slot, TransferStack, INITIAL_PAGE_SIZE};

use std::fmt;

/// Data accessed only by the thread that owns the shard.
pub(crate) struct Local {
    head: UnsafeCell<usize>,
}

/// Data accessed by any thread.
pub(crate) struct Shared<T> {
    remote: TransferStack,
    size: usize,
    prev_sz: usize,
    slab: UnsafeCell<Option<Box<[Slot<T>]>>>,
}

/// Returns the size of the page at index `n`
pub(super) fn size(n: usize) -> usize {
    INITIAL_PAGE_SIZE << n
}

impl Local {
    pub(crate) fn new() -> Self {
        Self {
            head: UnsafeCell::new(0),
        }
    }

    fn head(&self) -> usize {
        self.head.with(|head| unsafe { *head })
    }

    fn set_head(&self, new_head: usize) {
        self.head.with_mut(|head| unsafe {
            *head = new_head;
        })
    }
}

impl<T: Entry> Shared<T> {
    pub(crate) fn new(size: usize, prev_sz: usize) -> Shared<T> {
        Self {
            prev_sz,
            size,
            remote: TransferStack::new(),
            slab: UnsafeCell::new(None),
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
        slab.extend((1..self.size).map(Slot::new));
        slab.push(Slot::new(Address::NULL));

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

    pub(crate) fn alloc(&self, local: &Local) -> Option<Address> {
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
        if head == Address::NULL {
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
            slot.generation()
        });

        let index = head + self.prev_sz;

        Some(Address::new(index, gen))
    }

    pub(crate) fn get(&self, addr: Address) -> Option<&T> {
        let page_offset = addr.slot() - self.prev_sz;

        self.slab
            .with(|slab| unsafe { &*slab }.as_ref()?.get(page_offset))
            .map(|slot| slot.get())
    }

    pub(crate) fn remove_local(&self, local: &Local, addr: Address) {
        let offset = addr.slot() - self.prev_sz;

        self.slab.with(|slab| {
            let slab = unsafe { &*slab }.as_ref();

            let slot = if let Some(slot) = slab.and_then(|slab| slab.get(offset)) {
                slot
            } else {
                return;
            };

            if slot.reset(addr.generation()) {
                slot.set_next(local.head());
                local.set_head(offset);
            }
        })
    }

    pub(crate) fn remove_remote(&self, addr: Address) {
        let offset = addr.slot() - self.prev_sz;

        self.slab.with(|slab| {
            let slab = unsafe { &*slab }.as_ref();

            let slot = if let Some(slot) = slab.and_then(|slab| slab.get(offset)) {
                slot
            } else {
                return;
            };

            if !slot.reset(addr.generation()) {
                return;
            }

            self.remote.push(offset, |next| slot.set_next(next));
        })
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

impl<T> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared")
            .field("remote", &self.remote)
            .field("prev_sz", &self.prev_sz)
            .field("size", &self.size)
            // .field("slab", &self.slab)
            .finish()
    }
}
