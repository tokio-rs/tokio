mod entry;
mod handle;
mod level;
mod now;
mod registration;

use self::entry::Entry;
use self::level::{Level, Expiration};

pub use self::handle::{Handle, with_default};
pub use self::now::{Now, SystemNow};
pub(crate) use self::registration::Registration;

use Error;
use atomic::AtomicU64;

use tokio_executor::park::{Park, Unpark, ParkThread};

use std::{cmp, fmt};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{SeqCst, Relaxed};
use std::usize;

/// Timer implementation that drives [`Sleep`], [`Interval`], and [`Deadline`].
///
/// # Implementation
///
/// `Timer` is based on the [paper by Varghese and Lauck][paper].
///
/// A hashed timing wheel is a vector of slots, where each slot handles a time
/// slice. As time progresses, the timer walks over the slot for the current
/// instant, and processes each entry for that slot. When the timer reaches the
/// end of the wheel, it starts again at the beginning.
///
/// The `Timer` implementation maintains six wheels arranged in a set of levels.
/// As the levels go up, the slots of the associated wheel represent larger
/// intervals of time. At each level, the wheel has 64 slots. Each slot covers a
/// range of time equal to the wheel at the lower level. At level zero, each
/// slot represents one millisecond of time.
///
/// The wheels are:
///
/// * Level 0: 64 x 1 millisecond slots.
/// * Level 1: 64 x 64 millisecond slots.
/// * Level 2: 64 x ~4 second slots.
/// * Level 3: 64 x ~4 minute slots.
/// * Level 4: 64 x ~4 hour slots.
/// * Level 5: 64 x ~12 day slots.
///
/// When the timer processes entries at level zero, it will notify all the
/// [`Sleep`] instances as their deadlines have been reached. For all higher
/// levels, all entries will be redistributed across the wheel at the next level
/// down. Eventually, as time progresses, entries will `Sleep` instances will
/// either be canceled (dropped) or their associated entries will reach level
/// zero and be notified.
///
/// [`Sleep`]: ../struct.Sleep.html
/// [`Interval`]: ../struct.Interval.html
/// [`Deadline`]: ../struct.Deadline.html
/// [paper]: http://www.cs.columbia.edu/~nahum/w6998/papers/ton97-timing-wheels.pdf
#[derive(Debug)]
pub struct Timer<T, N = SystemNow> {
    /// Shared state
    inner: Arc<Inner>,

    /// The number of milliseconds elapsed since the timer started.
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

    /// Thread parker. The `Timer` park implementation delegates to this.
    park: T,

    /// Source of "now" instances
    now: N,
}

/// Return value from the `turn` method on `Timer`.
///
/// Currently this value doesn't actually provide any functionality, but it may
/// in the future give insight into what happened during `turn`.
#[derive(Debug)]
pub struct Turn(());

pub(crate) struct Inner {
    /// The instant at which the timer started running.
    start: Instant,

    /// The last published timer `elapsed` value.
    elapsed: AtomicU64,

    /// Number of active timeouts
    num: AtomicUsize,

    /// Head of the "process" linked list.
    process: entry::AtomicStack,

    /// Unparks the timer thread.
    unpark: Box<Unpark>,
}

/// Number of levels
const NUM_LEVELS: usize = 6;

/// The maximum duration of a sleep
const MAX_DURATION: u64 = 1 << (6 * NUM_LEVELS);

/// Maximum number of timeouts the system can handle concurrently.
const MAX_TIMEOUTS: usize = usize::MAX >> 1;

// ===== impl Timer =====

impl<T> Timer<T>
where T: Park
{
    pub fn new(park: T) -> Self {
        Timer::new_with_now(park, SystemNow::new())
    }
}

impl<T, N> Timer<T, N> {
    /// Returns a reference to the underlying `Park` instance.
    pub fn get_ref(&self) -> &T {
        &self.park
    }

    /// Returns a mutable reference to the underlying `Park` instance.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.park
    }
}

impl<T, N> Timer<T, N>
where T: Park,
      N: Now,
{
    pub fn new_with_now(park: T, mut now: N) -> Self {
        let unpark = Box::new(park.unpark());

        let levels = (0..NUM_LEVELS)
            .map(Level::new)
            .collect();

        Timer {
            inner: Arc::new(Inner::new(now.now(), unpark)),
            elapsed: 0,
            levels,
            park,
            now,
        }
    }

    /// Returns a handle to the timer
    pub fn handle(&self) -> Handle {
        Handle::new(Arc::downgrade(&self.inner))
    }

    /// Performs one iteration of the timer loop.
    pub fn turn(&mut self, max_wait: Option<Duration>) -> Result<Turn, T::Error> {
        match max_wait {
            Some(timeout) => self.park_timeout(timeout)?,
            None => self.park()?,
        }

        Ok(Turn(()))
    }

    /// Returns the instant at which the next timeout expires.
    fn next_expiration(&self) -> Option<Expiration> {
        // Check all levels
        for level in 0..NUM_LEVELS {
            if let Some(expiration) = self.levels[level].next_expiration(self.elapsed) {
                return Some(expiration);
            }
        }

        None
    }

    /// Converts an `Expiration` to an `Instant`.
    fn expiration_instant(&self, expiration: &Expiration) -> Instant {
        self.inner.start + Duration::from_millis(expiration.deadline)
    }

    /// Run timer related logic
    fn process(&mut self) {
        let now = ms(self.now.now() - self.inner.start, Round::Down);

        loop {
            let expiration = match self.next_expiration() {
                Some(expiration) => expiration,
                None => break,
            };

            if expiration.deadline > now {
                // This expiration should not fire on this tick
                break;
            }

            // Prcess the slot, either moving it down a level or firing the
            // timeout if currently at the final (boss) level.
            self.process_expiration(&expiration);

            self.set_elapsed(expiration.deadline);
        }

        self.set_elapsed(now);
    }

    fn set_elapsed(&mut self, when: u64) {
        assert!(self.elapsed <= when, "elapsed={:?}; when={:?}", self.elapsed, when);

        if when > self.elapsed {
            self.elapsed = when;
            self.inner.elapsed.store(when, Relaxed);
        } else {
            assert_eq!(self.elapsed, when);
        }
    }

    fn process_expiration(&mut self, expiration: &Expiration) {
        while let Some(entry) = self.pop_entry(expiration) {
            if expiration.level == 0 {
                entry.fire();
            } else {
                let when = entry.deadline_ms(self.inner.start);
                let next_level = expiration.level - 1;

                debug_assert!({
                    self.levels[next_level].next_expiration(self.elapsed)
                        .map(|e| e.deadline >= self.elapsed)
                        .unwrap_or(true)
                });

                self.levels[next_level]
                    .add_entry(entry, when);

                debug_assert!({
                    self.levels[next_level].next_expiration(self.elapsed)
                        .map(|e| e.deadline >= self.elapsed)
                        .unwrap_or(true)
                });
            }
        }
    }

    fn pop_entry(&mut self, expiration: &Expiration) -> Option<Arc<Entry>> {
        self.levels[expiration.level].pop_entry_slot(expiration.slot)
    }

    /// Process the entry queue
    ///
    /// This handles adding and canceling timeouts.
    fn process_queue(&mut self) {
        for entry in self.inner.process.take() {
            // Check the entry state
            if entry.is_elapsed() {
                self.clear_entry(entry);
            } else {
                self.add_entry(entry);
            }
        }
    }

    fn clear_entry(&mut self, entry: Arc<Entry>) {
        let when = entry.deadline_ms(self.inner.start);

        if when <= self.elapsed {
            // The entry is no longer contained by the timer.
            return;
        }

        // Get the level at which the entry should be stored
        let level = level_for(when - self.elapsed);

        self.levels[level].remove_entry(&entry, when);
    }

    fn add_entry(&mut self, entry: Arc<Entry>) {
        // Avoid an underflow subtraction
        if entry.deadline() < self.inner.start {
            entry.fire();
            return;
        }

        // Convert the duration to millis
        let when = entry.deadline_ms(self.inner.start);
        debug_assert!(entry.deadline() <= self.inner.start + Duration::from_millis(when));

        if when > MAX_DURATION {
            entry.error();
            return;
        }

        // If the entry's deadline is in the past or present, trigger it.
        if when <= self.elapsed {
            entry.fire();
            return;
        }

        // Get the level at which the entry should be stored
        let level = level_for(when - self.elapsed);

        self.levels[level].add_entry(entry, when);

        debug_assert!({
            self.levels[level].next_expiration(self.elapsed)
                .map(|e| e.deadline >= self.elapsed)
                .unwrap_or(true)
        });
    }
}

impl Default for Timer<ParkThread, SystemNow> {
    fn default() -> Self {
        Timer::new(ParkThread::new())
    }
}

impl<T, N> Park for Timer<T, N>
where T: Park,
      N: Now,
{
    type Unpark = T::Unpark;
    type Error = T::Error;

    fn unpark(&self) -> Self::Unpark {
        self.park.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.process_queue();

        match self.next_expiration() {
            Some(expiration) => {
                let now = self.now.now();
                let deadline = self.expiration_instant(&expiration);

                if deadline > now {
                    self.park.park_timeout(deadline - now)?;
                }
            }
            None => {
                self.park.park()?;
            }
        }

        self.process();

        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.process_queue();

        match self.next_expiration() {
            Some(expiration) => {
                let now = self.now.now();
                let deadline = self.expiration_instant(&expiration);

                if deadline > now {
                    self.park.park_timeout(cmp::min(deadline - now, duration))?;
                }
            }
            None => {
                self.park.park_timeout(duration)?;
            }
        }

        self.process();

        Ok(())
    }
}

impl<T, N> Drop for Timer<T, N> {
    fn drop(&mut self) {
        // Shutdown the stack of entries to process, preventing any new entries
        // from being pushed.
        self.inner.process.shutdown();
    }
}

/// Convert a duration (milliseconds) to a level
fn level_for(duration: u64) -> usize {
    debug_assert!(duration > 0);

    // 63 is picked to offset this by 1. A duration of 0 is prohibited, so this
    // cannot underflow.
    let significant = 63 - duration.leading_zeros() as usize;
    significant / 6
}

// ===== impl Inner =====

impl Inner {
    fn new(start: Instant, unpark: Box<Unpark>) -> Inner {
        Inner {
            num: AtomicUsize::new(0),
            elapsed: AtomicU64::new(0),
            process: entry::AtomicStack::new(),
            start,
            unpark,
        }
    }

    fn now(&self) -> Instant {
        self.start + Duration::from_millis(self.elapsed.load(Relaxed))
    }

    /// Increment the number of active timeouts
    fn increment(&self) -> Result<(), Error> {
        let mut curr = self.num.load(SeqCst);

        loop {
            if curr == MAX_TIMEOUTS {
                return Err(Error::at_capacity());
            }

            let actual = self.num.compare_and_swap(curr, curr + 1, SeqCst);

            if curr == actual {
                return Ok(());
            }

            curr = actual;
        }
    }

    /// Decrement the number of active timeouts
    fn decrement(&self) {
        self.num.fetch_sub(1, SeqCst);
    }

    fn queue(&self, entry: &Arc<Entry>) -> Result<(), Error> {
        if self.process.push(entry)? {
            // The timer is notified so that it can process the timeout
            self.unpark.unpark();
        }

        Ok(())
    }
}

impl fmt::Debug for Inner {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Inner")
            .finish()
    }
}

enum Round {
    Up,
    Down,
}

/// Convert a `Duration` to milliseconds, rounding up and saturating at
/// `u64::MAX`.
///
/// The saturating is fine because `u64::MAX` milliseconds are still many
/// million years.
#[inline]
fn ms(duration: Duration, round: Round) -> u64 {
    const NANOS_PER_MILLI: u32 = 1_000_000;
    const MILLIS_PER_SEC: u64 = 1_000;

    // Round up.
    let millis = match round {
        Round::Up => (duration.subsec_nanos() + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI,
        Round::Down => duration.subsec_nanos() / NANOS_PER_MILLI,
    };

    duration.as_secs().saturating_mul(MILLIS_PER_SEC).saturating_add(millis as u64)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_level_for() {
        for pos in 1..64 {
            assert_eq!(0, level_for(pos), "level_for({}) -- binary = {:b}", pos, pos);
        }

        for level in 1..5 {
            for pos in level..64 {
                let a = pos * 64_usize.pow(level as u32);
                assert_eq!(level, level_for(a as u64),
                           "level_for({}) -- binary = {:b}", a, a);

                if pos > level {
                    let a = a - 1;
                    assert_eq!(level, level_for(a as u64),
                               "level_for({}) -- binary = {:b}", a, a);
                }

                if pos < 64 {
                    let a = a + 1;
                    assert_eq!(level, level_for(a as u64),
                               "level_for({}) -- binary = {:b}", a, a);
                }
            }
        }
    }
}
