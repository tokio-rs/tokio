use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::{Arc, Condvar, Mutex};
use crate::runtime::park::{Park, Unpark};

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Blocks the current thread using a condition variable.
///
/// Implements the [`Park`] functionality by using a condition variable. An
/// atomic variable is also used to avoid using the condition variable if
/// possible.
///
/// The condition variable is cached in a thread-local variable and is shared
/// across all `ParkThread` instances created on the same thread. This also
/// means that an instance of `ParkThread` might be unblocked by a handle
/// associated with a different `ParkThread` instance.
#[derive(Debug)]
pub(crate) struct CachedParkThread {
    _anchor: PhantomData<Rc<()>>,
}

#[derive(Debug)]
pub(crate) struct ParkThread {
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

/// Unblocks a thread that was blocked by `ParkThread`.
#[derive(Clone, Debug)]
pub(crate) struct UnparkThread {
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
    static CURRENT_PARKER: ParkThread = ParkThread::new();
}

// ==== impl ParkThread ====

impl ParkThread {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: AtomicUsize::new(IDLE),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
            }),
        }
    }
}

impl Park for ParkThread {
    type Unpark = UnparkThread;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        let inner = self.inner.clone();
        UnparkThread { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park(None)
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.inner.park(Some(duration))
    }
}

// ==== impl Inner ====

impl Inner {
    /// Park the current thread for at most `dur`.
    fn park(&self, timeout: Option<Duration>) -> Result<(), ParkError> {
        // If currently notified, then we skip sleeping. This is checked outside
        // of the lock to avoid acquiring a mutex if not necessary.
        match self.state.compare_and_swap(NOTIFY, IDLE, Ordering::SeqCst) {
            NOTIFY => return Ok(()),
            IDLE => {}
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
                return Ok(());
            }
            IDLE => {}
            _ => unreachable!(),
        }

        m = match timeout {
            Some(timeout) => self.condvar.wait_timeout(m, timeout).unwrap().0,
            None => self.condvar.wait(m).unwrap(),
        };

        // Transition back to idle. If the state has transitioned to `NOTIFY`,
        // this will consume that notification
        self.state.store(IDLE, Ordering::SeqCst);

        // Explicitly drop the mutex guard. There is no real point in doing it
        // except that I find it helpful to make it explicit where we want the
        // mutex to unlock.
        drop(m);

        Ok(())
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

        // Transition to NOTIFY
        match self.state.swap(NOTIFY, Ordering::SeqCst) {
            SLEEP => {}
            NOTIFY => return,
            IDLE => return,
            _ => unreachable!(),
        }

        // Wakeup the sleeper
        self.condvar.notify_one();
    }
}

// ===== impl ParkThread =====

impl CachedParkThread {
    /// Create a new `ParkThread` handle for the current thread.
    ///
    /// This type cannot be moved to other threads, so it should be created on
    /// the thread that the caller intends to park.
    #[cfg(feature = "rt-threaded")]
    pub(crate) fn new() -> CachedParkThread {
        CachedParkThread {
            _anchor: PhantomData,
        }
    }

    /// Get a reference to the `ParkThread` handle for this thread.
    fn with_current<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&ParkThread) -> R,
    {
        CURRENT_PARKER.with(|inner| f(inner))
    }
}

impl Park for CachedParkThread {
    type Unpark = UnparkThread;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        self.with_current(|park_thread| park_thread.unpark())
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.with_current(|park_thread| park_thread.inner.park(None))?;
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.with_current(|park_thread| park_thread.inner.park(Some(duration)))?;
        Ok(())
    }
}

impl Default for ParkThread {
    fn default() -> Self {
        Self::new()
    }
}

// ===== impl UnparkThread =====

impl Unpark for UnparkThread {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

#[cfg(feature = "rt-threaded")]
mod waker {
    use super::{Inner, UnparkThread};
    use crate::loom::sync::Arc;

    use std::mem;
    use std::task::{RawWaker, RawWakerVTable, Waker};

    impl UnparkThread {
        pub(crate) fn into_waker(self) -> Waker {
            unsafe {
                let raw = unparker_to_raw_waker(self.inner);
                Waker::from_raw(raw)
            }
        }
    }

    impl Inner {
        #[allow(clippy::wrong_self_convention)]
        fn into_raw(this: Arc<Inner>) -> *const () {
            Arc::into_raw(this) as *const ()
        }

        unsafe fn from_raw(ptr: *const ()) -> Arc<Inner> {
            Arc::from_raw(ptr as *const Inner)
        }
    }

    unsafe fn unparker_to_raw_waker(unparker: Arc<Inner>) -> RawWaker {
        RawWaker::new(
            Inner::into_raw(unparker),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker),
        )
    }

    unsafe fn clone(raw: *const ()) -> RawWaker {
        let unparker = Inner::from_raw(raw);

        // Increment the ref count
        mem::forget(unparker.clone());

        unparker_to_raw_waker(unparker)
    }

    unsafe fn drop_waker(raw: *const ()) {
        let _ = Inner::from_raw(raw);
    }

    unsafe fn wake(raw: *const ()) {
        let unparker = Inner::from_raw(raw);
        unparker.unpark();
    }

    unsafe fn wake_by_ref(raw: *const ()) {
        let unparker = Inner::from_raw(raw);
        unparker.unpark();

        // We don't actually own a reference to the unparker
        mem::forget(unparker);
    }
}
