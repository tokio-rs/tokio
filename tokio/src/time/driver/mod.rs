//! Time driver

mod atomic_stack;
use self::atomic_stack::AtomicStack;

mod entry;
pub(super) use self::entry::Entry;

mod handle;
pub(crate) use self::handle::Handle;

mod registration;
pub(crate) use self::registration::Registration;

mod stack;
use self::stack::Stack;

use crate::loom::sync::atomic::{AtomicU64, AtomicUsize};
use crate::park::{Park, Unpark};
use crate::time::{wheel, Error};
use crate::time::{Clock, Duration, Instant};

use std::sync::atomic::Ordering::{Acquire, Relaxed, Release, SeqCst};

use std::sync::Arc;
use std::usize;
use std::{cmp, fmt};

/// Time implementation that drives [`Delay`], [`Interval`], and [`Timeout`].
///
/// A `Driver` instance tracks the state necessary for managing time and
/// notifying the [`Delay`] instances once their deadlines are reached.
///
/// It is expected that a single instance manages many individual [`Delay`]
/// instances. The `Driver` implementation is thread-safe and, as such, is able
/// to handle callers from across threads.
///
/// After creating the `Driver` instance, the caller must repeatedly call
/// [`turn`]. The time driver will perform no work unless [`turn`] is called
/// repeatedly.
///
/// The driver has a resolution of one millisecond. Any unit of time that falls
/// between milliseconds are rounded up to the next millisecond.
///
/// When an instance is dropped, any outstanding [`Delay`] instance that has not
/// elapsed will be notified with an error. At this point, calling `poll` on the
/// [`Delay`] instance will result in `Err` being returned.
///
/// # Implementation
///
/// THe time driver is based on the [paper by Varghese and Lauck][paper].
///
/// A hashed timing wheel is a vector of slots, where each slot handles a time
/// slice. As time progresses, the timer walks over the slot for the current
/// instant, and processes each entry for that slot. When the timer reaches the
/// end of the wheel, it starts again at the beginning.
///
/// The implementation maintains six wheels arranged in a set of levels. As the
/// levels go up, the slots of the associated wheel represent larger intervals
/// of time. At each level, the wheel has 64 slots. Each slot covers a range of
/// time equal to the wheel at the lower level. At level zero, each slot
/// represents one millisecond of time.
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
/// `Delay` instances as their deadlines have been reached. For all higher
/// levels, all entries will be redistributed across the wheel at the next level
/// down. Eventually, as time progresses, entries will [`Delay`] instances will
/// either be canceled (dropped) or their associated entries will reach level
/// zero and be notified.
#[derive(Debug)]
pub(crate) struct Driver<T> {
    /// Shared state
    inner: Arc<Inner>,

    /// Timer wheel
    wheel: wheel::Wheel<Stack>,

    /// Thread parker. The `Driver` park implementation delegates to this.
    park: T,

    /// Source of "now" instances
    clock: Clock,
}

/// Timer state shared between `Driver`, `Handle`, and `Registration`.
pub(crate) struct Inner {
    /// The instant at which the timer started running.
    start: Instant,

    /// The last published timer `elapsed` value.
    elapsed: AtomicU64,

    /// Number of active timeouts
    num: AtomicUsize,

    /// Head of the "process" linked list.
    process: AtomicStack,

    /// Unparks the timer thread.
    unpark: Box<dyn Unpark>,
}

/// Maximum number of timeouts the system can handle concurrently.
const MAX_TIMEOUTS: usize = usize::MAX >> 1;

// ===== impl Driver =====

impl<T> Driver<T>
where
    T: Park,
{
    /// Creates a new `Driver` instance that uses `park` to block the current
    /// thread and `now` to get the current `Instant`.
    ///
    /// Specifying the source of time is useful when testing.
    pub(crate) fn new(park: T, clock: Clock) -> Driver<T> {
        let unpark = Box::new(park.unpark());

        Driver {
            inner: Arc::new(Inner::new(clock.now(), unpark)),
            wheel: wheel::Wheel::new(),
            park,
            clock,
        }
    }

    /// Returns a handle to the timer.
    ///
    /// The `Handle` is how `Delay` instances are created. The `Delay` instances
    /// can either be created directly or the `Handle` instance can be passed to
    /// `with_default`, setting the timer as the default timer for the execution
    /// context.
    pub(crate) fn handle(&self) -> Handle {
        Handle::new(Arc::downgrade(&self.inner))
    }

    /// Converts an `Expiration` to an `Instant`.
    fn expiration_instant(&self, when: u64) -> Instant {
        self.inner.start + Duration::from_millis(when)
    }

    /// Runs timer related logic
    fn process(&mut self) {
        let now = crate::time::ms(
            self.clock.now() - self.inner.start,
            crate::time::Round::Down,
        );
        let mut poll = wheel::Poll::new(now);

        while let Some(entry) = self.wheel.poll(&mut poll, &mut ()) {
            let when = entry.when_internal().expect("invalid internal entry state");

            // Fire the entry
            entry.fire(when);

            // Track that the entry has been fired
            entry.set_when_internal(None);
        }

        // Update the elapsed cache
        self.inner.elapsed.store(self.wheel.elapsed(), SeqCst);
    }

    /// Processes the entry queue
    ///
    /// This handles adding and canceling timeouts.
    fn process_queue(&mut self) {
        for entry in self.inner.process.take() {
            match (entry.when_internal(), entry.load_state()) {
                (None, None) => {
                    // Nothing to do
                }
                (Some(_), None) => {
                    // Remove the entry
                    self.clear_entry(&entry);
                }
                (None, Some(when)) => {
                    // Queue the entry
                    self.add_entry(entry, when);
                }
                (Some(_), Some(next)) => {
                    self.clear_entry(&entry);
                    self.add_entry(entry, next);
                }
            }
        }
    }

    fn clear_entry(&mut self, entry: &Arc<Entry>) {
        self.wheel.remove(entry, &mut ());
        entry.set_when_internal(None);
    }

    /// Fires the entry if it needs to, otherwise queue it to be processed later.
    ///
    /// Returns `None` if the entry was fired.
    fn add_entry(&mut self, entry: Arc<Entry>, when: u64) {
        use crate::time::wheel::InsertError;

        entry.set_when_internal(Some(when));

        match self.wheel.insert(when, entry, &mut ()) {
            Ok(_) => {}
            Err((entry, InsertError::Elapsed)) => {
                // The entry's deadline has elapsed, so fire it and update the
                // internal state accordingly.
                entry.set_when_internal(None);
                entry.fire(when);
            }
            Err((entry, InsertError::Invalid)) => {
                // The entry's deadline is invalid, so error it and update the
                // internal state accordingly.
                entry.set_when_internal(None);
                entry.error();
            }
        }
    }
}

impl<T> Park for Driver<T>
where
    T: Park,
{
    type Unpark = T::Unpark;
    type Error = T::Error;

    fn unpark(&self) -> Self::Unpark {
        self.park.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.process_queue();

        match self.wheel.poll_at() {
            Some(when) => {
                let now = self.clock.now();
                let deadline = self.expiration_instant(when);

                if deadline > now {
                    let dur = deadline - now;

                    if self.clock.is_paused() {
                        self.park.park_timeout(Duration::from_secs(0))?;
                        self.clock.advance(dur);
                    } else {
                        self.park.park_timeout(dur)?;
                    }
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

        match self.wheel.poll_at() {
            Some(when) => {
                let now = self.clock.now();
                let deadline = self.expiration_instant(when);

                if deadline > now {
                    let duration = cmp::min(deadline - now, duration);

                    if self.clock.is_paused() {
                        self.park.park_timeout(Duration::from_secs(0))?;
                        self.clock.advance(duration);
                    } else {
                        self.park.park_timeout(duration)?;
                    }
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

impl<T> Drop for Driver<T> {
    fn drop(&mut self) {
        use std::u64;

        // Shutdown the stack of entries to process, preventing any new entries
        // from being pushed.
        self.inner.process.shutdown();

        // Clear the wheel, using u64::MAX allows us to drain everything
        let mut poll = wheel::Poll::new(u64::MAX);

        while let Some(entry) = self.wheel.poll(&mut poll, &mut ()) {
            entry.error();
        }
    }
}

// ===== impl Inner =====

impl Inner {
    fn new(start: Instant, unpark: Box<dyn Unpark>) -> Inner {
        Inner {
            num: AtomicUsize::new(0),
            elapsed: AtomicU64::new(0),
            process: AtomicStack::new(),
            start,
            unpark,
        }
    }

    fn elapsed(&self) -> u64 {
        self.elapsed.load(SeqCst)
    }

    #[cfg(all(test, loom))]
    fn num(&self, ordering: std::sync::atomic::Ordering) -> usize {
        self.num.load(ordering)
    }

    /// Increments the number of active timeouts
    fn increment(&self) -> Result<(), Error> {
        let mut curr = self.num.load(Relaxed);
        loop {
            if curr == MAX_TIMEOUTS {
                return Err(Error::at_capacity());
            }

            match self
                .num
                .compare_exchange_weak(curr, curr + 1, Release, Relaxed)
            {
                Ok(_) => return Ok(()),
                Err(next) => curr = next,
            }
        }
    }

    /// Decrements the number of active timeouts
    fn decrement(&self) {
        let prev = self.num.fetch_sub(1, Acquire);
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

        crate::time::ms(deadline - self.start, crate::time::Round::Up)
    }
}

impl fmt::Debug for Inner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Inner").finish()
    }
}

#[cfg(all(test, loom))]
mod tests;
