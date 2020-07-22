//! Tracks the location of an entry in a slab.
//!
//! # Index packing
//!
//! A slab index consists of multiple indices packed into a single `usize` value
//! that correspond to different parts of the slab.
//!
//! The least significant `MAX_PAGES + INITIAL_PAGE_SIZE.trailing_zeros() + 1`
//! bits store the address within a shard, starting at 0 for the first slot on
//! the first page. To index a slot within a shard, we first find the index of
//! the page that the address falls on, and then the offset of the slot within
//! that page.
//!
//! Since every page is twice as large as the previous page, and all page sizes
//! are powers of two, we can determine the page index that contains a given
//! address by shifting the address down by the smallest page size and looking
//! at how many twos places necessary to represent that number, telling us what
//! power of two page size it fits inside of. We can determine the number of
//! twos places by counting the number of leading zeros (unused twos places) in
//! the number's binary representation, and subtracting that count from the
//! total number of bits in a word.
//!
//! Once we know what page contains an address, we can subtract the size of all
//! previous pages from the address to determine the offset within the page.
//!
//! After the page address, the next `MAX_THREADS.trailing_zeros() + 1` least
//! significant bits are the thread ID. These are used to index the array of
//! shards to find which shard a slot belongs to. If an entry is being removed
//! and the thread ID of its index matches that of the current thread, we can
//! use the `remove_local` fast path; otherwise, we have to use the synchronized
//! `remove_remote` path.
//!
//! Finally, a generation value is packed into the index. The `RESERVED_BITS`
//! most significant bits are left unused, and the remaining bits between the
//! last bit of the thread ID and the first reserved bit are used to store the
//! generation. The generation is used as part of an atomic read-modify-write
//! loop every time a `ScheduledIo`'s readiness is modified, or when the
//! resource is removed, to guard against the ABA problem.
//!
//! Visualized:
//!
//! ```text
//!     ┌──────────┬───────────────┬──────────────────┬──────────────────────────┐
//!     │ reserved │  generation   │    thread ID     │         address          │
//!     └▲─────────┴▲──────────────┴▲─────────────────┴▲────────────────────────▲┘
//!      │          │               │                  │                        │
//! bits(usize)     │       bits(MAX_THREADS)          │                        0
//!                 │                                  │
//!      bits(usize) - RESERVED       MAX_PAGES + bits(INITIAL_PAGE_SIZE)
//! ```

use crate::util::bit;
use crate::util::slab::{Generation, INITIAL_PAGE_SIZE, MAX_PAGES, MAX_THREADS};

use std::usize;

/// References the location at which an entry is stored in a slab.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct Address(usize);

const PAGE_INDEX_SHIFT: u32 = INITIAL_PAGE_SIZE.trailing_zeros() + 1;

/// Address in the shard
const SLOT: bit::Pack = bit::Pack::least_significant(MAX_PAGES as u32 + PAGE_INDEX_SHIFT);

/// Masks the thread identifier
const THREAD: bit::Pack = SLOT.then(MAX_THREADS.trailing_zeros() + 1);

/// Masks the generation
const GENERATION: bit::Pack = THREAD
    .then(bit::pointer_width().wrapping_sub(RESERVED.width() + THREAD.width() + SLOT.width()));

// Chosen arbitrarily
const RESERVED: bit::Pack = bit::Pack::most_significant(5);

impl Address {
    /// Represents no entry, picked to avoid collision with Mio's internals.
    /// This value should not be passed to mio.
    pub(crate) const NULL: usize = usize::MAX >> 1;

    /// Re-exported by `Generation`.
    pub(super) const GENERATION_WIDTH: u32 = GENERATION.width();

    pub(super) fn new(shard_index: usize, generation: Generation) -> Address {
        let mut repr = 0;

        repr = SLOT.pack(shard_index, repr);
        repr = GENERATION.pack(generation.to_usize(), repr);

        Address(repr)
    }

    /// Convert from a `usize` representation.
    pub(crate) fn from_usize(src: usize) -> Address {
        assert_ne!(src, Self::NULL);

        Address(src)
    }

    /// Convert to a `usize` representation
    pub(crate) fn to_usize(self) -> usize {
        self.0
    }

    pub(crate) fn generation(self) -> Generation {
        Generation::new(GENERATION.unpack(self.0))
    }

    /// Returns the page index
    pub(super) fn page(self) -> usize {
        // Since every page is twice as large as the previous page, and all page
        // sizes are powers of two, we can determine the page index that
        // contains a given address by shifting the address down by the smallest
        // page size and looking at how many twos places necessary to represent
        // that number, telling us what power of two page size it fits inside
        // of. We can determine the number of twos places by counting the number
        // of leading zeros (unused twos places) in the number's binary
        // representation, and subtracting that count from the total number of
        // bits in a word.
        let slot_shifted = (self.slot() + INITIAL_PAGE_SIZE) >> PAGE_INDEX_SHIFT;
        (bit::pointer_width() - slot_shifted.leading_zeros()) as usize
    }

    /// Returns the slot index
    pub(super) fn slot(self) -> usize {
        SLOT.unpack(self.0)
    }
}

#[cfg(test)]
cfg_not_loom! {
    use proptest::proptest;

    #[test]
    fn test_pack_format() {
        assert_eq!(5, RESERVED.width());
        assert_eq!(0b11111, RESERVED.max_value());
    }

    proptest! {
        #[test]
        fn address_roundtrips(
            slot in 0usize..SLOT.max_value(),
            generation in 0usize..Generation::MAX,
        ) {
            let address = Address::new(slot, Generation::new(generation));
            // Round trip
            let address = Address::from_usize(address.to_usize());

            assert_eq!(address.slot(), slot);
            assert_eq!(address.generation().to_usize(), generation);
        }
    }
}
