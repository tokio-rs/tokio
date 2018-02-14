//! Abstraction around parking a thread. This is used by executor
//! implementations.

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Parks the current thread
pub trait Park {
    /// Unpark handle
    type Unpark: Unpark;

    /// Get a new `Unpark` handle.
    fn unpark(&self) -> Self::Unpark;

    /// Park the current thread
    fn park(&mut self);

    /// Park the current thread for at most `duration`.
    fn park_timeout(&mut self, duration: Duration);
}

/// Unpark a parked thread
pub trait Unpark: Sync + Send + 'static {
    /// Unpark up the parked thread.
    fn unpark(&self);
}

/// Parks the current thread
#[derive(Debug)]
pub struct ParkThread {
    _anchor: PhantomData<Rc<()>>,
}

/// Unparks a thread that was parked by `ParkThread`.
#[derive(Clone, Debug)]
pub struct UnparkThread {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    state: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

const IDLE: usize = 0;
const NOTIFY: usize = 1;
const SLEEP: usize = 2;

thread_local! {
    static CURRENT_PARK_THREAD: Arc<Inner> = Arc::new(Inner {
        state: AtomicUsize::new(IDLE),
        mutex: Mutex::new(()),
        condvar: Condvar::new(),
    });
}

// ===== impl ParkThread =====

impl ParkThread {
    /// Create a new `ParkThread` handle for the current thread.
    ///
    /// This type cannot be moved to other threads, so it should be created on
    /// the thread that the caller intends to park.
    pub fn new() -> ParkThread {
        ParkThread {
            _anchor: PhantomData,
        }
    }

    /// Get a reference to the `ParkThread` handle for this thread.
    fn with_current<F, R>(&self, f: F) -> R
        where F: FnOnce(&Arc<Inner>) -> R,
    {
        CURRENT_PARK_THREAD.with(|inner| f(inner))
    }
}

impl Park for ParkThread {
    type Unpark = UnparkThread;

    fn unpark(&self) -> Self::Unpark {
        let inner = self.with_current(|inner| inner.clone());
        UnparkThread { inner }
    }

    fn park(&mut self) {
        self.with_current(|inner| inner.park(None));
    }

    fn park_timeout(&mut self, duration: Duration) {
        self.with_current(|inner| inner.park(Some(duration)));
    }
}

// ===== impl UnparkThread =====

impl Unpark for UnparkThread {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

// ===== impl Inner =====

impl Inner {
    /// Park the current thread for at most `dur`.
    fn park(&self, dur: Option<Duration>) {
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
