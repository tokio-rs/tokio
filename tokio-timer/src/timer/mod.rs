//! Timer implementation.
//!
//! This module contains the types needed to run a timer.
//!
//! The [`Timer`] type runs the timer logic. It holds all the necessary state
//! to track all associated [`Delay`] instances and delivering notifications
//! once the deadlines are reached.
//!
//! The [`Handle`] type is a reference to a [`Timer`] instance. This type is
//! `Clone`, `Send`, and `Sync`. This type is used to create instances of
//! [`Delay`].
//!
//! The [`Now`] trait describes how to get an `Instance` representing the
//! current moment in time. [`SystemNow`] is the default implementation, where
//! [`Now::now`] is implemented by calling `Instant::now`.
//!
//! [`Timer`] is generic over [`Now`]. This allows the source of time to be
//! customized. This ability is especially useful in tests and any environment
//! where determinism is necessary.
//!
//! Note, when using the Tokio runtime, the `Timer` does not need to be manually
//! setup as the runtime comes pre-configured with a `Timer` instance.
//!
//! [`Timer`]: struct.Timer.html
//! [`Handle`]: struct.Handle.html
//! [`Delay`]: ../struct.Delay.html
//! [`Now`]: trait.Now.html
//! [`Now::now`]: trait.Now.html#method.now
//! [`SystemNow`]: struct.SystemNow.html

// This allows the usage of the old `Now` trait.
#![allow(deprecated)]

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
use std::sync::atomic::Ordering::SeqCst;
use std::usize;

/// Timer implementation that drives [`Delay`], [`Interval`], and [`Deadline`].
///
/// A `Timer` instance tracks the state necessary for managing time and
/// notifying the [`Delay`] instances once their deadlines are reached.
///
/// It is expected that a single `Timer` instance manages many individual
/// `Delay` instances. The `Timer` implementation is thread-safe and, as such,
/// is able to handle callers from across threads.
///
/// Callers do not use `Timer` directly to create `Delay` instances.  Instead,
/// [`Handle`] is used. A handle for the timer instance is obtained by calling
/// [`handle`]. [`Handle`] is the type that implements `Clone` and is `Send +
/// Sync`.
///
/// After creating the `Timer` instance, the caller must repeatedly call
/// [`turn`]. The timer will perform no work unless [`turn`] is called
/// repeatedly.
///
/// The `Timer` has a resolution of one millisecond. Any unit of time that falls
/// between milliseconds are rounded up to the next millisecond.
///
/// When the `Timer` instance is dropped, any outstanding `Delay` instance that
/// has not elapsed will be notified with an error. At this point, calling
/// `poll` on the `Delay` instance will result in `Err` being returned.
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
/// [`Delay`] instances as their deadlines have been reached. For all higher
/// levels, all entries will be redistributed across the wheel at the next level
/// down. Eventually, as time progresses, entries will `Delay` instances will
/// either be canceled (dropped) or their associated entries will reach level
/// zero and be notified.
///
/// [`Delay`]: ../struct.Delay.html
/// [`Interval`]: ../struct.Interval.html
/// [`Deadline`]: ../struct.Deadline.html
/// [paper]: http://www.cs.columbia.edu/~nahum/w6998/papers/ton97-timing-wheels.pdf
/// [`handle`]: #method.handle
/// [`turn`]: #method.turn
/// [`Handle`]: struct.Handle.html
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

/// Timer state shared between `Timer`, `Handle`, and `Registration`.
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

/// Number of levels. Each level has 64 slots. By using 6 levels with 64 slots
/// each, the timer is able to track time up to 2 years into the future with a
/// precision of 1 millisecond.
const NUM_LEVELS: usize = 6;

/// The maximum duration of a delay
const MAX_DURATION: u64 = 1 << (6 * NUM_LEVELS);

/// Maximum number of timeouts the system can handle concurrently.
const MAX_TIMEOUTS: usize = usize::MAX >> 1;

// ===== impl Timer =====

impl<T> Timer<T>
where T: Park
{
    /// Create a new `Timer` instance that uses `park` to block the current
    /// thread.
    ///
    /// Once the timer has been created, a handle can be obtained using
    /// [`handle`]. The handle is used to create `Delay` instances.
    ///
    /// Use `default` when constructing a `Timer` using the default `park`
    /// instance.
    ///
    /// [`handle`]: #method.handle
    pub fn new(park: T) -> Self {
        Timer::new_with_now(park, SystemNow::new())
    }
}

impl<T, N> Timer<T, N> {
    /// Returns a reference to the underlying `Park` instance.
    pub fn get_park(&self) -> &T {
        &self.park
    }

    /// Returns a mutable reference to the underlying `Park` instance.
    pub fn get_park_mut(&mut self) -> &mut T {
        &mut self.park
    }
}

impl<T, N> Timer<T, N>
where T: Park,
      N: Now,
{
    /// Create a new `Timer` instance that uses `park` to block the current
    /// thread and `now` to get the current `Instant`.
    ///
    /// Specifying the source of time is useful when testing.
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

    /// Returns a handle to the timer.
    ///
    /// The `Handle` is how `Delay` instances are created. The `Delay` instances
    /// can either be created directly or the `Handle` instance can be passed to
    /// `with_default`, setting the timer as the default timer for the execution
    /// context.
    pub fn handle(&self) -> Handle {
        Handle::new(Arc::downgrade(&self.inner))
    }

    /// Performs one iteration of the timer loop.
    ///
    /// This function must be called repeatedly in order for the `Timer`
    /// instance to make progress. This is where the work happens.
    ///
    /// The `Timer` will use the `Park` instance that was specified in [`new`]
    /// to block the current thread until the next `Delay` instance elapses. One
    /// call to `turn` results in at most one call to `park.park()`.
    ///
    /// # Return
    ///
    /// On success, `Ok(Turn)` is returned, where `Turn` is a placeholder type
    /// that currently does nothing but may, in the future, have functions add
    /// to provide information about the call to `turn`.
    ///
    /// If the call to `park.park()` fails, then `Err` is returned with the
    /// error.
    ///
    /// [`new`]: #method.new
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

            // Process the slot, either moving it down a level or firing the
            // timeout if currently at the final (boss) level.
            self.process_expiration(&expiration);

            self.set_elapsed(expiration.deadline);
        }

        self.set_elapsed(now);
    }

    fn set_elapsed(&mut self, when: u64) {
        if when > self.elapsed {
            self.elapsed = when;
            self.inner.elapsed.store(when, SeqCst);
        }
    }

    fn process_expiration(&mut self, expiration: &Expiration) {
        while let Some(entry) = self.pop_entry(expiration) {
            if expiration.level == 0 {
                let when = entry.when_internal()
                    .expect("invalid internal entry state");

                debug_assert_eq!(when, expiration.deadline);

                // Fire the entry
                entry.fire(when);

                // Track that the entry has been fired
                entry.set_when_internal(None);
            } else {
                let when = entry.when_internal()
                    .expect("entry not tracked");

                let next_level = expiration.level - 1;

                self.levels[next_level]
                    .add_entry(entry, when);
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
            match (entry.when_internal(), entry.load_state()) {
                (None, None) => {
                    // Nothing to do
                }
                (Some(when), None) => {
                    // Remove the entry
                    self.clear_entry(&entry, when);
                }
                (None, Some(when)) => {
                    // Queue the entry
                    self.add_entry(entry, when);
                }
                (Some(curr), Some(next)) => {
                    self.clear_entry(&entry, curr);
                    self.add_entry(entry, next);
                }
            }
        }
    }

    fn clear_entry(&mut self, entry: &Arc<Entry>, when: u64) {
        // Get the level at which the entry should be stored
        let level = self.level_for(when);
        self.levels[level].remove_entry(entry, when);

        entry.set_when_internal(None);
    }

    /// Fire the entry if it needs to, otherwise queue it to be processed later.
    ///
    /// Returns `None` if the entry was fired.
    fn add_entry(&mut self, entry: Arc<Entry>, when: u64) {
        if when <= self.elapsed {
            // The entry's deadline has elapsed, so fire it and update the
            // internal state accordingly.
            entry.set_when_internal(None);
            entry.fire(when);

            return;
        } else if when - self.elapsed > MAX_DURATION {
            // The entry's deadline is invalid, so error it and update the
            // internal state accordingly.
            entry.set_when_internal(None);
            entry.error();

            return;
        }

        // Get the level at which the entry should be stored
        let level = self.level_for(when);

        entry.set_when_internal(Some(when));
        self.levels[level].add_entry(entry, when);

        debug_assert!({
            self.levels[level].next_expiration(self.elapsed)
                .map(|e| e.deadline >= self.elapsed)
                .unwrap_or(true)
        });
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
                } else {
                    self.park.park_timeout(Duration::from_secs(0))?;
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
                } else {
                    self.park.park_timeout(Duration::from_secs(0))?;
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

    fn elapsed(&self) -> u64 {
        self.elapsed.load(SeqCst)
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
        let prev = self.num.fetch_sub(1, SeqCst);
        debug_assert!(prev <= MAX_TIMEOUTS);
    }

    fn queue(&self, entry: &Arc<Entry>) -> Result<(), Error> {
        if self.process.push(entry)? {
            // The timer is notified so that it can process the timeout
            self.unpark.unpark();
        }

        Ok(())
    }

    fn normalize_deadline(&self, deadline: Instant) -> u64 {
        if deadline < self.start {
            return 0;
        }

        ms(deadline - self.start, Round::Up)
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
            assert_eq!(0, level_for(0, pos), "level_for({}) -- binary = {:b}", pos, pos);
        }

        for level in 1..5 {
            for pos in level..64 {
                let a = pos * 64_usize.pow(level as u32);
                assert_eq!(level, level_for(0, a as u64),
                           "level_for({}) -- binary = {:b}", a, a);

                if pos > level {
                    let a = a - 1;
                    assert_eq!(level, level_for(0, a as u64),
                               "level_for({}) -- binary = {:b}", a, a);
                }

                if pos < 64 {
                    let a = a + 1;
                    assert_eq!(level, level_for(0, a as u64),
                               "level_for({}) -- binary = {:b}", a, a);
                }
            }
        }
    }
}
