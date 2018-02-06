use futures::executor::Notify;

use std::fmt;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Puts the current thread to sleep.
pub trait Sleep {
    /// Wake up handle.
    type Wakeup: Wakeup;

    /// Get a new `Wakeup` handle.
    fn wakeup(&self) -> Self::Wakeup;

    /// Put the current thread to sleep.
    fn sleep(&mut self);

    /// Put the current thread to sleep for at most `duration`.
    fn sleep_timeout(&mut self, duration: Duration);
}

/// Wake up a sleeping thread.
pub trait Wakeup: Clone + Send + 'static {
    /// Wake up the sleeping thread.
    fn wakeup(&self);
}

/// Blocks the current thread
pub struct BlockThread {
    state: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

const IDLE: usize = 0;
const NOTIFY: usize = 1;
const SLEEP: usize = 2;

thread_local! {
    static CURRENT_THREAD_NOTIFY: Arc<BlockThread> = Arc::new(BlockThread {
        state: AtomicUsize::new(IDLE),
        mutex: Mutex::new(()),
        condvar: Condvar::new(),
    });
}

// ===== impl BlockThread =====

impl BlockThread {
    pub fn with_current<F, R>(f: F) -> R
        where F: FnOnce(&Arc<BlockThread>) -> R,
    {
        CURRENT_THREAD_NOTIFY.with(|notify| f(notify))
    }

    pub fn park(&self) {
        self.park_timeout(None);
    }

    pub fn park_timeout(&self, dur: Option<Duration>) {
        // If currently notified, then we skip sleeping. This is checked outside
        // of the lock to avoid acquiring a mutex if not necessary.
        match self.state.compare_and_swap(NOTIFY, IDLE, Ordering::SeqCst) {
            NOTIFY => return,
            IDLE => {},
            _ => unreachable!(),
        }

        // The state is currently idle, so obtain the lock and then try to
        // transition to a sleeping state.
        let mut m = self.mutex.lock().unwrap();

        // Transition to sleeping
        match self.state.compare_and_swap(IDLE, SLEEP, Ordering::SeqCst) {
            NOTIFY => {
                // Notified before we could sleep, consume the notification and
                // exit
                self.state.store(IDLE, Ordering::SeqCst);
                return;
            }
            IDLE => {},
            _ => unreachable!(),
        }

        // Track (until, remaining)
        let mut time = dur.map(|dur| (Instant::now() + dur, dur));

        loop {
            m = match time {
                Some((until, rem)) => {
                    let (guard, _) = self.condvar.wait_timeout(m, rem).unwrap();
                    let now = Instant::now();

                    if now >= until {
                        // Timed out... exit sleep state
                        self.state.store(IDLE, Ordering::SeqCst);
                        return;
                    }

                    time = Some((until, until - now));
                    guard
                }
                None => self.condvar.wait(m).unwrap(),
            };

            // Transition back to idle, loop otherwise
            if NOTIFY == self.state.compare_and_swap(NOTIFY, IDLE, Ordering::SeqCst) {
                return;
            }
        }
    }

    fn unpark(&self) {
        // First, try transitioning from IDLE -> NOTIFY, this does not require a
        // lock.
        match self.state.compare_and_swap(IDLE, NOTIFY, Ordering::SeqCst) {
            IDLE | NOTIFY => return,
            SLEEP => {}
            _ => unreachable!(),
        }

        // The other half is sleeping, this requires a lock
        let _m = self.mutex.lock().unwrap();

        // Transition from SLEEP -> NOTIFY
        match self.state.compare_and_swap(SLEEP, NOTIFY, Ordering::SeqCst) {
            SLEEP => {}
            _ => return,
        }

        // Wakeup the sleeper
        self.condvar.notify_one();
    }
}

impl Notify for BlockThread {
    fn notify(&self, _unpark_id: usize) {
        self.unpark();
    }
}

impl<'a> Sleep for &'a Arc<BlockThread> {
    type Wakeup = Arc<BlockThread>;

    fn wakeup(&self) -> Self::Wakeup {
        (*self).clone()
    }

    fn sleep(&mut self) {
        self.park();
    }

    fn sleep_timeout(&mut self, duration: Duration) {
        self.park_timeout(Some(duration));
    }
}

impl Wakeup for Arc<BlockThread> {
    fn wakeup(&self) {
        self.unpark();
    }
}

impl fmt::Debug for BlockThread {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BlockThread").finish()
    }
}
