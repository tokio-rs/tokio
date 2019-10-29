use crate::executor::loom::sync::atomic::AtomicUsize;
use crate::executor::loom::sync::{Arc, Condvar, Mutex};
use crate::executor::park::{Park, Unpark};

use std::error::Error;
use std::fmt;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

/// Parks the thread.
#[derive(Debug)]
pub(crate) struct DefaultPark {
    inner: Arc<Inner>,
}

/// Unparks threads that were parked by `DefaultPark`.
#[derive(Debug)]
pub(crate) struct DefaultUnpark {
    inner: Arc<Inner>,
}

/// Error returned by [`ParkThread`]
///
/// This currently is never returned, but might at some point in the future.
///
/// [`ParkThread`]: struct.ParkThread.html
#[derive(Debug)]
pub(crate) struct ParkError {
    _p: (),
}

const EMPTY: usize = 0;
const PARKED: usize = 1;
const NOTIFIED: usize = 2;

#[derive(Debug)]
struct Inner {
    state: AtomicUsize,
    lock: Mutex<()>,
    cvar: Condvar,
}

impl DefaultPark {
    /// Creates a new `DefaultPark` instance.
    pub(crate) fn new() -> DefaultPark {
        DefaultPark {
            inner: Arc::new(Inner {
                state: AtomicUsize::new(EMPTY),
                lock: Mutex::new(()),
                cvar: Condvar::new(),
            }),
        }
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
        self.inner.park(None);
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.inner.park(Some(duration));
        Ok(())
    }
}

impl Unpark for DefaultUnpark {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

impl fmt::Display for ParkError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "unknown park error")
    }
}

impl Error for ParkError {}

impl Inner {
    fn park(&self, timeout: Option<Duration>) {
        // If we were previously notified then we consume this notification and return quickly.
        if self
            .state
            .compare_exchange(NOTIFIED, EMPTY, SeqCst, SeqCst)
            .is_ok()
        {
            return;
        }

        // If the timeout is zero, then there is no need to actually block.
        if let Some(ref dur) = timeout {
            if *dur == Duration::from_millis(0) {
                return;
            }
        }

        // Otherwise we need to coordinate going to sleep.
        let mut _m = self.lock.lock().unwrap();

        match self.state.compare_exchange(EMPTY, PARKED, SeqCst, SeqCst) {
            Ok(_) => {}
            // Consume this notification to avoid spurious wakeups in the next park.
            Err(NOTIFIED) => {
                // We must read `state` here, even though we know it will be `NOTIFIED`. This is
                // because `unpark` may have been called again since we read `NOTIFIED` in the
                // `compare_exchange` above. We must perform an acquire operation that synchronizes
                // with that `unpark` to observe any writes it made before the call to `unpark`. To
                // do that we must read from the write it made to `state`.
                let old = self.state.swap(EMPTY, SeqCst);
                assert_eq!(old, NOTIFIED, "park state changed unexpectedly");
                return;
            }
            Err(n) => panic!("inconsistent park_timeout state: {}", n),
        }

        match timeout {
            None => {
                loop {
                    // Block the current thread on the conditional variable.
                    _m = self.cvar.wait(_m).unwrap();

                    if self
                        .state
                        .compare_exchange(NOTIFIED, EMPTY, SeqCst, SeqCst)
                        .is_ok()
                    {
                        return; // got a notification
                    }

                    // spurious wakeup, go back to sleep
                }
            }
            Some(timeout) => {
                // Wait with a timeout, and if we spuriously wake up or otherwise wake up from a
                // notification we just want to unconditionally set `state` back to `EMPTY`, either
                // consuming a notification or un-flagging ourselves as parked.
                _m = self.cvar.wait_timeout(_m, timeout).unwrap().0;

                match self.state.swap(EMPTY, SeqCst) {
                    NOTIFIED => {} // got a notification
                    PARKED => {}   // no notification
                    n => panic!("inconsistent park_timeout state: {}", n),
                }
            }
        }
    }

    fn unpark(&self) {
        // To ensure the unparked thread will observe any writes we made before this call, we must
        // perform a release operation that `park` can synchronize with. To do that we must write
        // `NOTIFIED` even if `state` is already `NOTIFIED`. That is why this must be a swap rather
        // than a compare-and-swap that returns if it reads `NOTIFIED` on failure.
        match self.state.swap(NOTIFIED, SeqCst) {
            EMPTY => return,    // no one was waiting
            NOTIFIED => return, // already unparked
            PARKED => {}        // gotta go wake someone up
            n => panic!("inconsistent state in unpark: {}", n),
        }

        // There is a period between when the parked thread sets `state` to `PARKED` (or last
        // checked `state` in the case of a spurious wakeup) and when it actually waits on `cvar`.
        // If we were to notify during this period it would be ignored and then when the parked
        // thread went to sleep it would never wake up. Fortunately, it has `lock` locked at this
        // stage so we can acquire `lock` to wait until it is ready to receive the notification.
        //
        // Releasing `lock` before the call to `notify_one` means that when the parked thread wakes
        // it doesn't get woken only to have to wait for us to release `lock`.
        drop(self.lock.lock());
        self.cvar.notify_one();
    }
}
