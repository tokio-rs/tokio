use crate::time::driver::{TimerHandle, TimerShared};
use crate::time::error::InsertError;

mod level;
pub(crate) use self::level::Expiration;
use self::level::Level;

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
    levels: Vec<Level>,
}

/// Number of levels. Each level has 64 slots. By using 6 levels with 64 slots
/// each, the timer is able to track time up to 2 years into the future with a
/// precision of 1 millisecond.
const NUM_LEVELS: usize = 6;

/// The maximum duration of a `Sleep`
const MAX_DURATION: u64 = (1 << (6 * NUM_LEVELS)) - 1;

impl Wheel {
    /// Create a new timing wheel
    pub(crate) fn new() -> Wheel {
        let levels = (0..NUM_LEVELS).map(Level::new).collect();

        Wheel { elapsed: 0, levels }
    }

    /// Return the number of milliseconds that have elapsed since the timing
    /// wheel's creation.
    pub(crate) fn elapsed(&self) -> u64 {
        self.elapsed
    }

    /// Insert an entry into the timing wheel.
    ///
    /// # Arguments
    ///
    /// * `item`: The item to insert into the wheel.
    ///
    /// # Return
    ///
    /// Returns `Ok` when the item is successfully inserted, `Err` otherwise.
    ///
    /// `Err(Elapsed)` indicates that `when` represents an instant that has
    /// already passed. In this case, the caller should fire the timeout
    /// immediately.
    ///
    /// `Err(Invalid)` indicates an invalid `when` argument as been supplied.
    ///
    /// # Safety
    ///
    /// This function registers item into an intrusive linked list. The caller
    /// must ensure that `item` is pinned and will not be dropped without first
    /// being deregistered.
    pub(crate) unsafe fn insert(
        &mut self,
        item: TimerHandle,
    ) -> Result<u64, (TimerHandle, InsertError)> {
        let when = item.sync_when();

        if when <= self.elapsed {
            return Err((item, InsertError::Elapsed));
        } else if when - self.elapsed > MAX_DURATION {
            return Err((item, InsertError::Invalid));
        }

        // Get the level at which the entry should be stored
        let level = self.level_for(when);

        unsafe {
            self.levels[level].add_entry(item);
        }

        debug_assert!({
            self.levels[level]
                .next_expiration(self.elapsed)
                .map(|e| e.deadline >= self.elapsed)
                .unwrap_or(true)
        });

        Ok(when)
    }

    /// Remove `item` from the timing wheel.
    pub(crate) unsafe fn remove(&mut self, item: NonNull<TimerShared>) {
        let when = unsafe { item.as_ref() }.cached_when();
        let level = self.level_for(when);

        unsafe {
            self.levels[level].remove_entry(item);
        }
    }

    /// Instant at which to poll
    pub(crate) fn poll_at(&self) -> Option<u64> {
        self.next_expiration().map(|expiration| expiration.deadline)
    }

    /// Advances the timer up to the instant represented by `now`.
    pub(crate) fn poll(&mut self, now: u64) -> Option<TimerHandle> {
        loop {
            // under what circumstances is poll.expiration Some vs. None?
            let expiration = self.next_expiration().and_then(|expiration| {
                if expiration.deadline > now {
                    None
                } else {
                    Some(expiration)
                }
            });

            match expiration {
                Some(ref expiration) => {
                    if let Some(item) = self.poll_expiration(expiration) {
                        return Some(item);
                    }

                    self.set_elapsed(expiration.deadline);
                }
                None => {
                    // in this case the poll did not indicate an expiration
                    // _and_ we were not able to find a next expiration in
                    // the current list of timers.  advance to the poll's
                    // current time and do nothing else.
                    self.set_elapsed(now);
                    return None;
                }
            }
        }
    }

    /// Returns the instant at which the next timeout expires.
    fn next_expiration(&self) -> Option<Expiration> {
        // Check all levels
        for level in 0..NUM_LEVELS {
            if let Some(expiration) = self.levels[level].next_expiration(self.elapsed) {
                // There cannot be any expirations at a higher level that happen
                // before this one.
                debug_assert!(self.no_expirations_before(level + 1, expiration.deadline));

                return Some(expiration);
            }
        }

        None
    }

    /// Used for debug assertions
    fn no_expirations_before(&self, start_level: usize, before: u64) -> bool {
        let mut res = true;

        for l2 in start_level..NUM_LEVELS {
            if let Some(e2) = self.levels[l2].next_expiration(self.elapsed) {
                if e2.deadline < before {
                    res = false;
                }
            }
        }

        res
    }

    /// iteratively find entries that are between the wheel's current
    /// time and the expiration time.  for each in that population either
    /// return it for notification (in the case of the last level) or tier
    /// it down to the next level (in all other cases).
    pub(crate) fn poll_expiration(&mut self, expiration: &Expiration) -> Option<TimerHandle> {
        while let Some(item) = self.pop_entry(expiration) {
            if expiration.level == 0 {
                debug_assert_eq!(unsafe { item.cached_when() }, expiration.deadline);
            }

            // The expiration time may have been reset, so sync up and
            // potentially reinsert at a different time than before
            let sync_when = unsafe { item.sync_when() };

            if sync_when <= expiration.deadline {
                return Some(item);
            } else {
                let level = level_for(expiration.deadline, sync_when);
                unsafe {
                    self.levels[level].add_entry(item);
                }
            }
        }

        None
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

    /// Pops an entry based on the expiration provided.
    ///
    fn pop_entry(&mut self, expiration: &Expiration) -> Option<TimerHandle> {
        self.levels[expiration.level].pop_entry_slot(expiration.slot)
    }

    fn level_for(&self, when: u64) -> usize {
        level_for(self.elapsed, when)
    }
}

fn level_for(elapsed: u64, when: u64) -> usize {
    let masked = elapsed ^ when;

    assert!(masked != 0, "elapsed={}; when={}", elapsed, when);

    let leading_zeros = masked.leading_zeros() as usize;
    let significant = 63 - leading_zeros;
    significant / 6
}

#[cfg(all(test, not(loom)))]
mod test {
    use super::*;

    #[test]
    fn test_level_for() {
        for pos in 1..64 {
            assert_eq!(
                0,
                level_for(0, pos),
                "level_for({}) -- binary = {:b}",
                pos,
                pos
            );
        }

        for level in 1..5 {
            for pos in level..64 {
                let a = pos * 64_usize.pow(level as u32);
                assert_eq!(
                    level,
                    level_for(0, a as u64),
                    "level_for({}) -- binary = {:b}",
                    a,
                    a
                );

                if pos > level {
                    let a = a - 1;
                    assert_eq!(
                        level,
                        level_for(0, a as u64),
                        "level_for({}) -- binary = {:b}",
                        a,
                        a
                    );
                }

                if pos < 64 {
                    let a = a + 1;
                    assert_eq!(
                        level,
                        level_for(0, a as u64),
                        "level_for({}) -- binary = {:b}",
                        a,
                        a
                    );
                }
            }
        }
    }
}
