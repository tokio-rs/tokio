// Currently, rust warns when an unsafe fn contains an unsafe {} block. However,
// in the future, this will change to the reverse. For now, suppress this
// warning and generally stick with being explicit about unsafety.
#![allow(unused_unsafe)]
#![cfg_attr(not(feature = "rt"), allow(dead_code))]

//! Time driver.

mod entry;
pub(crate) use entry::TimerEntry;
use entry::{EntryList, TimerHandle, TimerShared, MAX_SAFE_MILLIS_DURATION};

mod handle;
pub(crate) use self::handle::Handle;

mod source;
pub(crate) use source::TimeSource;

mod timer_buckets;
use timer_buckets::GlobalTimerBuckets;

mod wheel;

use crate::loom::sync::atomic::{AtomicBool, Ordering};
use crate::loom::sync::Mutex;
use crate::runtime::driver::{self, IoHandle, IoStack};
use crate::time::error::Error;
use crate::time::{Clock, Duration};
use crate::util::WakeList;

use std::fmt;
use std::{num::NonZeroU64, ptr::NonNull};

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
pub(crate) struct Driver {
    /// Parker to delegate to.
    park: IoStack,
}

/// Timer state shared between `Driver`, `Handle`, and `Registration`.
struct Inner {
    // The state is split like this so `Handle` can access `is_shutdown` without locking the mutex
    state: Mutex<InnerState>,

    /// Global timer buckets for fast-path timer registration (0-120 seconds).
    /// These have their own synchronization and don't need the driver lock.
    buckets: GlobalTimerBuckets,

    /// True if the driver is being shutdown.
    is_shutdown: AtomicBool,

    // When `true`, a call to `park_timeout` should immediately return and time
    // should not advance. One reason for this to be `true` is if the task
    // passed to `Runtime::block_on` called `task::yield_now()`.
    //
    // While it may look racy, it only has any effect when the clock is paused
    // and pausing the clock is restricted to a single-threaded runtime.
    #[cfg(feature = "test-util")]
    did_wake: AtomicBool,
}

/// Time state shared which must be protected by a `Mutex`
struct InnerState {
    /// The earliest time at which we promise to wake up without unparking.
    next_wake: Option<NonZeroU64>,

    /// Timer wheel (fallback for timers > 120 seconds).
    wheel: wheel::Wheel,
}

// ===== impl Driver =====

impl Driver {
    /// Creates a new `Driver` instance that uses `park` to block the current
    /// thread and `time_source` to get the current time and convert to ticks.
    ///
    /// Specifying the source of time is useful when testing.
    pub(crate) fn new(park: IoStack, clock: &Clock) -> (Driver, Handle) {
        let time_source = TimeSource::new(clock);
        let initial_tick = time_source.now(clock);

        let handle = Handle {
            time_source,
            inner: Inner {
                state: Mutex::new(InnerState {
                    next_wake: None,
                    wheel: wheel::Wheel::new(),
                }),
                buckets: GlobalTimerBuckets::new(initial_tick),
                is_shutdown: AtomicBool::new(false),

                #[cfg(feature = "test-util")]
                did_wake: AtomicBool::new(false),
            },
        };

        let driver = Driver { park };

        (driver, handle)
    }

    pub(crate) fn park(&mut self, handle: &driver::Handle) {
        self.park_internal(handle, None);
    }

    pub(crate) fn park_timeout(&mut self, handle: &driver::Handle, duration: Duration) {
        self.park_internal(handle, Some(duration));
    }

    pub(crate) fn shutdown(&mut self, rt_handle: &driver::Handle) {
        let handle = rt_handle.time();

        if handle.is_shutdown() {
            return;
        }

        handle.inner.is_shutdown.store(true, Ordering::SeqCst);

        // Advance time forward to the end of time.

        handle.process_at_time(u64::MAX);

        self.park.shutdown(rt_handle);
    }

    fn park_internal(&mut self, rt_handle: &driver::Handle, limit: Option<Duration>) {
        let handle = rt_handle.time();
        let mut lock = handle.inner.lock();

        assert!(!handle.is_shutdown());

        // Calculate next_wake from both bucket timers and wheel timers
        let maybe_bucket_next = handle.inner.buckets.next_expiration_time();
        let maybe_wheel_next = lock.wheel.next_expiration_time();

        // Take the minimum of bucket and wheel next expiration times
        let next_wake = match (maybe_bucket_next, maybe_wheel_next) {
            (Some(bucket_next), Some(wheel_next)) => Some(std::cmp::min(bucket_next, wheel_next)),
            (bucket_next, wheel_next) => bucket_next.or(wheel_next),
        };

        lock.next_wake =
            next_wake.map(|t| NonZeroU64::new(t).unwrap_or_else(|| NonZeroU64::new(1).unwrap()));

        drop(lock);

        match next_wake {
            Some(when) => {
                let now = handle.time_source.now(rt_handle.clock());
                // Note that we effectively round up to 1ms here - this avoids
                // very short-duration microsecond-resolution sleeps that the OS
                // might treat as zero-length.
                let mut duration = handle
                    .time_source
                    .tick_to_duration(when.saturating_sub(now));

                if duration > Duration::from_millis(0) {
                    if let Some(limit) = limit {
                        duration = std::cmp::min(limit, duration);
                    }

                    self.park_thread_timeout(rt_handle, duration);
                } else {
                    self.park.park_timeout(rt_handle, Duration::from_secs(0));
                }
            }
            None => {
                if let Some(duration) = limit {
                    self.park_thread_timeout(rt_handle, duration);
                } else {
                    self.park.park(rt_handle);
                }
            }
        }

        // Process pending timers after waking up
        handle.process(rt_handle.clock());
    }

    cfg_test_util! {
        fn park_thread_timeout(&mut self, rt_handle: &driver::Handle, duration: Duration) {
            let handle = rt_handle.time();
            let clock = rt_handle.clock();

            if clock.can_auto_advance() {
                self.park.park_timeout(rt_handle, Duration::from_secs(0));

                // If the time driver was woken, then the park completed
                // before the "duration" elapsed (usually caused by a
                // yield in `Runtime::block_on`). In this case, we don't
                // advance the clock.
                if !handle.did_wake() {
                    // Simulate advancing time
                    if let Err(msg) = clock.advance(duration) {
                        panic!("{}", msg);
                    }
                }
            } else {
                self.park.park_timeout(rt_handle, duration);
            }
        }
    }

    cfg_not_test_util! {
        fn park_thread_timeout(&mut self, rt_handle: &driver::Handle, duration: Duration) {
            self.park.park_timeout(rt_handle, duration);
        }
    }
}

impl Handle {
    pub(self) fn process(&self, clock: &Clock) {
        let now = self.time_source().now(clock);

        self.process_at_time(now);
    }

    pub(self) fn process_at_time(&self, mut now: u64) {
        let mut waker_list = WakeList::new();

        // First, advance the timer buckets and fire all expired timers
        // This doesn't require the driver lock - buckets have their own synchronization
        // SAFETY: The buckets manage their own thread safety via atomics and per-bucket locks
        let bucket_wakers = unsafe { self.inner.buckets.advance(now) };
        for waker in bucket_wakers {
            waker_list.push(waker);

            if !waker_list.can_push() {
                waker_list.wake_all();
            }
        }

        // Now process the timer wheel (for timers > 120s) - this DOES need the driver lock
        let mut lock = self.inner.lock();

        if now < lock.wheel.elapsed() {
            // Time went backwards! This normally shouldn't happen as the Rust language
            // guarantees that an Instant is monotonic, but can happen when running
            // Linux in a VM on a Windows host due to std incorrectly trusting the
            // hardware clock to be monotonic.
            //
            // See <https://github.com/tokio-rs/tokio/issues/3619> for more information.
            now = lock.wheel.elapsed();
        }

        while let Some(entry) = lock.wheel.poll(now) {
            debug_assert!(unsafe { entry.is_pending() });

            // SAFETY: We hold the driver lock, and just removed the entry from any linked lists.
            if let Some(waker) = unsafe { entry.fire(Ok(())) } {
                waker_list.push(waker);

                if !waker_list.can_push() {
                    // Wake a batch of wakers. To avoid deadlock, we must do this with the lock temporarily dropped.
                    drop(lock);

                    waker_list.wake_all();

                    lock = self.inner.lock();
                }
            }
        }

        // Calculate next_wake from both bucket timers and wheel timers
        let maybe_bucket_next = self.inner.buckets.next_expiration_time();
        let maybe_wheel_next = lock.wheel.poll_at();

        // Take the minimum of bucket and wheel next expiration times
        let next_wake = match (maybe_bucket_next, maybe_wheel_next) {
            (Some(b), Some(w)) => Some(std::cmp::min(b, w)),
            (bucket, wheel) => bucket.or(wheel),
        };

        lock.next_wake =
            next_wake.map(|t| NonZeroU64::new(t).unwrap_or_else(|| NonZeroU64::new(1).unwrap()));

        drop(lock);

        waker_list.wake_all();
    }

    /// Removes a registered timer from the driver.
    ///
    /// The timer will be moved to the cancelled state. Wakers will _not_ be
    /// invoked. If the timer is already completed, this function is a no-op.
    ///
    /// This function always acquires the driver lock, even if the entry does
    /// not appear to be registered.
    ///
    /// SAFETY: The timer must not be registered with some other driver, and
    /// `add_entry` must not be called concurrently.
    pub(self) unsafe fn clear_entry(&self, entry: NonNull<TimerShared>) {
        unsafe {
            // Check if this timer is in the buckets or the wheel
            let in_buckets = entry.as_ref().is_in_buckets();

            if in_buckets {
                // Timer is in buckets - remove it from the bucket Vec before the entry is freed.
                // This prevents use-after-free when the TimerShared (embedded in TimerEntry) is dropped.
                let registered_when = entry.as_ref().registered_when();
                let entry_handle = entry.as_ref().handle();
                self.inner.buckets.try_remove(registered_when, entry_handle);
            } else {
                // Timer is in the wheel - need driver lock for safe removal
                let mut lock = self.inner.lock();

                if entry.as_ref().might_be_registered() {
                    lock.wheel.remove(entry);
                }

                entry.as_ref().handle().fire(Ok(()));
            }
        }
    }

    /// Removes and re-adds an entry to the driver.
    ///
    /// SAFETY: The timer must be either unregistered, or registered with this
    /// driver. No other threads are allowed to concurrently manipulate the
    /// timer at all (the current thread should hold an exclusive reference to
    /// the `TimerEntry`)
    pub(self) unsafe fn reregister(
        &self,
        unpark: &IoHandle,
        new_tick: u64,
        entry: NonNull<TimerShared>,
    ) {
        // Check if this timer was previously in buckets
        let was_in_buckets = unsafe { entry.as_ref().is_in_buckets() };

        if was_in_buckets {
            // Timer is in buckets - keep it in buckets for reset
            // Just insert at new deadline - stale copies will be skipped via registered_when check
            let entry_handle = entry.as_ref().handle();

            match self.inner.buckets.try_insert(new_tick, entry_handle) {
                timer_buckets::InsertResult::Inserted => {
                    // Always unpark for bucket insertions - the bucket maintains its own
                    // next_wake atomic, and we need to ensure the driver wakes up for
                    // bucket timers. Checking next_wake would require locking, defeating
                    // the purpose of lock-free bucket insertion.
                    unpark.unpark();
                    return;
                }
                timer_buckets::InsertResult::Elapsed(handle) => {
                    // Timer already elapsed - but update registered_when so future resets work
                    unsafe {
                        entry.as_ref().set_registered_when(new_tick);
                        handle.fire(Ok(()));
                    };
                    return;
                }
                timer_buckets::InsertResult::OutOfRange(_handle) => {
                    // New deadline is >120s, must move to wheel
                    unsafe { entry.as_ref().handle().unmark_in_buckets() };
                    // Fall through to wheel path below
                }
            }
        } else {
            // Timer was NOT in buckets (either new or was in wheel)
            // Try buckets first for the new deadline
            let entry_handle = entry.as_ref().handle();

            match self.inner.buckets.try_insert(new_tick, entry_handle) {
                timer_buckets::InsertResult::Inserted => {
                    // Successfully inserted in buckets
                    // If timer was previously in wheel, it will remain there as a stale entry
                    // The wheel will skip it when it sees in_buckets = true

                    // Always unpark for bucket insertions - the bucket maintains its own
                    // next_wake atomic, and we need to ensure the driver wakes up for
                    // bucket timers. Checking next_wake would require locking, defeating
                    // the purpose of lock-free bucket insertion.
                    unpark.unpark();
                    return;
                }
                timer_buckets::InsertResult::Elapsed(handle) => {
                    // Timer already elapsed - but update registered_when so future resets work
                    unsafe {
                        entry.as_ref().set_registered_when(new_tick);
                        handle.fire(Ok(()));
                    };
                    return;
                }
                timer_buckets::InsertResult::OutOfRange(_handle) => {
                    // Fall through to wheel path
                }
            }
        }

        // Timer didn't fit in buckets (>120s) - use wheel
        let waker = unsafe {
            let mut lock = self.inner.lock();

            // Remove from wheel if it's already there
            if unsafe { entry.as_ref().might_be_registered() } {
                lock.wheel.remove(entry);
            }

            // Now that we have exclusive control of this entry, mint a handle to reinsert it.
            let entry = entry.as_ref().handle();

            if self.is_shutdown() {
                unsafe { entry.fire(Err(crate::time::error::Error::shutdown())) }
            } else {
                unsafe { entry.set_expiration(new_tick) };

                // Note: We don't have to worry about racing with some other resetting
                // thread, because add_entry and reregister require exclusive control of
                // the timer entry.
                match unsafe { lock.wheel.insert(entry) } {
                    Ok(when) => {
                        if lock
                            .next_wake
                            .map(|next_wake| when < next_wake.get())
                            .unwrap_or(true)
                        {
                            unpark.unpark();
                        }

                        None
                    }
                    Err((entry, crate::time::error::InsertError::Elapsed)) => unsafe {
                        entry.fire(Ok(()))
                    },
                }
            }

            // Must release lock before invoking waker to avoid the risk of deadlock.
        };

        // The timer was fired synchronously as a result of the reregistration.
        // Wake the waker; this is needed because we might reset _after_ a poll,
        // and otherwise the task won't be awoken to poll again.
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    cfg_test_util! {
        fn did_wake(&self) -> bool {
            self.inner.did_wake.swap(false, Ordering::SeqCst)
        }
    }
}

// ===== impl Inner =====

impl Inner {
    /// Locks the driver's inner structure
    pub(super) fn lock(&self) -> crate::loom::sync::MutexGuard<'_, InnerState> {
        self.state.lock()
    }

    // Check whether the driver has been shutdown
    pub(super) fn is_shutdown(&self) -> bool {
        self.is_shutdown.load(Ordering::SeqCst)
    }
}

impl fmt::Debug for Inner {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Inner").finish()
    }
}

#[cfg(test)]
mod tests;
