use tokio_executor::park::{Park, Unpark};

use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

/// Parks the thread.
#[derive(Debug)]
pub struct DefaultPark {
    inner: Arc<Inner>,
}

/// Unparks threads that were parked by `DefaultPark`.
#[derive(Debug)]
pub struct DefaultUnpark {
    inner: Arc<Inner>,
}

/// Error returned by [`ParkThread`]
///
/// This currently is never returned, but might at some point in the future.
///
/// [`ParkThread`]: struct.ParkThread.html
#[derive(Debug)]
pub struct ParkError {
    _p: (),
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

// ===== impl DefaultPark =====

impl DefaultPark {
    /// Creates a new `DefaultPark` instance.
    pub fn new() -> DefaultPark {
        let inner = Arc::new(Inner {
            state: AtomicUsize::new(IDLE),
            mutex: Mutex::new(()),
            condvar: Condvar::new(),
        });

        DefaultPark { inner }
    }
}

impl Park for DefaultPark {
    type Unpark = DefaultUnpark;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        let inner = self.inner.clone();
        DefaultUnpark { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park(None)
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.inner.park(Some(duration))
    }
}

// ===== impl DefaultUnpark =====

impl Unpark for DefaultUnpark {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

impl Inner {
    /// Park the current thread for at most `dur`.
    fn park(&self, timeout: Option<Duration>) -> Result<(), ParkError> {
        // If currently notified, then we skip sleeping. This is checked outside
        // of the lock to avoid acquiring a mutex if not necessary.
        match self.state.compare_and_swap(NOTIFY, IDLE, SeqCst) {
            NOTIFY => return Ok(()),
            IDLE => {},
            _ => unreachable!(),
        }

        // If the duration is zero, then there is no need to actually block
        if let Some(ref dur) = timeout {
            if *dur == Duration::from_millis(0) {
                return Ok(());
            }
        }

        // The state is currently idle, so obtain the lock and then try to
        // transition to a sleeping state.
        let mut m = self.mutex.lock().unwrap();

        // Transition to sleeping
        match self.state.compare_and_swap(IDLE, SLEEP, SeqCst) {
            NOTIFY => {
                // Notified before we could sleep, consume the notification and
                // exit
                self.state.store(IDLE, SeqCst);
                return Ok(());
            }
            IDLE => {},
            _ => unreachable!(),
        }

        m = match timeout {
            Some(timeout) => self.condvar.wait_timeout(m, timeout).unwrap().0,
            None => self.condvar.wait(m).unwrap(),
        };

        // Transition back to idle. If the state has transitione dto `NOTIFY`,
        // this will consume that notification
        self.state.store(IDLE, SeqCst);

        // Explicitly drop the mutex guard. There is no real point in doing it
        // except that I find it helpful to make it explicit where we want the
        // mutex to unlock.
        drop(m);

        Ok(())
    }

    fn unpark(&self) {
        // First, try transitioning from IDLE -> NOTIFY, this does not require a
        // lock.
        match self.state.compare_and_swap(IDLE, NOTIFY, SeqCst) {
            IDLE | NOTIFY => return,
            SLEEP => {}
            _ => unreachable!(),
        }

        // The other half is sleeping, this requires a lock
        let _m = self.mutex.lock().unwrap();

        // Transition from SLEEP -> NOTIFY
        match self.state.compare_and_swap(SLEEP, NOTIFY, SeqCst) {
            SLEEP => {}
            _ => return,
        }

        // Wakeup the sleeper
        self.condvar.notify_one();
    }
}

// ===== impl ParkError =====

impl fmt::Display for ParkError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.description().fmt(fmt)
    }
}

impl Error for ParkError {
    fn description(&self) -> &str {
        "unknown park error"
    }
}
