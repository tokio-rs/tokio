//! The BWoS queue is a fast block-based work stealing queue for parallel processing.
//!
//! The BWoS queue is based on the [BBQ] (Block-based Bounded Queue) and is specially designed for the
//! workstealing scenario. Based on the real-world observation that the "stealing" operation is
//! rare and most of the operations are local enqueues and dequeues this queue implementation
//! offers a single [Owner] which can enqueue and dequeue without any heavy synchronization mechanisms
//! on the fast path. Concurrent stealing is possible and does not slow done the Owner too much.
//! This allows stealing policies which steal single items or in small batches.
//!
//! # Queue Semantics
//!
//! - The block-based design reduces the synchronization requirements on the fast-path
//!   inside a block and moves the heavy synchronization operations necessary to support
//!   multiple stealers to the slow-path when transitioning to the next block.
//! - The producer (enqueue) may not advance to the next block if the consumer or a stealer
//!   is still operating on that block. This allows the producer to remove producer-consumer/stealer
//!   synchronization from its fast-path operations, but reduces the queue capacity by
//!   at most one block.
//! - Stealers may not steal from the same block as the consumer. This allows the consumer
//!   to remove consumer-stealer synchronization from its fast-path operations, but means
//!   one block is not available for stealing.
//! - Consumers may "take-over" the next block preventing stealers from stealing in that
//!   block after the take-over. Stealers will still proceed with already in-progress steal
//!   operations in this block.
//! - This queue implementation puts the producer and consumer into a shared Owner struct,
//!
//! [BBQ]: https://www.usenix.org/conference/atc22/presentation/wang-jiawei
//!
//! # Todo:
//! - Instead of const generics we could use a boxed slice for a dynamically sized array.
//!   The performance impact be benchmarked though, since this will result in multiple operations
//!   not being able to be calculated at compile-time anymore.

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(unreachable_pub)]

use core::{
    marker::{Send, Sync},
    pin::Pin,
};
use std::fmt::Formatter;
use std::mem::MaybeUninit;

mod bwos_queue;
mod metadata;

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{
    AtomicUsize,
    Ordering::{Acquire, Relaxed, Release},
};
use crate::loom::sync::Arc;
use crate::util::cache_padded::CachePadded;
use bwos_queue::{Block, BwsQueue};
use metadata::{Index, IndexAndVersion};

/// The Owner interface to the BWoS queue
///
/// The owner is both the single producer and single consumer.
pub(crate) struct Owner<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> {
    /// Producer cache (single producer）- points to block in self.queue.
    pcache: CachePadded<*const Block<E, { ENTRIES_PER_BLOCK }>>,
    /// Consumer cache (single consumer) - points to block in self.queue.
    ccache: CachePadded<*const Block<E, { ENTRIES_PER_BLOCK }>>,
    /// `Arc` to the actual queue to ensure the queue lives at least as long as the Owner.
    #[allow(dead_code)]
    queue: Pin<Arc<BwsQueue<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>>>,
}

/// A Stealer interface to the BWoS queue
///
/// There may be multiple stealers. Stealers share the stealer position which is used to quickly look up
/// the next block for attempted stealing.
pub(crate) struct Stealer<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> {
    /// The actual stealer position is `self.spos % NUM_BLOCKS`. The position is incremented beyond
    /// `NUM_BLOCKS` to detect ABA problems.
    spos: CachePadded<Arc<AtomicUsize>>,
    queue: Pin<Arc<BwsQueue<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>>>,
}

/// An iterator over elements of one Block.
///
/// The iterator borrows all elements up to `committed` to allows batched
/// operations on the elements. When the iterator is dropped the entries
/// are marked as consumed in one atomic operation.
pub(crate) struct BlockIter<'a, E, const ENTRIES_PER_BLOCK: usize> {
    buffer: &'a [UnsafeCell<MaybeUninit<E>>; ENTRIES_PER_BLOCK],
    /// Index if the next to be consumed entry in the buffer.
    i: usize,
    /// Number of committed entries in the buffer.
    committed: usize,
}

/// An iterator over elements of one Block of a stealer
///
/// Marks the stolen entries as stolen once the iterator has been consumed.
pub(crate) struct StealerBlockIter<'a, E, const ENTRIES_PER_BLOCK: usize> {
    /// Stealer Block
    stealer_block: &'a Block<E, ENTRIES_PER_BLOCK>,
    /// Remember how many entries where reserved for the Drop implementation
    num_reserved: usize,
    /// reserved index of the block. We own the entries from `i..block_reserved`
    block_reserved: usize,
    /// curr index in the block
    i: usize,
}

unsafe impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> Send
    for Owner<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
}

// todo: is this really needed?
unsafe impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> Sync
    for Owner<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
}

unsafe impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> Send
    for Stealer<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
}

unsafe impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> Sync
    for Stealer<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
}

impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>
    Owner<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
    /// Try to enqueue `t` into the FIFO queue.
    ///
    /// If the queue is full, `Err(t)` is returned to the caller.
    #[inline(always)]
    pub(crate) fn enqueue(&mut self, t: E) -> Result<(), E> {
        loop {
            // SAFETY: `pcache` always points to a valid `Block` in the queue. We never create a mutable reference
            // to a Block, so it is safe to construct a shared reference here.
            let blk = unsafe { &**self.pcache };

            // Load the index of the next free queue entry for the producer. `committed` is only written to by the
            // single producer, so `Relaxed` reading is fine.
            let committed = blk.committed.load(Relaxed);
            let committed_idx = committed.raw_index();

            // Fastpath (the block is not full): Due to the slowpath checks we know that the entire remaining block
            // is available to the producer and do not need to check the consumed index in the fastpath.
            if let Some(entry_cell) = blk.entries.get(committed_idx) {
                // SAFETY: We checked the entry is available for writing and the index can be
                // post-incremented unconditionally since `index == NE` is valid and means the block
                // is full.
                let committed_new = unsafe {
                    entry_cell.with_mut(|uninit_entry| uninit_entry.write(MaybeUninit::new(t)));
                    committed.index_add_unchecked(1)
                };
                // Synchronizes with `Acquire` ordering on the stealer side.
                blk.committed.store(committed_new, Release);
                #[cfg(feature = "stats")]
                self.queue.stats.increment_enqueued(1);
                return Ok(());
            }

            /* slow path, move to the next block */
            let nblk = unsafe { &*blk.next() };
            let next = committed.next_version(nblk.is_head());

            /* check if next block is ready */
            if !self.is_next_block_writable(nblk, next.version()) {
                return Err(t);
            };

            /* reset cursor and advance block */
            nblk.committed.store(next, Relaxed);
            nblk.stolen.store(next, Relaxed);
            // Ensures the writes to `committed` and `stolen` are visible when `reserved` is loaded.
            nblk.reserved.store(next, Release);
            *self.pcache = nblk;
        }
    }

    /// Enqueue a batch of items without capacity checks
    ///
    /// # Safety
    ///
    /// The caller must ensure that the queue has sufficient remaining capacity
    /// to enqueue all items in the iterator.
    pub(crate) unsafe fn enqueue_batch_unchecked(
        &mut self,
        mut iter: Box<dyn ExactSizeIterator<Item = E> + '_>,
    ) {
        loop {
            if iter.len() == 0 {
                return;
            }
            // SAFETY: `pcache` always points to a valid `Block` in the queue. We never create a mutable reference
            // to a Block, so it is safe to construct a shared reference here.
            let blk = unsafe { &**self.pcache };

            // Load the index of the next free queue entry for the producer. `committed` is only written to by the
            // single producer, so `Relaxed` reading is fine.
            let committed = blk.committed.load(Relaxed);
            let start_idx = committed.raw_index();
            let end = core::cmp::min(ENTRIES_PER_BLOCK, start_idx.wrapping_add(iter.len()));
            if start_idx < ENTRIES_PER_BLOCK {
                let count = end - start_idx;
                for idx in start_idx..end {
                    // Fastpath (the block is not full): Due to the slowpath checks we know that the entire remaining block
                    // is available to the producer and do not need to check the consumed index in the
                    // fastpath.
                    let entry = iter.next().expect("Iterator magically lost an item");
                    blk.entries[idx].with_mut(|uninit_entry| unsafe {
                        uninit_entry.write(MaybeUninit::new(entry))
                    });
                }
                // SAFETY: We checked that the addition will not overflow beyond ENTRIES_PER_BLOCK
                let new_committed = unsafe { committed.index_add_unchecked(count) };
                blk.committed.store(new_committed, Release);
                #[cfg(feature = "stats")]
                self.queue.stats.increment_enqueued(count);
                if end < ENTRIES_PER_BLOCK {
                    continue;
                }
            }
            /* slow path, move to the next block */
            let nblk = unsafe { &*blk.next() };
            let next = committed.next_version(nblk.is_head());

            // The caller promises they already confirmed the next block is ready, so we only
            // debug assert.
            debug_assert!(
                self.is_next_block_writable(nblk, next.version()),
                "Precondition of unchecked enqueue function violated."
            );

            /* reset cursor and advance block */
            nblk.committed.store(next, Relaxed);
            nblk.stolen.store(next, Relaxed);
            // The changes to `committed` and `stolen` must be visible when reserved is changed.
            nblk.reserved.store(next, Release);
            *self.pcache = nblk;
        }
    }
    /// true if the next block is ready for the producer to start writing.
    fn is_next_block_writable(
        &self,
        next_blk: &Block<E, ENTRIES_PER_BLOCK>,
        next_block_version: usize,
    ) -> bool {
        let expected_version = next_block_version.wrapping_sub(1);
        let consumed = next_blk.consumed.load(Relaxed);
        let is_consumed = consumed.index().is_full() && expected_version == consumed.version();

        // The next block must be already _fully_ consumed, since we do not want to checked the `consumed` index
        // in the enqueue fastpath!
        if !is_consumed {
            return false;
        }
        // The producer must wait until the next block has no active stealers.
        let stolen = next_blk.stolen.load(Acquire);
        if !stolen.index().is_full() || stolen.version() != expected_version {
            return false;
        }
        true
    }
}

impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>
    Owner<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
    /// Try to dequeue the oldest element in the queue.
    #[inline(always)]
    pub(crate) fn dequeue(&mut self) -> Option<E> {
        let (blk, consumed) = self.get_consumer_block()?;

        // We trust that the correct index is passed to us here.
        let entry_cell = &blk.entries[consumed.raw_index()];
        // SAFETY: We know there is an entry to dequeue, so we know the entry is a valid initialized `E`.
        let item = unsafe { entry_cell.with(|entry| entry.read().assume_init()) };
        // SAFETY: We already checked that `consumed_idx < ENTRIES_PER_BLOCK`.
        let new_consumed = unsafe { consumed.index_add_unchecked(1) };
        blk.consumed.store(new_consumed, Relaxed);
        #[cfg(feature = "stats")]
        self.queue.stats.increment_dequeued(1);
        Some(item)
    }

    /// Try to dequeue all remaining committed entries in the current block.
    pub(crate) fn dequeue_block(&mut self) -> Option<BlockIter<'_, E, ENTRIES_PER_BLOCK>> {
        let (blk, consumed) = self.get_consumer_block()?;

        let committed = blk.committed.load(Relaxed);

        // We are claiming the tasks **before** reading them out of the buffer.
        // This is safe because only the **current** thread is able to push new
        // tasks.
        //
        // There isn't really any need for memory ordering... Relaxed would
        // work. This is because all tasks are pushed into the queue from the
        // current thread (or memory has been acquired if the local queue handle
        // moved).
        blk.consumed.store(committed, Relaxed);

        Some(BlockIter {
            buffer: &blk.entries,
            i: consumed.raw_index(),
            committed: committed.raw_index(),
        })
    }

    // returns true on success, false when advancing not possible.
    fn try_advance_consumer_block(
        &mut self,
        next_block: &Block<E, ENTRIES_PER_BLOCK>,
        curr_consumed: IndexAndVersion<ENTRIES_PER_BLOCK>,
    ) -> bool {
        let next_cons_vsn = curr_consumed
            .version()
            .wrapping_add(next_block.is_head() as usize);

        // The reserved field is updated last in `enqueue()`. It is only updated by the producer
        // (`Owner`), so `Relaxed` is sufficient. If the actual reserved version is not equal to the
        // expected next consumer version, then the producer has not advanced to the next block yet
        // and we must wait.
        let next_reserved_vsn = next_block.reserved.load(Relaxed).version();
        if next_reserved_vsn != next_cons_vsn {
            debug_assert!(next_reserved_vsn == next_cons_vsn.wrapping_sub(1));
            return false;
        }

        /* stop stealers */
        let reserved_new = IndexAndVersion::new(next_cons_vsn, Index::full());
        // todo: Why can this be Relaxed?
        let reserved_old = next_block.reserved.swap(reserved_new, Relaxed);
        debug_assert_eq!(reserved_old.version(), next_cons_vsn);
        let reserved_old_idx = reserved_old.raw_index();

        // Number of entries that can't be stolen anymore because we stopped stealing.
        let num_consumer_owned = ENTRIES_PER_BLOCK.saturating_sub(reserved_old_idx);
        // Increase `stolen`, by the number of entries that can't be stolen anymore and are now up to the
        // consumer to deqeuue. This ensures that, once the stealers have finished stealing the already reserved
        // entries, `nblk.stolen == ENTRIES_PER_BLOCK` holds, i.e. this block is marked as having no active
        // stealers, which will allow the producer to the enter this block again (in the next round).
        next_block.stolen.fetch_add(num_consumer_owned, Relaxed);

        /* advance the block and try again */
        // The consumer must skip already reserved entries.
        next_block.consumed.store(reserved_old, Relaxed);
        *self.ccache = next_block;
        true
    }

    /// Advance consumer to the next block, unless the producer has not reached the block yet.
    fn can_advance_consumer_block(
        &self,
        next_block: &Block<E, ENTRIES_PER_BLOCK>,
        curr_consumed: IndexAndVersion<ENTRIES_PER_BLOCK>,
    ) -> bool {
        let next_cons_vsn = curr_consumed
            .version()
            .wrapping_add(next_block.is_head() as usize);
        // The reserved field is updated last in `enqueue()`. It is only updated by the producer
        // (`Owner`), so `Relaxed` is sufficient. If the actual reserved version is not equal to the
        // expected next consumer version, then the producer has not advanced to the next block yet
        // and we must wait.
        let next_reserved_vsn = next_block.reserved.load(Relaxed).version();
        if next_reserved_vsn != next_cons_vsn {
            debug_assert!(next_reserved_vsn == next_cons_vsn.wrapping_sub(1));
            return false;
        }
        true
    }

    /// Check if there are any entries in the next block that are currently being stolen.
    pub(crate) fn next_block_has_stealers(&self) -> bool {
        // SAFETY: `pcache` always points to a valid `Block` in the queue. We never create a mutable reference
        // to a Block, so it is safe to construct a shared reference here.
        let blk = unsafe { &**self.pcache };
        let reserved = blk.reserved.load(Relaxed);
        let stolen = blk.stolen.load(Relaxed);
        // If reserved and stolen don't match then there is still an active stealer in the block.
        stolen != reserved
    }

    /// Check if there are entries that can be stolen from the queue.
    ///
    /// Note that stealing may still fail for a number of reasons even if this function returned true
    pub(crate) fn has_stealable_entries(&self) -> bool {
        // If the consumer is not on the same block as the producer, then there
        // is at least the producer block that can be stolen from.
        if self.pcache == self.ccache {
            return false;
        }

        // SAFETY: `pcache` always points to a valid `Block` in the queue. We never create a mutable reference
        // to a Block, so it is safe to construct a shared reference here.
        let blk = unsafe { &**self.pcache };
        let committed = blk.committed.load(Relaxed);
        // Probably we could get away with Relaxed here too, since a false positive
        // isn't that bad since users anyway have to expect concurrent stealing.
        let reserved = blk.reserved.load(Acquire);
        // For the current FIFO stealing policy it is sufficient to only check
        // if all items in the producer block have been reserved yet.
        reserved != committed
    }

    /// Returns the minimum amount of slots that are free and can be enqueued.
    ///
    /// For performance reasons this implementation will only check the current and next
    /// block. This is sufficient for stealing policies which steal at most 1 block at a
    /// time.
    pub(crate) fn min_remaining_slots(&self) -> usize {
        // SAFETY: self.pcache always points to a valid Block.
        let current_block: &Block<E, ENTRIES_PER_BLOCK> = unsafe { &*(*self.pcache) };
        // The committed field is only updated by the owner.
        let committed_idx = current_block.committed.load(Relaxed).raw_index();
        // Free slots in the current block ( 0 <= free_slots <= ENTRIES_PER_BLOCK)
        let mut free_slots = ENTRIES_PER_BLOCK - committed_idx;

        // SAFETY: The next pointer is always valid
        let next_block = unsafe { &*current_block.next() };
        let committed = next_block.committed.load(Relaxed);
        if self.is_next_block_writable(next_block, committed.version()) {
            free_slots += ENTRIES_PER_BLOCK;
        }
        free_slots
    }

    /// `true` if there is at least one entry that can be dequeued.
    ///
    /// It is possible that a dequeue can still fail, since the item was stolen after we checked
    /// and before the consumer advanced to the block in question.
    pub(crate) fn can_consume(&self) -> bool {
        // SAFETY: `ccache` always points to a valid `Block` in the queue. We never create a mutable reference
        // to a Block, so it is safe to construct a shared reference here.
        let current_blk_cache = unsafe { &**self.ccache };
        let mut blk = current_blk_cache;
        for _ in 0..NUM_BLOCKS + 1 {
            // check if the block is fully consumed already
            let consumed = blk.consumed.load(Relaxed);
            let consumed_idx = consumed.raw_index();

            // Fastpath (Block is not fully consumed yet)
            if consumed_idx < ENTRIES_PER_BLOCK {
                // we know the block is not full, but we must first check if there is an entry to
                // dequeue.
                let committed_idx = blk.committed.load(Relaxed).raw_index();
                if consumed_idx == committed_idx {
                    return false;
                }

                /* There is an entry to dequeue */
                return true;
            }

            /* Slow-path */

            /* Consumer head may never pass the Producer head and Consumer/Stealer tail */
            let nblk = unsafe { &*blk.next() };
            if self.can_advance_consumer_block(nblk, consumed) {
                blk = nblk;
            } else {
                return false;
            }
            /* We advanced to the next block - loop around and try again */
        }
        // Since there is no concurrent enqueuing and the buffer is bounded, we should reach
        // one of the exit conditions in at most NUM_BLOCKS iterations.
        unreachable!()
    }

    fn get_consumer_block(
        &mut self,
    ) -> Option<(
        &Block<E, ENTRIES_PER_BLOCK>,
        IndexAndVersion<ENTRIES_PER_BLOCK>,
    )> {
        // SAFETY: `ccache` always points to a valid `Block` in the queue. We never create a mutable reference
        // to a Block, so it is safe to construct a shared reference here.
        let current_blk_cache = unsafe { &**self.ccache };
        let mut blk = current_blk_cache;
        // The +1 is necessary to advance again to our original starting block, this time with a
        // new version. This can happen in the edge-case that all items in the queue where stolen.
        for _ in 0..NUM_BLOCKS + 1 {
            // check if the block is fully consumed already
            let consumed = blk.consumed.load(Relaxed);
            let consumed_idx = consumed.raw_index();

            // Fastpath (Block is not fully consumed yet)
            if consumed_idx < ENTRIES_PER_BLOCK {
                // we know the block is not full, but we must first check if there is an entry to
                // dequeue.
                let committed_idx = blk.committed.load(Relaxed).raw_index();
                if consumed_idx == committed_idx {
                    return None;
                }

                /* There is an entry to dequeue */
                return Some((blk, consumed));
            }

            /* Slow-path */

            /* Consumer head may never pass the Producer head and Consumer/Stealer tail */
            let nblk = unsafe { &*blk.next() };
            if self.try_advance_consumer_block(nblk, consumed) {
                blk = nblk;
            } else {
                return None;
            }
            /* We advanced to the next block - loop around and try again */
        }
        // Since there is no concurrent enqueuing and the buffer is bounded, we should reach
        // one of the exit conditions in at most NUM_BLOCKS+1 iterations.
        unreachable!()
    }
}

impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize> Clone
    for Stealer<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
    fn clone(&self) -> Self {
        Self {
            spos: self.spos.clone(),
            queue: self.queue.clone(),
        }
    }
}

impl<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>
    Stealer<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>
{
    /// Try to steal a single item from the queue
    #[inline]
    pub(crate) fn steal(&self) -> Option<E> {
        loop {
            let (blk, curr_spos) = self.curr_block();

            /* check if the block is fully reserved */
            let reserved = blk.reserved.load(Acquire);
            let reserved_idx = reserved.raw_index();

            if reserved_idx < ENTRIES_PER_BLOCK {
                /* check if we have an entry to occupy */
                let committed = blk.committed.load(Acquire);
                let committed_idx = committed.raw_index();
                if reserved_idx == committed_idx {
                    return None;
                }
                // SAFETY: We checked before that `reserved_idx` < ENTRIES_PER_BLOCK, so the index
                // can't overflow.
                let new_reserved = unsafe { reserved.index_add_unchecked(1) };
                let _ = blk
                    .reserved
                    .compare_exchange_weak(reserved, new_reserved, Release, Relaxed)
                    .ok()?;

                /* we got the entry */

                #[cfg(feature = "stats")]
                self.queue.stats.increment_stolen(1);

                // SAFETY: We know the entry is a valid and initialized `E` and is now exclusively owned by us.
                let t =
                    unsafe { blk.entries[reserved_idx].with(|entry| entry.read().assume_init()) };
                // `t` is now owned by us so we mark the stealing as finished. Synchronizes with the Owner Acquire.
                let old_stolen = blk.stolen.fetch_add(1, Release);
                debug_assert!(old_stolen.raw_index() < ENTRIES_PER_BLOCK);
                return Some(t);
            }

            // Slow-path: The current block is already fully reserved. Try to advance to the next block
            if !self.can_advance(blk, reserved) {
                return None;
            }
            self.try_advance_spos(curr_spos);
        }
    }

    /// Get the current stealer `Block` and the corresponding stealer position (`spos`)
    ///
    /// The returned `spos` can be larger than `NUM_BLOCKS` to detect [ABA](https://en.wikipedia.org/wiki/ABA_problem)
    /// situations.
    fn curr_block(&self) -> (&Block<E, ENTRIES_PER_BLOCK>, usize) {
        let curr_spos = self.spos.load(Relaxed);
        // spos increments beyond NUM_BLOCKS to prevent ABA problems.
        let block_idx = curr_spos % NUM_BLOCKS;
        let blk: &Block<E, ENTRIES_PER_BLOCK> = &self.queue.blocks[block_idx];
        (blk, curr_spos)
    }

    /// Try to steal a block from `self`.
    ///
    /// Tries to steal a full block from `self`. If the block is not fully
    /// committed yet it will steal up to and including the last committed entry
    /// of that block.
    #[inline]
    pub(crate) fn steal_block(&self) -> Option<StealerBlockIter<'_, E, ENTRIES_PER_BLOCK>> {
        loop {
            let (blk, curr_spos) = self.curr_block();

            /* check if the block is fully reserved */
            let reserved = blk.reserved.load(Acquire);
            let reserved_idx = reserved.raw_index();

            if reserved_idx < ENTRIES_PER_BLOCK {
                /* check if we have an entry to occupy */
                let committed = blk.committed.load(Acquire);
                let committed_idx = committed.raw_index();
                if reserved_idx == committed_idx {
                    return None;
                }

                // Try to steal the block up to the latest committed entry
                let reserve_res = blk
                    .reserved
                    .compare_exchange_weak(reserved, committed, Release, Relaxed);

                if reserve_res.is_err() {
                    return None;
                }

                let num_reserved = committed_idx - reserved_idx;
                // From the statistics perspective we consider the reserved range to already be
                // stolen, since it is not available for the consumer or other stealers anymore.
                #[cfg(feature = "stats")]
                self.queue.stats.increment_stolen(num_reserved);
                return Some(StealerBlockIter {
                    stealer_block: blk,
                    block_reserved: committed_idx,
                    i: reserved_idx,
                    num_reserved,
                });
            }

            // Slow-path: The current block is already fully reserved. Try to advance to next block
            if !self.can_advance(blk, reserved) {
                return None;
            }
            self.try_advance_spos(curr_spos);
        }
    }

    /// `true` if there is at least one entry that can be stolen
    ///
    /// Note: Calling this function is expensive! Try to avoid it.
    /// This is intended solely for the case where the worker owning the queue
    /// is parked, and we need to check if there is work remaining in their queue.
    pub(crate) fn is_empty(&self) -> bool {
        let spos = self.spos.load(Acquire);
        for i in 0..NUM_BLOCKS + 1 {
            let blk = &self.queue.blocks[spos.wrapping_add(i) % NUM_BLOCKS];
            // check if the block is fully consumed already
            let consumed = blk.reserved.load(Acquire);
            let consumed_idx = consumed.raw_index();

            // Fastpath (Block is not fully consumed yet)
            if consumed_idx < ENTRIES_PER_BLOCK {
                // we know the block is not full, but we must first check if there is an entry to
                // dequeue.
                let committed_idx = blk.committed.load(Acquire).raw_index();
                if consumed_idx == committed_idx {
                    return false;
                }

                /* There is an entry to dequeue */
                return true;
            }

            /* Advance to next block */

            if !self.can_advance(blk, consumed) {
                return false;
            }
        }
        // Since there is no concurrent enqueuing and the buffer is bounded, we should reach
        // one of the exit conditions in at most NUM_BLOCKS iterations.
        unreachable!()
    }

    /// True if the stealer can advance to the next block
    fn can_advance(
        &self,
        curr_block: &Block<E, ENTRIES_PER_BLOCK>,
        curr_reserved: IndexAndVersion<ENTRIES_PER_BLOCK>,
    ) -> bool {
        /* r_head never pass the w_head and r_tail */
        let nblk = unsafe { &*curr_block.next() };
        let next_expect_vsn = curr_reserved.version() + nblk.is_head() as usize;
        let next_actual_vsn = nblk.reserved.load(Relaxed).version();
        next_expect_vsn == next_actual_vsn
    }

    /// Try and advance `spos` to the next block.
    ///
    /// We are not interested in the failure case, since the next stealer can just try again.
    fn try_advance_spos(&self, curr_spos: usize) {
        // Ignore result. Failure means a different stealer succeeded in updating
        // the stealer block index. In case of a sporadic failure the next stealer will try again.
        let _ =
            self.spos
                .compare_exchange_weak(curr_spos, curr_spos.wrapping_add(1), Relaxed, Relaxed);
    }

    /// The estimated number of entries currently enqueued.
    #[cfg(feature = "stats")]
    pub(crate) fn estimated_queue_entries(&self) -> usize {
        self.queue.estimated_len()
    }
}

impl<'a, E, const ENTRIES_PER_BLOCK: usize> Iterator for BlockIter<'a, E, ENTRIES_PER_BLOCK> {
    type Item = E;

    #[inline]
    fn next(&mut self) -> Option<E> {
        let i = self.i;
        self.i += 1;
        if i < self.committed {
            self.buffer.get(i).map(|entry_cell| {
                entry_cell.with(|entry| {
                    // SAFETY: we claimed the entries
                    unsafe { entry.read().assume_init() }
                })
            })
        } else {
            None
        }
    }
}

impl<'a, E, const ENTRIES_PER_BLOCK: usize> Iterator
    for StealerBlockIter<'a, E, ENTRIES_PER_BLOCK>
{
    type Item = E;

    #[inline]
    fn next(&mut self) -> Option<E> {
        if self.i < self.block_reserved {
            let entry = self.stealer_block.entries[self.i].with(|entry| {
                // SAFETY: we claimed the entries
                unsafe { entry.read().assume_init() }
            });
            self.i += 1;
            Some(entry)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<'a, E, const ENTRIES_PER_BLOCK: usize> ExactSizeIterator
    for StealerBlockIter<'a, E, ENTRIES_PER_BLOCK>
{
}

impl<'a, E, const ENTRIES_PER_BLOCK: usize> Drop for StealerBlockIter<'a, E, ENTRIES_PER_BLOCK> {
    fn drop(&mut self) {
        // Ensure `Drop` is called on any items that where not consumed, by consuming the iterator,
        // which implicitly dequeues all items
        while self.next().is_some() {}
        self.stealer_block
            .stolen
            .fetch_add(self.num_reserved, Release);
    }
}

impl<'a, E, const ENTRIES_PER_BLOCK: usize> StealerBlockIter<'a, E, ENTRIES_PER_BLOCK> {
    pub(crate) fn len(&self) -> usize {
        self.block_reserved - self.i
    }
}

impl<'a, E, const ENTRIES_PER_BLOCK: usize> core::fmt::Debug
    for StealerBlockIter<'a, E, ENTRIES_PER_BLOCK>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "StealerBlockIter over {} entries",
            self.block_reserved - self.i
        ))
    }
}

/// Create a new BWoS queue and return the [Owner] and a [Stealer] instance
///
/// `NUM_BLOCKS` must be a power two and at least 2. `ENTRIES_PER_BLOCK` can be freely chosen (non-zero).
/// The total length of the queue is `NUM_BLOCKS * ENTRIES_PER_BLOCK` and must not be more than `usize::MAX`.
///
/// ## Performance considerations
///
/// The Owner throughput will improve with a larger `ENTRIES_PER_BLOCK` value.
/// Thieves however will prefer a higher `NUM_BLOCKS` count since it makes it easier to
/// steal a whole block.
pub(crate) fn new<E, const NUM_BLOCKS: usize, const ENTRIES_PER_BLOCK: usize>() -> (
    Owner<E, { NUM_BLOCKS }, { ENTRIES_PER_BLOCK }>,
    Stealer<E, { NUM_BLOCKS }, { ENTRIES_PER_BLOCK }>,
) {
    assert!(NUM_BLOCKS.checked_mul(ENTRIES_PER_BLOCK).is_some());
    assert!(NUM_BLOCKS.is_power_of_two());
    assert!(NUM_BLOCKS >= 1);
    assert!(ENTRIES_PER_BLOCK >= 1);

    let q: Pin<Arc<BwsQueue<E, NUM_BLOCKS, ENTRIES_PER_BLOCK>>> = BwsQueue::new();
    let first_block = &q.blocks[0];

    let stealer_position = Arc::new(AtomicUsize::new(0));

    (
        Owner {
            pcache: CachePadded::new(first_block),
            ccache: CachePadded::new(first_block),
            queue: q.clone(),
        },
        Stealer {
            spos: CachePadded::new(stealer_position),
            queue: q,
        },
    )
}
