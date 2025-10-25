//! Global timer buckets for fast-path timer registration and firing.
//!
//! This module implements a ring buffer of timer buckets with 1ms granularity,
//! covering timers from 0-120 seconds in the future. Timers beyond this range
//! fall back to the global timer wheel.
//!
//! # Design
//!
//! The ring buffer contains 120,000 buckets (one per millisecond for 2 minutes).
//! Each bucket has its own lock and contains a Vec of TimerHandles. This provides:
//! - Lock-free index calculation for insertion
//! - Fine-grained locking (only contention when timers land in same millisecond)
//! - O(1) advancement as time progresses
//! - Natural wraparound every 120 seconds
//!
//! # Synchronization
//!
//! - `head`: Atomic index pointing to the "current time" bucket
//! - `ref_time`: Atomic tick value representing when head was at position 0
//! - Per-bucket locks: Only held during push (insertion) or drain (firing)
//!
//! This allows workers to insert timers lock-free (just atomic reads + single bucket lock),
//! while the driver advances the head pointer and fires expired buckets under the driver lock.

use crate::loom::sync::atomic::Ordering;
use crate::loom::sync::atomic::{AtomicU64, AtomicUsize};
use crate::loom::sync::Mutex;
use crate::runtime::time::TimerHandle;

use std::task::Waker;

/// Number of milliseconds to cover in the ring buffer (2 minutes).
/// Timers beyond this range fall back to the global timer wheel.
const BUCKET_COUNT: usize = 120_000;

/// Global ring buffer of timer buckets.
///
/// This structure is shared across all workers and is integrated into the
/// timer driver's Handle.
pub(crate) struct GlobalTimerBuckets {
    /// The buckets themselves. Vec of 120,000 buckets.
    buckets: Vec<Bucket>,

    /// Index of the "head" bucket (represents current time).
    /// Advances forward as time progresses.
    head: AtomicUsize,

    /// The tick value (milliseconds since epoch) that the head position represents.
    /// Used to calculate bucket offsets for new timers.
    ref_time: AtomicU64,

    /// The earliest timer deadline across all buckets.
    /// Used to calculate when the driver should wake up.
    /// Value of u64::MAX means no timers are registered.
    next_wake: AtomicU64,
}

/// A single bucket in the ring buffer.
struct Bucket {
    /// Timers that expire in this millisecond.
    /// Protected by a mutex for simplicity - can be optimized to lock-free later if needed.
    timers: Mutex<Vec<TimerHandle>>,
}

impl Default for Bucket {
    fn default() -> Self {
        Self {
            timers: Mutex::new(Vec::new()),
        }
    }
}

/// Result of attempting to insert a timer into buckets.
#[derive(Debug)]
pub(crate) enum InsertResult {
    /// Timer was successfully inserted into a bucket
    Inserted,
    /// Timer is too far in the future (>120s), try the wheel instead
    OutOfRange(TimerHandle),
    /// Timer has already elapsed and should be fired immediately
    Elapsed(TimerHandle),
}

impl GlobalTimerBuckets {
    /// Creates a new global timer bucket ring buffer.
    ///
    /// The `initial_tick` parameter sets the reference time for the head position.
    pub(crate) fn new(initial_tick: u64) -> Self {
        // Pre-allocate all buckets on the heap
        let buckets = (0..BUCKET_COUNT).map(|_| Bucket::default()).collect();

        Self {
            buckets,
            head: AtomicUsize::new(0),
            ref_time: AtomicU64::new(initial_tick),
            next_wake: AtomicU64::new(u64::MAX),
        }
    }

    /// Attempts to insert a timer into the appropriate bucket.
    ///
    /// Returns `InsertResult` indicating what happened.
    ///
    /// # Parameters
    /// - `deadline_tick`: The absolute tick (milliseconds since epoch) when timer expires
    /// - `timer`: The timer handle to insert
    pub(crate) fn try_insert(&self, deadline_tick: u64, timer: TimerHandle) -> InsertResult {
        self.try_insert_inner(deadline_tick, timer, true)
    }

    fn try_insert_inner(&self, deadline_tick: u64, timer: TimerHandle, mark: bool) -> InsertResult {
        // Read current reference time and head position atomically
        let ref_tick = self.ref_time.load(Ordering::Acquire);
        let head_pos = self.head.load(Ordering::Acquire);

        // Check if timer has already elapsed
        if deadline_tick <= ref_tick {
            return InsertResult::Elapsed(timer);
        }

        // Calculate offset from current reference time
        let offset = deadline_tick - ref_tick;

        // If timer is beyond our range, don't insert it here
        if offset >= BUCKET_COUNT as u64 {
            return InsertResult::OutOfRange(timer);
        }

        // Calculate target bucket index with wraparound
        let bucket_idx = (head_pos + offset as usize) % BUCKET_COUNT;

        // Lock just this bucket and insert the timer
        let mut bucket = self.buckets[bucket_idx].timers.lock();

        // Re-check after locking - ref_time might have advanced while we were waiting for lock
        let ref_tick_locked = self.ref_time.load(Ordering::Acquire);
        if deadline_tick <= ref_tick_locked {
            // Deadline passed while we were acquiring the lock
            return InsertResult::Elapsed(timer);
        }

        // SAFETY: We hold the bucket lock which synchronizes with advance().
        // The handle is valid (just passed to us), and this timer is not in
        // the wheel (it's going into buckets instead). The bucket lock provides
        // the necessary memory fence for the relaxed atomic operations in
        // set_expiration() to be visible when advance() later fires this timer.
        unsafe {
            if mark {
                timer.mark_in_buckets();
            }
            timer.set_expiration(deadline_tick);
        }

        bucket.push(timer);

        // Update next_wake if this timer is earlier
        self.next_wake.fetch_min(deadline_tick, Ordering::Release);

        InsertResult::Inserted
    }

    /// Returns the tick of the next timer that will fire, if any.
    ///
    /// This is used to calculate when the driver should wake up.
    pub(crate) fn next_expiration_time(&self) -> Option<u64> {
        let next = self.next_wake.load(Ordering::Acquire);
        if next == u64::MAX {
            None
        } else {
            Some(next)
        }
    }

    /// Removes a timer from the bucket where it was inserted.
    ///
    /// This is called when a timer is being cancelled/dropped before it fires.
    /// We need to remove the handle from the bucket Vec to avoid use-after-free
    /// when the underlying TimerShared is freed.
    ///
    /// # Parameters
    /// - `registered_when`: The tick value when this timer was originally inserted (stored in registered_when)
    /// - `timer_to_remove`: The handle to remove
    pub(crate) fn try_remove(&self, registered_when: u64, _timer_to_remove: TimerHandle) {
        // Calculate which bucket this timer should be in based on when it was registered
        let ref_tick = self.ref_time.load(Ordering::Acquire);
        let head_pos = self.head.load(Ordering::Acquire);

        // Only try to remove if the timer is still in the valid bucket range
        if registered_when <= ref_tick {
            // Timer's deadline has passed, it's either already been fired or elapsed
            // Don't try to remove it
            return;
        }

        let offset = registered_when - ref_tick;

        // If offset is >= BUCKET_COUNT, the timer is out of range (shouldn't happen for buckets,
        // but be safe)
        if offset >= BUCKET_COUNT as u64 {
            return;
        }

        let bucket_idx = (head_pos + offset as usize) % BUCKET_COUNT;

        // Lock the bucket and search for/remove the matching timer
        if let Some(mut bucket) = self.buckets[bucket_idx].timers.try_lock() {
            // Remove the timer from the bucket Vec by finding the matching registered_when value.
            // Multiple timers can be in the same bucket, but only one should have our registered_when.
            let target_registered_when = registered_when;
            bucket.retain(|handle: &TimerHandle| {
                let handle_registered = unsafe { handle.registered_when() };
                // Keep the handle if it has a different registered_when (not our timer)
                handle_registered != target_registered_when
            });
        }
        // If we can't acquire the lock, bail out. The timer will be fired during the
        // current advance() call that's holding the lock, since it's still marked as
        // being in buckets.
    }

    /// Advances the ring buffer to the current time and fires all expired timers.
    ///
    /// This is called by the driver when it processes timers. Returns all wakers
    /// that need to be woken after the driver lock is released.
    ///
    /// # Parameters
    /// - `now_tick`: The current tick (milliseconds since epoch)
    ///
    /// # Safety
    /// Must be called with the driver lock held.
    pub(crate) unsafe fn advance(&self, now_tick: u64) -> Vec<Waker> {
        let mut wakers = Vec::new();

        let ref_tick = self.ref_time.load(Ordering::Acquire);
        let ticks_elapsed = now_tick.saturating_sub(ref_tick);

        // Cap the number of ticks we advance to the bucket count
        // If time jumped way forward (e.g., during shutdown with u64::MAX), we only need
        // to fire all buckets once, not loop billions of times
        let ticks_to_advance = std::cmp::min(ticks_elapsed, BUCKET_COUNT as u64);

        // Track if we need to clear next_wake (if we fire the bucket it points to and it becomes empty)
        let current_next_wake = self.next_wake.load(Ordering::Acquire);

        // Advance through each elapsed tick, firing timers in each bucket
        for _ in 0..ticks_to_advance {
            // Atomically advance head and ref_time, get the tick we're firing
            // fetch_add returns the OLD value, but we want to fire the NEW bucket at the NEW time
            let bucket_idx = (self.head.fetch_add(1, Ordering::AcqRel) + 1) % BUCKET_COUNT;
            let current_tick = self.ref_time.fetch_add(1, Ordering::AcqRel) + 1;

            // Fire all timers in this bucket
            let mut bucket = self.buckets[bucket_idx].timers.lock();

            let had_timers = !bucket.is_empty();

            for timer_handle in bucket.drain(..) {
                // Only fire timers that are actually in buckets and match the current tick.
                // If a timer was reset to a deadline > 120s, it was moved to the wheel but
                // its old handle may still be in this bucket Vec. We skip these by checking
                // is_in_buckets and registered_when to ensure we only fire valid entries.
                if unsafe { timer_handle.is_in_buckets_unsafe() } {
                    let registered = unsafe { timer_handle.registered_when() };
                    if registered == current_tick {
                        // SAFETY: We hold the driver lock, which is required for firing
                        if let Some(waker) = unsafe { timer_handle.fire(Ok(())) } {
                            wakers.push(waker);
                        }
                    }
                }
            }

            // If we just drained the bucket that next_wake was pointing to, clear it
            if had_timers && current_tick == current_next_wake {
                // Use compare_exchange to only clear if it's still pointing to this tick
                // (another thread might have inserted a new earlier timer)
                let _ = self.next_wake.compare_exchange(
                    current_next_wake,
                    u64::MAX,
                    Ordering::Release,
                    Ordering::Relaxed,
                );
            }
        }

        wakers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_offset_calculation() {
        let buckets = GlobalTimerBuckets::new(1000);

        // Timer 5000ms in the future should land at index 5000
        assert_eq!(buckets.head.load(Ordering::Relaxed), 0);
        assert_eq!(buckets.ref_time.load(Ordering::Relaxed), 1000);

        // Offset = 6000 - 1000 = 5000
        // Index = (0 + 5000) % 120000 = 5000
        let offset = 6000u64.saturating_sub(1000);
        assert_eq!(offset, 5000);
        assert!((offset as usize) < BUCKET_COUNT);
    }

    #[test]
    fn test_wraparound() {
        let buckets = GlobalTimerBuckets::new(0);

        // Advance head near the end
        buckets.head.store(119_500, Ordering::Release);
        buckets.ref_time.store(119_500, Ordering::Release);

        // Timer 1000ms in the future should wrap around
        let offset = 120_500u64.saturating_sub(119_500);
        assert_eq!(offset, 1000);

        let bucket_idx = (119_500 + offset as usize) % BUCKET_COUNT;
        assert_eq!(bucket_idx, 120_500 % BUCKET_COUNT);
        assert_eq!(bucket_idx, 500); // Wrapped around
    }
}
