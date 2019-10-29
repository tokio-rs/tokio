use crate::executor::loom::sync::atomic::AtomicUsize;
use crate::executor::loom::sync::{Arc, Condvar, Mutex};
use crate::executor::park::{Park, Unpark};

use std::marker::PhantomData;
use std::mem;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::task::{RawWaker, RawWakerVTable, Waker};
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
pub struct ParkThread {
    _anchor: PhantomData<Rc<()>>,
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

struct Parker {
    unparker: Arc<Inner>,
}

/// Unblocks a thread that was blocked by `ParkThread`.
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
    static CURRENT_PARKER: Parker = Parker::new();
}

// ==== impl Parker ====

impl Parker {
    pub(crate) fn new() -> Self {
        Self {
            unparker: Arc::new(Inner {
                state: AtomicUsize::new(IDLE),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
            }),
        }
    }

    pub(crate) fn unparker(&self) -> &Arc<Inner> {
        &self.unparker
    }

    pub(crate) fn park(&self) -> Result<(), ParkError> {
        self.unparker.park(None)
    }

    pub(crate) fn park_timeout(&self, timeout: Duration) -> Result<(), ParkError> {
        self.unparker.park(Some(timeout))
    }
}

// ==== impl Inner ====

impl Inner {
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn into_raw(this: Arc<Inner>) -> *const () {
        Arc::into_raw(this) as *const ()
    }

    pub(crate) unsafe fn from_raw(ptr: *const ()) -> Arc<Inner> {
        Arc::from_raw(ptr as *const Inner)
    }

    /// Park the current thread for at most `dur`.
    pub(crate) fn park(&self, timeout: Option<Duration>) -> Result<(), ParkError> {
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

    pub(crate) fn unpark(&self) {
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
    where
        F: FnOnce(&Parker) -> R,
    {
        CURRENT_PARKER.with(|inner| f(inner))
    }
}

impl Park for ParkThread {
    type Unpark = UnparkThread;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        let inner = self.with_current(|inner| inner.unparker().clone());
        UnparkThread { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.with_current(|inner| inner.park())?;
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.with_current(|inner| inner.park_timeout(duration))?;
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

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker);

impl UnparkThread {
    pub(crate) fn into_waker(self) -> Waker {
        unsafe {
            let raw = unparker_to_raw_waker(self.inner);
            Waker::from_raw(raw)
        }
    }
}

unsafe fn unparker_to_raw_waker(unparker: Arc<Inner>) -> RawWaker {
    RawWaker::new(Inner::into_raw(unparker), &VTABLE)
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
