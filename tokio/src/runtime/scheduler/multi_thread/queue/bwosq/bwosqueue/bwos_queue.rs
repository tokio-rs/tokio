use super::metadata::AtomicIndexAndVersion;
use crate::loom::{cell::UnsafeCell, sync::Arc};
use crate::util::array_init;
use crate::util::cache_padded::CachePadded;
use core::{marker::PhantomPinned, mem::MaybeUninit, pin::Pin, ptr::null};

#[cfg(feature = "stats")]
mod bwsstats {
    use crate::loom::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    use crate::util::cache_padded::CachePadded;

    pub(crate) struct BwsStats {
        owner_counter: CachePadded<AtomicUsize>,
        total_stolen: CachePadded<AtomicUsize>,
    }

    impl BwsStats {
        pub(crate) const fn new() -> Self {
            Self {
                owner_counter: CachePadded::new(AtomicUsize::new(0)),
                total_stolen: CachePadded::new(AtomicUsize::new(0)),
            }
        }

        #[inline]
        pub(crate) fn increment_enqueued(&self, rhs: usize) {
            let curr = self.owner_counter.load(Relaxed);
            let new = curr.wrapping_add(rhs);
            self.owner_counter.store(new, Relaxed);
        }
        #[inline]
        pub(crate) fn increment_dequeued(&self, rhs: usize) {
            let curr = self.owner_counter.load(Relaxed);
            let new = curr.wrapping_sub(rhs);
            self.owner_counter.store(new, Relaxed);
        }

        #[inline]
        pub(crate) fn increment_stolen(&self, rhs: usize) {
            self.total_stolen.fetch_add(rhs, Relaxed);
        }

        /// Returns the _estimated_ number of currently enqueued items.
        ///
        ///
        #[inline]
        pub(crate) fn curr_enqueued(&self) -> usize {
            let owner_cnt = self.owner_counter.load(Relaxed);
            let total_stolen = self.total_stolen.load(Relaxed);

            // We assume the `u64` total numbers will never overflow.
            let num = owner_cnt.saturating_sub(total_stolen);
            num
        }
    }
}

#[cfg(feature = "stats")]
pub(crate) use bwsstats::*;

pub(crate) struct BwsQueue<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> {
    pub(crate) blocks: CachePadded<[Block<E, { ENTRIES_PER_BLOCK }>; NUM_BLOCKS]>,
    #[cfg(feature = "stats")]
    pub(crate) stats: CachePadded<BwsStats>,
    _pin: PhantomPinned,
}

pub(crate) struct Block<E, const NE: usize> {
    /// The index and version of the next writable entry in the block
    ///
    /// index == NE signals that the producer has already fully written this block.
    /// `committed` is only written to by the single producer ([Owner](super::Owner)).
    pub(crate) committed: CachePadded<AtomicIndexAndVersion<{ NE }>>,
    /// The index and version of the next readable entry in the block
    ///
    /// If consumed == committed, then there are not items that can be read in this block.
    /// `consumed` is only written by the single consumer ([Owner](super::Owner)).
    pub(crate) consumed: CachePadded<AtomicIndexAndVersion<{ NE }>>,
    /// stealer-head - We ensure that consumer and stealer are never on same block
    pub(crate) reserved: CachePadded<AtomicIndexAndVersion<{ NE }>>,
    /// stealer-tail - stealing finished
    pub(crate) stolen: CachePadded<AtomicIndexAndVersion<{ NE }>>,
    /// Block specific configuration, including a reference to the next block in the bwosqueue.
    conf: CachePadded<BlockConfig<E, { NE }>>,
    /// The storage for all entries in this block
    pub(crate) entries: CachePadded<[UnsafeCell<MaybeUninit<E>>; NE]>,
}

struct BlockConfig<E, const NE: usize> {
    /// true if this Block is the HEAD of the queue.
    beginning: bool,
    /// Blocks are linked together as a linked list via the `next` pointer to speed up accessing
    /// the next block. The pointer is fixed, but needs to be initialized after the Block has
    /// been put behind a shared reference in pinned memory, since we can't directly initialize
    /// and pin memory on the heap.
    next: UnsafeCell<*const Block<E, { NE }>>,
}

impl<E, const NE: usize> BlockConfig<E, { NE }> {
    fn new(idx: usize) -> BlockConfig<E, NE> {
        BlockConfig {
            beginning: idx == 0,
            next: UnsafeCell::new(null()),
        }
    }
}

impl<E, const NE: usize> Block<E, { NE }> {
    fn new(idx: usize) -> Block<E, NE> {
        let is_queue_head = idx == 0;
        Block {
            committed: CachePadded::new(AtomicIndexAndVersion::new_owner(is_queue_head)),
            consumed: CachePadded::new(AtomicIndexAndVersion::new_owner(is_queue_head)),
            reserved: CachePadded::new(AtomicIndexAndVersion::new_stealer(is_queue_head)),
            stolen: CachePadded::new(AtomicIndexAndVersion::new_stealer(is_queue_head)),
            conf: CachePadded::new(BlockConfig::new(idx)),
            entries: CachePadded::new(array_init(|_| UnsafeCell::new(MaybeUninit::uninit()))),
        }
    }

    /// Returns the next Block in the BWoS queue
    #[inline(always)]
    pub(crate) fn next(&self) -> *const Self {
        // SAFETY: The next pointer is static and valid after initialization of the queue for
        // the whole lifetime of the queue.
        unsafe { self.conf.next.with(|next| *next) }
    }

    /// true if this block is the head of the BWoS queue
    #[inline(always)]
    pub(crate) fn is_head(&self) -> bool {
        self.conf.beginning
    }
}

impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>
    BwsQueue<E, { NUM_BLOCKS }, { ENTRIES_PER_BLOCK }>
{
    #[cfg(const_assert)]
    const _ASSERT_NUM_BLOCKS_POW2: () = assert!(NUM_BLOCKS.is_power_of_two());
    #[cfg(const_assert)]
    const _ASSERT_NUM_GREATER_1: () = assert!(NUM_BLOCKS > 1);

    pub(crate) fn new() -> Pin<Arc<Self>> {
        // We need to "use" the assertions here, otherwise the compile-time assertions are ignored.
        #[cfg(const_assert)]
        #[allow(clippy::let_unit_value)]
        let _ = Self::_ASSERT_NUM_BLOCKS_POW2;
        #[cfg(const_assert)]
        #[allow(clippy::let_unit_value)]
        let _ = Self::_ASSERT_NUM_GREATER_1;

        // First create and pin the queue on the heap
        let q = Arc::pin(BwsQueue {
            blocks: CachePadded::new(array_init(|idx| Block::new(idx))),
            #[cfg(feature = "stats")]
            stats: CachePadded::new(BwsStats::new()),
            _pin: PhantomPinned,
        });
        // Now initialize the fast-path pointers
        let blocks: &[Block<E, { ENTRIES_PER_BLOCK }>; NUM_BLOCKS] = &q.blocks;
        for block_window in blocks.windows(2) {
            // Note: This cannot panic since we asserted at compile-time that BwsQueue has at least
            // 2 blocks
            let curr_block = block_window.get(0).expect("INVALID_NUM_BLOCKS");
            let next_block = block_window.get(1).expect("INVALID_NUM_BLOCKS");
            // SAFETY: Since our array of blocks is already behind an `Arc` and `Pin`ned we can't
            // initialize the pointers with safe code, but we do know that at this point in time
            // no concurrent mutable access is possible, since there are no other references.
            unsafe {
                curr_block.conf.next.with_mut(|next_ptr| {
                    (*next_ptr) = next_block;
                });
            }
        }

        let first_block = blocks.first().expect("INVALID_NUM_BLOCKS");
        let last_block = blocks.last().expect("INVALID_NUM_BLOCKS");

        // SAFETY: There are no other active references to the curr and next block and no
        // concurrent access is possible here.
        unsafe {
            last_block.conf.next.with_mut(|next_ptr| {
                (*next_ptr) = first_block;
            });
        }
        // Now all fields in the Queue are initialized correctly
        q
    }

    /// The estimated number of elements currently enqueued.
    ///
    /// Items which are currently being stolen do not count towards the length,
    /// so this method is not suited to determine if the queue is full.
    #[cfg(feature = "stats")]
    pub(crate) fn estimated_len(&self) -> usize {
        self.stats.curr_enqueued()
    }

    // #[cfg(feature = "stats")]
    // pub(crate) fn is_empty(&self) -> bool {
    //     self.estimated_len() == 0
    // }
}
