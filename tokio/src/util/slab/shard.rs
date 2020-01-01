use crate::util::slab::{page, Address, Entry, MAX_PAGES};

use std::fmt;

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

impl<T: Entry> Shard<T> {
    pub(super) fn new() -> Shard<T> {
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

        Shard { local, shared }
    }

    pub(super) fn alloc(&self) -> Option<Address> {
        // Can we fit the value into an existing page?
        for (page_idx, page) in self.shared.iter().enumerate() {
            let local = self.local(page_idx);

            if let Some(page_offset) = page.alloc(local) {
                return Some(page_offset);
            }
        }

        None
    }

    pub(super) fn get(&self, addr: Address) -> Option<&T> {
        let page_idx = addr.page();

        if page_idx > self.shared.len() {
            return None;
        }

        self.shared[page_idx].get(addr)
    }

    /// Remove an item on the shard's local thread.
    pub(super) fn remove_local(&self, addr: Address) {
        let page_idx = addr.page();

        if let Some(page) = self.shared.get(page_idx) {
            page.remove_local(self.local(page_idx), addr);
        }
    }

    /// Remove an item, while on a different thread from the shard's local thread.
    pub(super) fn remove_remote(&self, addr: Address) {
        if let Some(page) = self.shared.get(addr.page()) {
            page.remove_remote(addr);
        }
    }

    fn local(&self, i: usize) -> &page::Local {
        &self.local[i]
    }
}

impl<T> fmt::Debug for Shard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shard")
            .field("shared", &self.shared)
            .finish()
    }
}
