use timer::{entry, Entry};

use std::fmt;
use std::sync::Arc;

pub struct Level {
    level: usize,

    /// Tracks which slot entries are occupied.
    occupied: u64,

    /// Slots
    slot: [entry::Stack; LEVEL_MULT],
}

#[derive(Debug)]
pub struct Expiration {
    pub level: usize,
    pub slot: usize,
    pub deadline: u64,
}

/// Level multiplier.
///
/// Being a power of 2 is very important.
const LEVEL_MULT: usize = 64;

impl Level {
    pub fn new(level: usize) -> Level {
        macro_rules! s {
            () => { entry::Stack::new() };
        };

        Level {
            level,
            occupied: 0,
            slot: [
                // It does not look like the necessary traits are
                // derived for [T; 64].
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
                s!(), s!(), s!(), s!(), s!(), s!(), s!(), s!(),
            ],
        }
    }

    /// Returns the instant at which the next timeout expires.
    pub fn next_expiration(&self, now: u64) -> Option<Expiration> {
        let slot = match self.next_occupied_slot(now) {
            Some(slot) => slot,
            None => return None,
        };

        let level_range = level_range(self.level);
        let slot_range = slot_range(self.level);

        // TODO: This can probably be simplified w/ power of 2 math
        let level_start = now - (now % level_range);
        let mut deadline = level_start + slot as u64 * slot_range;

        if deadline < now {
            deadline += level_range;
        }

        debug_assert!(deadline >= now, "deadline={}; now={}; level={}; slot={}; occupied={:b}",
                      deadline, now, self.level, slot, self.occupied);

        Some(Expiration {
            level: self.level,
            slot,
            deadline,
        })
    }

    pub fn slot_for(&self, now: u64) -> Option<usize> {
        let slot_range = slot_range(self.level);

        // TODO: Improve with power of 2 math
        if now % slot_range == 0 {
            return Some((now / slot_range) as usize % LEVEL_MULT);
        }

        None
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

    pub fn add_entry(&mut self, entry: Arc<Entry>, when: u64) {
        let slot = slot_for(when, self.level);

        self.slot[slot].push(entry);
        self.occupied |= occupied_bit(slot);
    }

    pub fn remove_entry(&mut self, entry: &Entry, when: u64) {
        let slot = slot_for(when, self.level);

        self.slot[slot].remove(entry);

        if self.slot[slot].is_empty() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            // Unset the bit
            self.occupied ^= occupied_bit(slot);
        }
    }

    pub fn pop_entry_slot(&mut self, slot: usize) -> Option<Arc<Entry>> {
        let ret = self.slot[slot].pop();

        if ret.is_some() && self.slot[slot].is_empty() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            self.occupied ^= occupied_bit(slot);
        }

        ret
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        while let Some(slot) = self.next_occupied_slot(0) {
            // This should always have one
            let entry = self.pop_entry_slot(slot)
                .expect("occupied bit set invalid");

            entry.error();
        }
    }
}

impl fmt::Debug for Level {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Level")
            .field("occupied", &self.occupied)
            .finish()
    }
}

fn occupied_bit(slot: usize) -> u64 {
    (1 << slot)
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_slot_for() {
        for pos in 1..64 {
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
