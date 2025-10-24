mod level;
use crate::runtime::ThreadId;

pub(crate) use self::level::Expiration;
use self::level::Level;

mod entry;
cfg_rt_and_time! {
    pub(crate) use entry::Cancelling as EntryCancelling;
}
pub(crate) use entry::Handle as EntryHandle;
pub(crate) use entry::State as EntryState;
use entry::TransitionToPending;
use entry::TransitionToRegistered;
use entry::{CancellationQueueEntry, Entry, EntryList};

pub(crate) mod cancellation_queue;
use cancellation_queue::Sender;

use std::array;
use std::ptr::NonNull;

/// Timing wheel implementation.
///
/// This type provides the hashed timing wheel implementation that backs `Timer`
/// and `DelayQueue`.
///
/// The structure is generic over `T: Stack`. This allows handling timeout data
/// being stored on the heap or in a slab. In order to support the latter case,
/// the slab must be passed into each function allowing the implementation to
/// lookup timer entries.
///
/// See `Timer` documentation for some implementation notes.
#[derive(Debug)]
pub(crate) struct Wheel {
    /// The number of milliseconds elapsed since the wheel started.
    elapsed: u64,

    /// Timer wheel.
    ///
    /// Levels:
    ///
    /// * 1 ms slots / 64 ms range
    /// * 64 ms slots / ~ 4 sec range
    /// * ~ 4 sec slots / ~ 4 min range
    /// * ~ 4 min slots / ~ 4 hr range
    /// * ~ 4 hr slots / ~ 12 day range
    /// * ~ 12 day slots / ~ 2 yr range
    levels: Box<[Level; NUM_LEVELS]>,

    /// Entries queued for firing
    pending: EntryList,
}

/// Number of levels. Each level has 64 slots. By using 6 levels with 64 slots
/// each, the timer is able to track time up to 2 years into the future with a
/// precision of 1 millisecond.
const NUM_LEVELS: usize = 6;

/// The maximum duration of a `Sleep`.
pub(super) const MAX_DURATION: u64 = (1 << (6 * NUM_LEVELS)) - 1;

impl Wheel {
    /// Creates a new timing wheel.
    pub(crate) fn new() -> Wheel {
        Wheel {
            elapsed: 0,
            levels: Box::new(array::from_fn(Level::new)),
            pending: EntryList::new(),
        }
    }

    /// Returns the number of milliseconds that have elapsed since the timing
    /// wheel's creation.
    pub(crate) fn elapsed(&self) -> u64 {
        self.elapsed
    }

    /// Inserts an entry into the timing wheel.
    ///
    /// # Arguments
    ///
    /// * `hdl`: The entry handle to insert into the wheel.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    ///
    /// * The entry is not already registered in ANY wheel.
    pub(crate) unsafe fn insert(
        &mut self,
        hdl: EntryHandle,
        cancel_tx: Sender,
        thread_id: ThreadId,
    ) -> Insert {
        let deadline = hdl.deadline();

        if deadline <= self.elapsed {
            return Insert::Elapsed;
        }

        // Get the level at which the entry should be stored
        let level = self.level_for(deadline);

        match hdl.transition_to_registered(cancel_tx, thread_id) {
            TransitionToRegistered::Success => {
                unsafe {
                    self.levels[level].add_entry(hdl);
                }

                debug_assert!({
                    self.levels[level]
                        .next_expiration(self.elapsed)
                        .map(|e| e.deadline >= self.elapsed)
                        .unwrap_or(true)
                });

                Insert::Success
            }
            TransitionToRegistered::Cancelling => Insert::Cancelling,
        }
    }

    /// Removes `item` from the timing wheel.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    ///
    /// * The entry is already registered in THIS wheel.
    pub(crate) unsafe fn remove(&mut self, hdl: EntryHandle) {
        if hdl.is_pending() {
            self.pending.remove(NonNull::from(&hdl));
        } else {
            let deadline = hdl.deadline();
            debug_assert!(
                self.elapsed <= deadline,
                "elapsed={}; deadline={}",
                self.elapsed,
                deadline
            );

            let level = self.level_for(deadline);
            self.levels[level].remove_entry(hdl.clone());
        }
    }

    /// Advances the timer up to the instant represented by `now`.
    pub(crate) fn poll(&mut self, now: u64) -> Option<EntryHandle> {
        loop {
            if let Some(hdl) = self.pending.pop_back() {
                return Some(hdl);
            }

            match self.next_expiration() {
                Some(ref expiration) if expiration.deadline <= now => {
                    self.process_expiration(expiration);

                    self.set_elapsed(expiration.deadline);
                }
                _ => {
                    // in this case the poll did not indicate an expiration
                    // _and_ we were not able to find a next expiration in
                    // the current list of timers.  advance to the poll's
                    // current time and do nothing else.
                    self.set_elapsed(now);
                    break;
                }
            }
        }

        self.pending.pop_back()
    }

    /// Returns the instant at which the next timeout expires.
    fn next_expiration(&self) -> Option<Expiration> {
        if !self.pending.is_empty() {
            // Expire immediately as we have things pending firing
            return Some(Expiration {
                level: 0,
                slot: 0,
                deadline: self.elapsed,
            });
        }

        // Check all levels
        for (level_num, level) in self.levels.iter().enumerate() {
            if let Some(expiration) = level.next_expiration(self.elapsed) {
                // There cannot be any expirations at a higher level that happen
                // before this one.
                debug_assert!(self.no_expirations_before(level_num + 1, expiration.deadline));

                return Some(expiration);
            }
        }

        None
    }

    /// Returns the tick at which this timer wheel next needs to perform some
    /// processing, or None if there are no timers registered.
    pub(crate) fn next_expiration_time(&self) -> Option<u64> {
        self.next_expiration().map(|ex| ex.deadline)
    }

    /// Used for debug assertions
    fn no_expirations_before(&self, start_level: usize, before: u64) -> bool {
        let mut res = true;

        for level in &self.levels[start_level..] {
            if let Some(e2) = level.next_expiration(self.elapsed) {
                if e2.deadline < before {
                    res = false;
                }
            }
        }

        res
    }

    /// iteratively find entries that are between the wheel's current
    /// time and the expiration time.  for each in that population either
    /// queue it for notification (in the case of the last level) or tier
    /// it down to the next level (in all other cases).
    pub(crate) fn process_expiration(&mut self, expiration: &Expiration) {
        // Note that we need to take _all_ of the entries off the list before
        // processing any of them. This is important because it's possible that
        // those entries might need to be reinserted into the same slot.
        //
        // This happens only on the highest level, when an entry is inserted
        // more than MAX_DURATION into the future. When this happens, we wrap
        // around, and process some entries a multiple of MAX_DURATION before
        // they actually need to be dropped down a level. We then reinsert them
        // back into the same position; we must make sure we don't then process
        // those entries again or we'll end up in an infinite loop.
        let mut entries = self.take_entries(expiration);

        while let Some(hdl) = entries.pop_back() {
            if expiration.level == 0 {
                debug_assert_eq!(hdl.deadline(), expiration.deadline);
            }

            match hdl.transition_to_pending(expiration.deadline) {
                TransitionToPending::Success => self.pending.push_front(hdl),
                TransitionToPending::NotElapsed(when) => {
                    let level = level_for(expiration.deadline, when);
                    unsafe {
                        self.levels[level].add_entry(hdl);
                    }
                }
                TransitionToPending::Cancelling => {}
            }
        }
    }

    fn set_elapsed(&mut self, when: u64) {
        assert!(
            self.elapsed <= when,
            "elapsed={:?}; when={:?}",
            self.elapsed,
            when
        );

        if when > self.elapsed {
            self.elapsed = when;
        }
    }

    /// Obtains the list of entries that need processing for the given expiration.
    fn take_entries(&mut self, expiration: &Expiration) -> EntryList {
        self.levels[expiration.level].take_slot(expiration.slot)
    }

    fn level_for(&self, when: u64) -> usize {
        level_for(self.elapsed, when)
    }
}

fn level_for(elapsed: u64, when: u64) -> usize {
    const SLOT_MASK: u64 = (1 << 6) - 1;

    // Mask in the trailing bits ignored by the level calculation in order to cap
    // the possible leading zeros
    let mut masked = elapsed ^ when | SLOT_MASK;

    if masked >= MAX_DURATION {
        // Fudge the timer into the top level
        masked = MAX_DURATION - 1;
    }

    let leading_zeros = masked.leading_zeros() as usize;
    let significant = 63 - leading_zeros;

    significant / NUM_LEVELS
}

/// The return type of the [`Wheel::insert`] method.
pub(crate) enum Insert {
    /// The entry was successfully inserted.
    Success,

    /// The entry has already expired, in this case,
    /// the entry is not inserted into the wheel.
    Elapsed,

    /// The entry is being cancelled, no need to register it.
    Cancelling,
}

#[cfg(all(test, not(loom)))]
mod test {
    use super::*;

    #[test]
    fn test_level_for() {
        for pos in 0..64 {
            assert_eq!(0, level_for(0, pos), "level_for({pos}) -- binary = {pos:b}");
        }

        for level in 1..5 {
            for pos in level..64 {
                let a = pos * 64_usize.pow(level as u32);
                assert_eq!(
                    level,
                    level_for(0, a as u64),
                    "level_for({a}) -- binary = {a:b}"
                );

                if pos > level {
                    let a = a - 1;
                    assert_eq!(
                        level,
                        level_for(0, a as u64),
                        "level_for({a}) -- binary = {a:b}"
                    );
                }

                if pos < 64 {
                    let a = a + 1;
                    assert_eq!(
                        level,
                        level_for(0, a as u64),
                        "level_for({a}) -- binary = {a:b}"
                    );
                }
            }
        }
    }
}
