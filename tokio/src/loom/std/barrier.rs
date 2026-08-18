//! A `Barrier` that provides `wait_timeout`.
//!
//! This implementation mirrors that of the Rust standard library.

use crate::loom::sync::{Condvar, Mutex};
use std::fmt;
use std::time::{Duration, Instant};

/// A barrier enables multiple threads to synchronize the beginning
/// of some computation.
///
/// # Examples
///
/// ```
/// # #[cfg(not(target_family = "wasm"))]
/// # {
/// use std::sync::{Arc, Barrier};
/// use std::thread;
///
/// let mut handles = Vec::with_capacity(10);
/// let barrier = Arc::new(Barrier::new(10));
/// for _ in 0..10 {
///     let c = Arc::clone(&barrier);
///     // The same messages will be printed together.
///     // You will NOT see any interleaving.
///     handles.push(thread::spawn(move|| {
///         println!("before wait");
///         c.wait();
///         println!("after wait");
///     }));
/// }
/// // Wait for other threads to finish.
/// for handle in handles {
///     handle.join().unwrap();
/// }
/// # }
/// ```
pub(crate) struct Barrier {
    lock: Mutex<BarrierState>,
    cvar: Condvar,
    num_threads: usize,
}

// The inner state of a double barrier
struct BarrierState {
    count: usize,
    generation_id: usize,
}

/// A `BarrierWaitResult` is returned by [`Barrier::wait()`] when all threads
/// in the [`Barrier`] have rendezvoused.
///
/// # Examples
///
/// ```
/// use std::sync::Barrier;
///
/// let barrier = Barrier::new(1);
/// let barrier_wait_result = barrier.wait();
/// ```
pub(crate) struct BarrierWaitResult(bool);

impl fmt::Debug for Barrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Barrier").finish_non_exhaustive()
    }
}

impl Barrier {
    /// Creates a new barrier that can block a given number of threads.
    ///
    /// A barrier will block `n`-1 threads which call [`wait()`] and then wake
    /// up all threads at once when the `n`th thread calls [`wait()`].
    ///
    /// [`wait()`]: Barrier::wait
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Barrier;
    ///
    /// let barrier = Barrier::new(10);
    /// ```
    #[must_use]
    pub(crate) fn new(n: usize) -> Barrier {
        Barrier {
            lock: Mutex::new(BarrierState {
                count: 0,
                generation_id: 0,
            }),
            cvar: Condvar::new(),
            num_threads: n,
        }
    }

    /// Blocks the current thread until all threads have rendezvoused here.
    ///
    /// Barriers are re-usable after all threads have rendezvoused once, and can
    /// be used continuously.
    ///
    /// A single (arbitrary) thread will receive a [`BarrierWaitResult`] that
    /// returns `true` from [`BarrierWaitResult::is_leader()`] when returning
    /// from this function, and all other threads will receive a result that
    /// will return `false` from [`BarrierWaitResult::is_leader()`].
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(not(target_family = "wasm"))]
    /// # {
    /// use std::sync::{Arc, Barrier};
    /// use std::thread;
    ///
    /// let mut handles = Vec::with_capacity(10);
    /// let barrier = Arc::new(Barrier::new(10));
    /// for _ in 0..10 {
    ///     let c = Arc::clone(&barrier);
    ///     // The same messages will be printed together.
    ///     // You will NOT see any interleaving.
    ///     handles.push(thread::spawn(move|| {
    ///         println!("before wait");
    ///         c.wait();
    ///         println!("after wait");
    ///     }));
    /// }
    /// // Wait for other threads to finish.
    /// for handle in handles {
    ///     handle.join().unwrap();
    /// }
    /// # }
    /// ```
    pub(crate) fn wait(&self) -> BarrierWaitResult {
        let mut lock = self.lock.lock();
        let local_gen = lock.generation_id;
        lock.count += 1;
        if lock.count < self.num_threads {
            // We need a while loop to guard against spurious wakeups.
            // https://en.wikipedia.org/wiki/Spurious_wakeup
            while local_gen == lock.generation_id {
                lock = self.cvar.wait(lock).unwrap();
            }
            BarrierWaitResult(false)
        } else {
            lock.count = 0;
            lock.generation_id = lock.generation_id.wrapping_add(1);
            self.cvar.notify_all();
            BarrierWaitResult(true)
        }
    }

    /// Blocks the current thread until all threads have rendezvoused here for
    /// at most `timeout` duration.
    pub(crate) fn wait_timeout(&self, timeout: Duration) -> Option<BarrierWaitResult> {
        // This implementation mirrors `wait`, but with each blocking operation
        // replaced by a timeout-amenable alternative.

        let deadline = Instant::now() + timeout;

        // Acquire `self.lock` with at most `timeout` duration.
        let mut lock = loop {
            if let Some(guard) = self.lock.try_lock() {
                break guard;
            } else if Instant::now() > deadline {
                return None;
            } else {
                std::thread::yield_now();
            }
        };

        // Shrink the `timeout` to account for the time taken to acquire `lock`.
        let timeout = deadline.saturating_duration_since(Instant::now());

        let local_gen = lock.generation_id;
        lock.count += 1;
        if lock.count < self.num_threads {
            // We need a while loop to guard against spurious wakeups.
            // https://en.wikipedia.org/wiki/Spurious_wakeup
            while local_gen == lock.generation_id {
                let (guard, timeout_result) = self.cvar.wait_timeout(lock, timeout).unwrap();
                lock = guard;
                if timeout_result.timed_out() {
                    // The generation may have advanced while we were timing out,
                    // meaning the barrier was completed by another thread and we
                    // were released as part of that rendezvous rather than
                    // actually timing out. Treat it as a successful wait.
                    if local_gen != lock.generation_id {
                        break;
                    }
                    // Otherwise we really timed out. Roll back the `count += 1`
                    // performed above before returning. `count` is reset to 0
                    // only on the leader path (the `else` branch below), so a
                    // generation that never reaches `num_threads` would
                    // otherwise leak this arrival. A later generation could then
                    // cross `num_threads` with fewer than `num_threads` threads
                    // actually present and elect a spurious leader. This cannot
                    // underflow: we hold the lock and the generation is
                    // unchanged since our own `count += 1`, so no leader has
                    // reset `count`, and it therefore still includes this thread.
                    debug_assert!(lock.count > 0);
                    lock.count -= 1;
                    return None;
                }
            }
            Some(BarrierWaitResult(false))
        } else {
            lock.count = 0;
            lock.generation_id = lock.generation_id.wrapping_add(1);
            self.cvar.notify_all();
            Some(BarrierWaitResult(true))
        }
    }
}

impl fmt::Debug for BarrierWaitResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BarrierWaitResult")
            .field("is_leader", &self.is_leader())
            .finish()
    }
}

impl BarrierWaitResult {
    /// Returns `true` if this thread is the "leader thread" for the call to
    /// [`Barrier::wait()`].
    ///
    /// Only one thread will have `true` returned from their result, all other
    /// threads will have `false` returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Barrier;
    ///
    /// let barrier = Barrier::new(1);
    /// let barrier_wait_result = barrier.wait();
    /// println!("{:?}", barrier_wait_result.is_leader());
    /// ```
    #[must_use]
    pub(crate) fn is_leader(&self) -> bool {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Barrier;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    // A `wait_timeout` that times out must roll back the arrival it recorded so
    // it does not leak into `count`. `count` is reset to 0 only when a leader is
    // elected, so a leaked arrival would let a later generation cross
    // `num_threads` with fewer than `num_threads` threads actually present and
    // elect a spurious leader.
    #[test]
    fn wait_timeout_rolls_back_arrival() {
        let barrier = Barrier::new(2);

        // A lone thread times out: only 1 of the 2 required threads arrived.
        assert!(barrier.wait_timeout(Duration::from_millis(50)).is_none());

        // A second lone thread must also time out. If the first arrival had
        // leaked, this call would push `count` from 1 to `num_threads` and
        // wrongly be elected leader instead of timing out.
        assert!(barrier.wait_timeout(Duration::from_millis(50)).is_none());

        // A genuine rendezvous still completes and elects exactly one leader.
        let barrier = Arc::new(Barrier::new(2));
        let other = barrier.clone();
        let handle = thread::spawn(move || other.wait_timeout(Duration::from_secs(10)));
        let a = barrier
            .wait_timeout(Duration::from_secs(10))
            .expect("rendezvous timed out");
        let b = handle.join().unwrap().expect("rendezvous timed out");
        assert!(a.is_leader() ^ b.is_leader());
    }
}
