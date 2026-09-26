use crate::runtime::time::{TimerHandle, TimerShared};
use crate::util::linked_list::LinkedList;

use std::{array, fmt, ptr::NonNull};

/// Wheel for a single level in the timer. This wheel contains 64 slots.
pub(crate) struct Level {
    level: usize,

    /// Bit field tracking which slots currently contain entries.
    ///
    /// Using a bit field to track slots that contain entries allows avoiding a
    /// scan to find entries. This field is updated when entries are added or
    /// removed from a slot.
    ///
    /// The least-significant bit represents slot zero.
    occupied: u64,

    /// Slots. We access these via the EntryInner `current_list` as well, so this needs to be an `UnsafeCell`.
    slot: [LinkedList<TimerShared>; LEVEL_MULT],
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
const LEVEL_MULT: usize = 1 << super::BITS_PER_LEVEL;

impl Level {
    pub(crate) fn new(level: usize) -> Level {
        Level {
            level,
            occupied: 0,
            slot: array::from_fn(|_| LinkedList::default()),
        }
    }

    /// Finds the slot that needs to be processed next and returns the slot and
    /// `Instant` at which this slot must be processed.
    pub(crate) fn next_expiration(&self, now: u64) -> Option<Expiration> {
        // Use the `occupied` bit field to get the index of the next slot that
        // needs to be processed.
        let slot = self.next_occupied_slot(now)?;

        // From the slot index, calculate the `Instant` at which it needs to be
        // processed. This value *must* be in the future with respect to `now`.

        let level_range = level_range(self.level);
        let slot_range = slot_range(self.level);

        // Compute the start date of the current level by masking the low bits
        // of `now` (`level_range` is a power of 2).
        let level_start = now & !(level_range - 1);
        let mut deadline = level_start + slot as u64 * slot_range;

        if deadline <= now {
            // A timer is in a slot "prior" to the current time. This can occur
            // because we do not have an infinite hierarchy of timer levels, and
            // eventually a timer scheduled for a very distant time might end up
            // being placed in a slot that is beyond the end of all of the
            // arrays.
            //
            // To deal with this, we first limit timers to being scheduled no
            // more than MAX_DURATION ticks in the future; that is, they're at
            // most one rotation of the top level away. Then, we force timers
            // that logically would go into the top+1 level, to instead go into
            // the top level's slots.
            //
            // What this means is that the top level's slots act as a
            // pseudo-ring buffer, and we rotate around them indefinitely. If we
            // compute a deadline before now, and it's the top level, it
            // therefore means we're actually looking at a slot in the future.
            debug_assert_eq!(self.level, super::NUM_LEVELS - 1);

            deadline += level_range;
        }

        debug_assert!(
            deadline >= now,
            "deadline={:016X}; now={:016X}; level={}; lr={:016X}, sr={:016X}, slot={}; occupied={:b}",
            deadline,
            now,
            self.level,
            level_range,
            slot_range,
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

        // Add the +1 offset for the `now_slot` to ignore the slot that `now` fits in,
        // since it's the farthest timer that could appear from `now`.
        // This is mostly relevant for the top level because it acts as a
        // pseudo-ring buffer: timers that would logically go past the top level are
        // fudged into it by `level_for` and the `MAX_DURATION` cap, so the slot holding
        // `now` can be occupied by an entry that is a whole rotation away.
        // For the lower levels `level_for` always places an entry in a slot other
        // than the one holding `now`, so `now_slot` is always empty there.
        let now_slot = ((now / slot_range(self.level)) % LEVEL_MULT as u64) as usize + 1;
        let occupied = self.occupied.rotate_right(now_slot as u32);
        let zeros = occupied.trailing_zeros() as usize;
        let slot = (zeros + now_slot) % LEVEL_MULT;

        Some(slot)
    }

    pub(crate) unsafe fn add_entry(&mut self, item: TimerHandle) {
        let slot = slot_for(unsafe { item.registered_when() }, self.level);

        self.slot[slot].push_front(item);

        self.occupied |= occupied_bit(slot);
    }

    pub(crate) unsafe fn remove_entry(&mut self, item: NonNull<TimerShared>) {
        let slot = slot_for(unsafe { item.as_ref().registered_when() }, self.level);

        unsafe {
            assert!(
                self.slot[slot].remove(item).is_some(),
                "Attempt to remove item not present in the timing wheel"
            )
        };
        if self.slot[slot].is_empty() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            // Unset the bit
            self.occupied ^= occupied_bit(slot);
        }
    }

    pub(crate) fn take_slot(&mut self, slot: usize) -> LinkedList<TimerShared> {
        self.occupied &= !occupied_bit(slot);

        std::mem::take(&mut self.slot[slot])
    }
}

impl fmt::Debug for Level {
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
    1 << (super::BITS_PER_LEVEL * level)
}

fn level_range(level: usize) -> u64 {
    1 << (super::BITS_PER_LEVEL * (level + 1))
}

/// Converts a duration (milliseconds) and a level to a slot position.
fn slot_for(duration: u64, level: usize) -> usize {
    ((duration >> (level * super::BITS_PER_LEVEL)) % LEVEL_MULT as u64) as usize
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
                assert_eq!(pos, slot_for(a as u64, level));
            }
        }
    }

    fn level_with(level: usize, occupied: u64) -> Level {
        let mut level = Level::new(level);
        level.occupied = occupied;
        level
    }

    #[test]
    fn next_occupied_slot_on_an_empty_level() {
        assert_eq!(Level::new(0).next_occupied_slot(0), None);
        assert_eq!(Level::new(5).next_occupied_slot(1 << 36), None);
    }

    #[test]
    fn next_occupied_slot_of_a_single_slot() {
        // slot 10 of level 0, i.e. tick 10 of every 64-tick window
        let level = level_with(0, 1 << 10);

        assert_eq!(level.next_occupied_slot(0), Some(10));
        assert_eq!(level.next_occupied_slot(9), Some(10));
        // `now` inside the slot itself, and past it
        assert_eq!(level.next_occupied_slot(10), Some(10));
        assert_eq!(level.next_occupied_slot(11), Some(10));
        // `now` past the window: the slot is taken modulo 64
        assert_eq!(level.next_occupied_slot(64 + 3), Some(10));
    }

    #[test]
    fn next_occupied_slot_picks_the_nearest_slot_forward() {
        let level = level_with(0, (1 << 10) | (1 << 40));

        assert_eq!(level.next_occupied_slot(0), Some(10));
        assert_eq!(level.next_occupied_slot(20), Some(40));
        // nothing left ahead in this window, so the scan wraps to slot 10
        assert_eq!(level.next_occupied_slot(41), Some(10));
    }

    #[test]
    fn next_occupied_slot_skips_the_slot_holding_now() {
        // The occurrence of slot 0 in this rotation has already started, so it
        // can only be processed a full `level_range` later. Slot 1 is still
        // ahead of `now` in this rotation and therefore expires first.
        let level = level_with(5, 0b11);

        assert_eq!(level.next_occupied_slot(0), Some(1));
        assert_eq!(level.next_occupied_slot((1 << 30) - 1), Some(1));

        // The same holds for any later slot, not just the adjacent one.
        let level = level_with(5, 1 | (1 << 40));

        assert_eq!(level.next_occupied_slot(0), Some(40));
    }

    #[test]
    fn next_occupied_slot_of_the_slot_holding_now_when_it_is_the_only_one() {
        // Nothing is ahead of `now`, so slot 0 is the earliest expiration even
        // though it is reached only in the next rotation.
        let level = level_with(5, 1);

        assert_eq!(level.next_occupied_slot(0), Some(0));
        assert_eq!(level.next_occupied_slot((1 << 30) - 1), Some(0));
    }

    #[test]
    fn next_occupied_slot_of_the_last_slot() {
        let level = level_with(5, 1 << 63);

        assert_eq!(level.next_occupied_slot(62 << 30), Some(63));
        assert_eq!(level.next_occupied_slot(63 << 30), Some(63));
    }

    #[test]
    fn next_expiration_reports_the_start_of_the_slot() {
        // slot 3 of level 1: slots are 64 ticks wide, so it starts at tick 192
        let expiration = level_with(1, 1 << 3).next_expiration(100).unwrap();

        assert_eq!(expiration.level, 1);
        assert_eq!(expiration.slot, 3);
        assert_eq!(expiration.deadline, 192);
    }

    #[test]
    fn next_expiration_below_the_top_level() {
        let level = level_with(4, 1 << 1);

        assert_eq!(level.next_expiration(0).unwrap().deadline, 1 << 24);
        assert_eq!(level.next_expiration(1000).unwrap().deadline, 1 << 24);
    }

    #[test]
    fn next_expiration_at_the_top_level() {
        let level = level_with(5, 1 << 1);

        assert_eq!(level.next_expiration(0).unwrap().deadline, 1 << 30);
        assert_eq!(level.next_expiration(1000).unwrap().deadline, 1 << 30);
    }

    #[test]
    fn next_expiration_wraps_a_slot_at_or_behind_now() {
        // Slot 0 of the top level starts at tick 0, so its next occurrence is a
        // full rotation of the level away.
        let level = level_with(5, 1 << 0);

        assert_eq!(level.next_expiration(0).unwrap().deadline, 1 << 36);
        assert_eq!(
            level.next_expiration((1 << 30) + 10).unwrap().deadline,
            1 << 36
        );
    }
}
