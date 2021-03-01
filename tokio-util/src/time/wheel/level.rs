use crate::time::wheel::Stack;

use std::fmt;

/// Wheel for a single level in the timer. This wheel contains 64 slots.
pub(crate) struct Level<T> {
    level: usize,

    /// Bit field tracking which slots currently contain entries.
    ///
    /// Using a bit field to track slots that contain entries allows avoiding a
    /// scan to find entries. This field is updated when entries are added or
    /// removed from a slot.
    ///
    /// The least-significant bit represents slot zero.
    occupied: u64,

    /// Slots
    slot: [T; LEVEL_MULT],
}

/// Indicates when a slot must be processed next.
#[derive(Debug)]
pub(crate) struct Expiration {
    /// The level containing the slot.
    pub(crate) level: usize,

    /// The slot index.
    pub(crate) slot: usize,

    /// The instant at which the slot needs to be processed.
    pub(crate) deadline: u64,
}

/// Level multiplier.
///
/// Being a power of 2 is very important.
const LEVEL_MULT: usize = 64;

impl<T: Stack> Level<T> {
    pub(crate) fn new(level: usize) -> Level<T> {
        // Rust's derived implementations for arrays require that the value
        // contained by the array be `Copy`. So, here we have to manually
        // initialize every single slot.
        macro_rules! s {
            () => {
                T::default()
            };
        }

        Level {
            level,
            occupied: 0,
            slot: [
                // It does not look like the necessary traits are
                // derived for [T; 64].
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
                s!(),
            ],
        }
    }

    /// Finds the slot that needs to be processed next and returns the slot and
    /// `Instant` at which this slot must be processed.
    pub(crate) fn next_expiration(&self, now: u64) -> Option<Expiration> {
        // Use the `occupied` bit field to get the index of the next slot that
        // needs to be processed.
        let slot = match self.next_occupied_slot(now) {
            Some(slot) => slot,
            None => return None,
        };

        // From the slot index, calculate the `Instant` at which it needs to be
        // processed. This value *must* be in the future with respect to `now`.

        let level_range = level_range(self.level);
        let slot_range = slot_range(self.level);

        // TODO: This can probably be simplified w/ power of 2 math
        let level_start = now - (now % level_range);
        let deadline = level_start + slot as u64 * slot_range;

        debug_assert!(
            deadline >= now,
            "deadline={}; now={}; level={}; slot={}; occupied={:b}",
            deadline,
            now,
            self.level,
            slot,
            self.occupied
        );

        Some(Expiration {
            level: self.level,
            slot,
            deadline,
        })
    }

    fn next_occupied_slot(&self, now: u64) -> Option<usize> {
        if self.occupied == 0 {
            return None;
        }

        // Get the slot for now using Maths
        let now_slot = (now / slot_range(self.level)) as usize;
        let occupied = self.occupied.rotate_right(now_slot as u32);
        let zeros = occupied.trailing_zeros() as usize;
        let slot = (zeros + now_slot) % 64;

        Some(slot)
    }

    pub(crate) fn add_entry(&mut self, when: u64, item: T::Owned, store: &mut T::Store) {
        let slot = slot_for(when, self.level);

        self.slot[slot].push(item, store);
        self.occupied |= occupied_bit(slot);
    }

    pub(crate) fn remove_entry(&mut self, when: u64, item: &T::Borrowed, store: &mut T::Store) {
        let slot = slot_for(when, self.level);

        self.slot[slot].remove(item, store);

        if self.slot[slot].is_empty() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            // Unset the bit
            self.occupied ^= occupied_bit(slot);
        }
    }

    pub(crate) fn pop_entry_slot(&mut self, slot: usize, store: &mut T::Store) -> Option<T::Owned> {
        let ret = self.slot[slot].pop(store);

        if ret.is_some() && self.slot[slot].is_empty() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            self.occupied ^= occupied_bit(slot);
        }

        ret
    }
}

impl<T> fmt::Debug for Level<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Level")
            .field("occupied", &self.occupied)
            .finish()
    }
}

fn occupied_bit(slot: usize) -> u64 {
    1 << slot
}

fn slot_range(level: usize) -> u64 {
    LEVEL_MULT.pow(level as u32) as u64
}

fn level_range(level: usize) -> u64 {
    LEVEL_MULT as u64 * slot_range(level)
}

/// Convert a duration (milliseconds) and a level to a slot position
fn slot_for(duration: u64, level: usize) -> usize {
    ((duration >> (level * 6)) % LEVEL_MULT as u64) as usize
}

#[cfg(all(test, not(loom)))]
mod test {
    use super::*;

    #[test]
    fn test_slot_for() {
        for pos in 0..64 {
            assert_eq!(pos as usize, slot_for(pos, 0));
        }

        for level in 1..5 {
            for pos in level..64 {
                let a = pos * 64_usize.pow(level as u32);
                assert_eq!(pos as usize, slot_for(a as u64, level));
            }
        }
    }
}
