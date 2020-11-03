// Currently, rust warns when an unsafe fn contains an unsafe {} block. However,
// in the future, this will change to the reverse. For now, suppress this
// warning and generally stick with being explicit about unsafety.
#![allow(unused_unsafe)]
#![cfg_attr(not(feature = "rt"), allow(dead_code))]

//! Time driver

mod entry;
pub(self) use self::entry::TimerEntry;
pub(self) use self::entry::{EntryList, EntryState, TimerHandle, TimerShared};

mod handle;
pub(crate) use self::handle::Handle;
use self::handle::InternalHandle;

mod wheel;

pub(super) mod sleep;

use crate::loom::sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex, MutexGuard};
use crate::park::{Park, Unpark};
use crate::time::error::Error;
use crate::time::{Clock, Duration, Instant};

use std::convert::TryInto;
use std::fmt;
use std::{num::NonZeroU64, ptr::NonNull, task::Waker};

/// Time implementation that drives [`Sleep`][sleep], [`Interval`][interval], and [`Timeout`][timeout].
///
/// A `Driver` instance tracks the state necessary for managing time and
/// notifying the [`Sleep`][sleep] instances once their deadlines are reached.
///
/// It is expected that a single instance manages many individual [`Sleep`][sleep]
/// instances. The `Driver` implementation is thread-safe and, as such, is able
/// to handle callers from across threads.
///
/// After creating the `Driver` instance, the caller must repeatedly call `park`
/// or `park_timeout`. The time driver will perform no work unless `park` or
/// `park_timeout` is called repeatedly.
///
/// The driver has a resolution of one millisecond. Any unit of time that falls
/// between milliseconds are rounded up to the next millisecond.
///
/// When an instance is dropped, any outstanding [`Sleep`][sleep] instance that has not
/// elapsed will be notified with an error. At this point, calling `poll` on the
/// [`Sleep`][sleep] instance will result in panic.
///
/// # Implementation
///
/// The time driver is based on the [paper by Varghese and Lauck][paper].
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
/// `Sleep` instances as their deadlines have been reached. For all higher
/// levels, all entries will be redistributed across the wheel at the next level
/// down. Eventually, as time progresses, entries with [`Sleep`][sleep] instances will
/// either be canceled (dropped) or their associated entries will reach level
/// zero and be notified.
///
/// [paper]: http://www.cs.columbia.edu/~nahum/w6998/papers/ton97-timing-wheels.pdf
/// [sleep]: crate::time::Sleep
/// [timeout]: crate::time::Timeout
/// [interval]: crate::time::Interval
#[derive(Debug)]
pub(crate) struct Driver<P: Park + 'static> {
    /// Timing backend in use
    time_source: ClockTime,

    /// Shared state
    inner: InternalHandle<ClockTime>,

    /// Parker to delegate to
    park: P,

    /// Unparker for this time driver
    unpark: TimeUnpark<P::Unpark>,
}

/// A time source trait used for mocks in tests.
///
/// This is distinct from Clock as it 1) handles the conversion to u64 ticks, 2)
/// avoids any global state, and 3) is agnostic to any particular strategy for
/// updating it. In particular, we'll be using this in loom-based testing for
/// the concurrency aspects of this driver.
///
/// The Driver frontend uses Clock to back the TimeSource, but some tests will
/// define their own lightweight mocks.
pub(super) trait TimeSource: Clone {
    fn deadline_to_tick(&self, t: Instant) -> u64;
    fn instant_to_tick(&self, t: Instant) -> u64;
    fn tick_to_instant(&self, t: u64) -> Instant;
    fn tick_to_duration(&self, t: u64) -> Duration {
        Duration::from_millis(t)
    }
    fn now(&self) -> u64;
}

#[derive(Debug, Clone)]
pub(super) struct ClockTime {
    clock: super::clock::Clock,
    start_time: Instant,
}

impl ClockTime {
    fn new(clock: Clock) -> Self {
        Self {
            clock,
            start_time: super::clock::now(),
        }
    }
}

impl TimeSource for ClockTime {
    fn deadline_to_tick(&self, t: Instant) -> u64 {
        // Round up to the end of a ms
        self.instant_to_tick(t + Duration::from_nanos(999_999))
    }

    fn instant_to_tick(&self, t: Instant) -> u64 {
        // round up
        let dur: Duration = t
            .checked_duration_since(self.start_time)
            .unwrap_or_else(|| Duration::from_secs(0));
        let ms = dur.as_millis();

        ms.try_into().expect("Duration too far into the future")
    }

    fn tick_to_instant(&self, t: u64) -> Instant {
        self.start_time + Duration::from_millis(t)
    }

    fn now(&self) -> u64 {
        self.instant_to_tick(self.clock.now())
    }
}

/// Timer state shared between `Driver`, `Handle`, and `Registration`.
pub(self) struct Inner<TS: TimeSource> {
    /// Timing backend in use
    time_source: TS,

    /// The last published timer `elapsed` value.
    elapsed: u64,

    /// The earliest time at which we promise to wake up without unparking
    next_wake: Option<NonZeroU64>,

    /// Timer wheel
    wheel: wheel::Wheel,

    /// True if the driver is being shutdown
    is_shutdown: bool,

    /// Access to the TimeUnparker
    unpark: TimeUnpark<dyn Unpark>,
}

// ===== impl Driver =====

impl<P> Driver<P>
where
    P: Park + 'static,
{
    /// Creates a new `Driver` instance that uses `park` to block the current
    /// thread and `time_source` to get the current time and convert to ticks.
    ///
    /// Specifying the source of time is useful when testing.
    pub(crate) fn new(park: P, clock: Clock) -> Driver<P> {
        let time_source = ClockTime::new(clock);
        let unpark = TimeUnpark::new(park.unpark());

        let inner = Inner::new(time_source.clone(), unpark.clone().coerce_unsized());

        Driver {
            time_source,
            inner: InternalHandle::new(Arc::new(Mutex::new(inner))),
            park,
            unpark,
        }
    }

    /// Returns a handle to the timer.
    ///
    /// The `Handle` is how `Sleep` instances are created. The `Sleep` instances
    /// can either be created directly or the `Handle` instance can be passed to
    /// `with_default`, setting the timer as the default timer for the execution
    /// context.
    pub(crate) fn handle(&self) -> Handle {
        self.inner.clone().into()
    }

    fn park_internal(&mut self, limit: Option<Duration>) -> Result<(), P::Error> {
        let clock = &self.time_source.clock;

        match self.inner.process() {
            Some(when) => {
                let when = when.get();

                let now = self.time_source.now();
                // Note that we effectively round up to 1ms here - this avoids
                // very short-duration microsecond-resolution sleeps that the OS
                // might treat as zero-length.
                let mut duration = self.time_source.tick_to_duration(when.saturating_sub(now));

                if duration > Duration::from_millis(0) {
                    if let Some(limit) = limit {
                        duration = std::cmp::min(limit, duration);
                    }

                    if clock.is_paused() {
                        self.park.park_timeout(Duration::from_secs(0))?;

                        // If we were unparked recently, we need to return to
                        // the scheduler to deal with that before we advance
                        // time.
                        if !self.unpark.get_and_clear() {
                            clock.advance(duration);
                        }
                    } else {
                        self.park.park_timeout(duration)?;
                    }
                } else {
                    self.park.park_timeout(Duration::from_secs(0))?;
                }
            }
            None => {
                if let Some(duration) = limit {
                    if clock.is_paused() {
                        self.park.park_timeout(Duration::from_secs(0))?;
                        if !self.unpark.get_and_clear() {
                            clock.advance(duration);
                        }
                    } else {
                        self.park.park_timeout(duration)?;
                    }
                } else {
                    self.park.park()?;
                }
            }
        }

        // Process pending timers after waking up
        self.inner.process();

        Ok(())
    }
}

impl<TS: TimeSource> InternalHandle<TS> {
    /// Runs timer related logic, and returns the next wakeup time
    pub(self) fn process(&self) -> Option<NonZeroU64> {
        let now = self.time_source().now();

        self.process_at_time(now)
    }

    pub(self) fn process_at_time(&self, now: u64) -> Option<NonZeroU64> {
        let mut any_awoken = false;

        let mut waker_list: [Option<Waker>; 32] = Default::default();
        let mut waker_idx = 0;

        let mut lock = self.lock();

        assert!(now >= lock.elapsed);

        while let Some(entry) = lock.wheel.poll(now) {
            // Fire the entry
            let waker = unsafe { entry.fire(EntryState::Fired) };

            if waker.is_some() {
                any_awoken = true;

                waker_list[waker_idx] = waker;

                waker_idx += 1;

                if waker_idx == waker_list.len() {
                    // Wake a batch of wakers. To avoid deadlock, we must do this with the lock temporarily dropped.
                    std::mem::drop(lock);

                    for waker in waker_list.iter_mut() {
                        waker.take().unwrap().wake();
                    }

                    waker_idx = 0;

                    lock = self.lock();
                }
            }
        }

        // Update the elapsed cache
        lock.elapsed = lock.wheel.elapsed();
        lock.next_wake = lock
            .wheel
            .poll_at()
            .map(|t| NonZeroU64::new(t).unwrap_or_else(|| NonZeroU64::new(1).unwrap()));

        let mut next_wake = lock.next_wake;

        if any_awoken {
            // Wakers don't call unpark when invoked from the thread they need
            // to awaken, so yield back to the runtime so it can check for
            // pending tasks.
            next_wake =
                Some(NonZeroU64::new(lock.elapsed).unwrap_or_else(|| NonZeroU64::new(1).unwrap()));
        }

        std::mem::drop(lock);

        for waker in waker_list[0..waker_idx].iter_mut() {
            waker.take().unwrap().wake();
        }

        next_wake
    }

    /// Removes a registered timer from the driver.
    ///
    /// The timer will be moved to the cancelled state. Wakers will _not_ be invoked.
    /// If the timer is already completed, this function is a no-op.
    ///
    /// SAFETY: The timer must not be registered with some other driver, and
    /// `add_entry` must not be called concurrently.
    pub(self) unsafe fn clear_entry(&self, entry: NonNull<TimerShared>) {
        unsafe {
            let mut lock = self.lock();

            if unsafe { entry.as_ref().is_registered() } {
                lock.wheel.remove(entry);
            }

            entry.as_ref().handle().fire(EntryState::Cancelled);
        }
    }

    /// Forces a timer in the FiringInProgress state into its final state.
    /// Timers in any other state will take the driver lock, but otherwise take
    /// no action.
    #[cold]
    pub(self) unsafe fn sync_timer(&self, entry: NonNull<TimerShared>) {
        unsafe {
            let mut lock = self.lock();

            if unsafe { entry.as_ref().is_pending() } {
                lock.wheel.remove(entry);
                entry.as_ref().handle().fire(EntryState::Fired);
            }
        }
    }

    /// Removes and re-adds an entry to the driver.
    ///
    /// SAFETY: The timer must be either unregistered, or registered with this
    /// driver. No other threads are allowed to concurrently manipulate the
    /// timer at all (the current thread should hold an exclusive reference to
    /// the `TimerEntry`)
    pub(self) unsafe fn reregister(&self, entry: NonNull<TimerShared>) {
        unsafe {
            let mut lock = self.lock();

            // We may have raced with a firing/deregistration, so check before
            // deregistering.
            if unsafe { entry.as_ref().is_registered() } {
                lock.wheel.remove(entry);
            }

            // Always attempt to reregister. We'll fire it again if it's already
            // expired/shutdown/etc.
            Self::add_entry0(lock, entry.as_ref().handle())
        }
    }

    /// Adds a new timer to the driver.
    ///
    /// If the timer is already expired, or if an error occurs, the timer will
    /// be automatically transitioned to a completed state. The waker will _not_
    /// be fired in this case, so the caller must check the timer state after
    /// calling add_entry.
    ///
    /// SAFETY: The corresponding entry must remain pinned until timer
    /// completion or deregistration, and must not yet be registered.
    pub(self) unsafe fn add_entry(&self, entry: TimerHandle) {
        debug_assert!(unsafe { entry.is_pre_registration() });

        Self::add_entry0(self.lock(), entry);
    }

    unsafe fn add_entry0(mut lock: MutexGuard<'_, Inner<TS>>, entry: TimerHandle) {
        if lock.is_shutdown {
            unsafe {
                entry.fire(EntryState::Error(crate::time::error::Kind::Shutdown));
                return;
            }
        }

        if !entry.set_registered() {
            return;
        }

        match unsafe { lock.wheel.insert(entry) } {
            Ok(when) => {
                if lock
                    .next_wake
                    .map(|next_wake| when < next_wake.get())
                    .unwrap_or(true)
                {
                    lock.unpark.unpark();
                }

                None
            }
            Err((entry, super::error::InsertError::Elapsed)) => unsafe {
                entry.fire(EntryState::Fired)
            },
            Err((entry, super::error::InsertError::Invalid)) => unsafe {
                entry.fire(EntryState::Error(crate::time::error::Kind::Invalid))
            },
        };
    }
}

pub(crate) struct TimeUnpark<U: Unpark + ?Sized> {
    // loom's Arc does not support trait objects
    unpark: std::sync::Arc<U>,
    unparked: Arc<AtomicBool>,
}

impl<U: Unpark + ?Sized> Clone for TimeUnpark<U> {
    fn clone(&self) -> Self {
        Self {
            unpark: self.unpark.clone(),
            unparked: self.unparked.clone(),
        }
    }
}

impl TimeUnpark<dyn Unpark> {
    #[cfg(test)]
    pub(self) fn mock() -> Self {
        struct MockUnpark;

        impl Unpark for MockUnpark {
            fn unpark(&self) {}
        }

        Self {
            unpark: std::sync::Arc::new(MockUnpark),
            unparked: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl<U: Unpark + ?Sized> TimeUnpark<U> {
    fn new(unpark: U) -> Self
    where
        U: Sized,
    {
        Self {
            unpark: std::sync::Arc::new(unpark),
            unparked: Arc::new(AtomicBool::new(false)),
        }
    }

    fn coerce_unsized(self) -> TimeUnpark<dyn Unpark>
    where
        U: Sized,
    {
        TimeUnpark {
            unpark: self.unpark,
            unparked: self.unparked,
        }
    }

    fn get_and_clear(&self) -> bool {
        self.unparked.swap(false, Ordering::AcqRel)
    }
}

impl<U: Unpark + ?Sized> std::fmt::Debug for TimeUnpark<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimeUnpark")
            .field("unparked", &self.unparked.load(Ordering::SeqCst))
            .finish()
    }
}

impl<U: Unpark + ?Sized> Unpark for TimeUnpark<U> {
    fn unpark(&self) {
        self.unparked.store(true, Ordering::Release);
        self.unpark.unpark();
    }
}

impl<P> Park for Driver<P>
where
    P: Park + 'static,
{
    type Unpark = TimeUnpark<P::Unpark>;
    type Error = P::Error;

    fn unpark(&self) -> Self::Unpark {
        self.unpark.clone()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.park_internal(None)
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.park_internal(Some(duration))
    }

    fn shutdown(&mut self) {
        let mut lock = self.inner.lock();

        if lock.is_shutdown {
            return;
        }

        lock.is_shutdown = true;

        std::mem::drop(lock);

        // Advance time forward to the end of time.

        self.inner.process_at_time(u64::MAX);

        self.park.shutdown();
    }
}

impl<P> Drop for Driver<P>
where
    P: Park + 'static,
{
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ===== impl Inner =====

impl<TS> Inner<TS>
where
    TS: TimeSource,
{
    pub(self) fn new(time_source: TS, unpark: TimeUnpark<dyn Unpark>) -> Self {
        Inner {
            time_source,
            elapsed: 0,
            next_wake: None,
            unpark,
            wheel: wheel::Wheel::new(),
            is_shutdown: false,
        }
    }
}

impl<TS: TimeSource> fmt::Debug for Inner<TS> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Inner").finish()
    }
}

//#[cfg(all(test, loom))]
//mod tests;

#[cfg(test)]
mod tests;
