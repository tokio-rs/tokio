#![allow(warnings)]

mod level;
mod stack;

pub(crate) use self::stack::Stack;
pub(crate) use self::level::Expiration;
use self::level::Level;

use std::borrow::Borrow;
use std::usize;

#[derive(Debug)]
pub(crate) struct Wheel<T> {
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
    levels: Vec<Level<T>>,
}

/// Number of levels. Each level has 64 slots. By using 6 levels with 64 slots
/// each, the timer is able to track time up to 2 years into the future with a
/// precision of 1 millisecond.
const NUM_LEVELS: usize = 6;

/// The maximum duration of a delay
const MAX_DURATION: u64 = 1 << (6 * NUM_LEVELS);

/// Maximum number of timeouts the system can handle concurrently.
const MAX_TIMEOUTS: usize = usize::MAX >> 1;

#[derive(Debug)]
pub(crate) enum InsertError {
    Elapsed,
    Invalid,
}

/// Poll expirations from the wheel
#[derive(Debug, Default)]
pub(crate) struct Poll {
    now: u64,
    expiration: Option<Expiration>,
}

impl<T> Wheel<T>
where
    T: Stack,
{
    pub fn new() -> Wheel<T> {
        let levels = (0..NUM_LEVELS)
            .map(Level::new)
            .collect();

        Wheel {
            elapsed: 0,
            levels,
        }
    }

    pub fn elapsed(&self) -> u64 {
        self.elapsed
    }

    pub fn insert(&mut self, when: u64, item: T::Owned, store: &mut T::Store)
        -> Result<(), (T::Owned, InsertError)>
    {
        if when <= self.elapsed {
            return Err((item, InsertError::Elapsed));
        } else if when - self.elapsed > MAX_DURATION {
            return Err((item, InsertError::Invalid));
        }

        // Get the level at which the entry should be stored
        let level = self.level_for(when);

        self.levels[level].add_entry(when, item, store);

        debug_assert!({
            self.levels[level].next_expiration(self.elapsed)
                .map(|e| e.deadline >= self.elapsed)
                .unwrap_or(true)
        });

        Ok(())
    }

    pub fn remove(&mut self, item: &T::Borrowed, store: &mut T::Store) {
        let when = T::when(item, store);
        let level = self.level_for(when);

        self.levels[level].remove_entry(when, item, store);
    }

    /// Instant at which to poll
    pub fn poll_at(&self, poll: &Poll) -> Option<u64> {
        self.next_expiration(poll.now)
            .map(|expiration| expiration.deadline)
    }

    pub fn poll(&mut self, poll: &mut Poll, store: &mut T::Store)
        -> Option<T::Owned>
    {
        loop {
            if poll.expiration.is_none() {
                poll.expiration = self.next_expiration(poll.now)
                    .and_then(|expiration| {
                        if expiration.deadline > poll.now {
                            None
                        } else {
                            Some(expiration)
                        }
                    });
            }

            match poll.expiration {
                Some(ref expiration) => {
                    if let Some(item) = self.poll_expiration(expiration, store) {
                        return Some(item);
                    }

                    self.set_elapsed(expiration.deadline);
                }
                None => {
                    self.set_elapsed(poll.now);
                    return None;
                }
            }

            poll.expiration = None;
        }
    }

    /// Returns the instant at which the next timeout expires.
    fn next_expiration(&self, now: u64) -> Option<Expiration> {
        // Check all levels
        for level in 0..NUM_LEVELS {
            if let Some(expiration) = self.levels[level].next_expiration(self.elapsed) {
                // There cannot be any expirations at a higher level that happen
                // before this one.
                debug_assert!({
                    let mut res = true;

                    for l2 in (level+1)..NUM_LEVELS {
                        if let Some(e2) = self.levels[l2].next_expiration(self.elapsed) {
                            if e2.deadline < expiration.deadline {
                                res = false;
                            }
                        }
                    }

                    res
                });

                return Some(expiration);
            }
        }

        None
    }

    pub fn poll_expiration(&mut self, expiration: &Expiration, store: &mut T::Store)
        -> Option<T::Owned>
    {
        while let Some(item) = self.pop_entry(expiration, store) {
            if expiration.level == 0 {
                debug_assert_eq!(T::when(item.borrow(), store), expiration.deadline);

                return Some(item);
            } else {
                let when = T::when(item.borrow(), store);

                let next_level = expiration.level - 1;

                self.levels[next_level]
                    .add_entry(when, item, store);
            }
        }

        None
    }

    fn set_elapsed(&mut self, when: u64) {
        assert!(self.elapsed <= when, "elapsed={:?}; when={:?}", self.elapsed, when);

        if when > self.elapsed {
            self.elapsed = when;
        }
    }

    fn pop_entry(&mut self, expiration: &Expiration, store: &mut T::Store) -> Option<T::Owned> {
        self.levels[expiration.level].pop_entry_slot(expiration.slot, store)
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

impl Poll {
    pub fn new(now: u64) -> Poll {
        Poll {
            now,
            expiration: None,
        }
    }
}
