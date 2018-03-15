use Error;

use futures::task::AtomicTask;
use tokio_executor::Enter;
use tokio_executor::park::{Park, Unpark, ParkThread};

use std::cell::{RefCell, UnsafeCell};
use std::{cmp, fmt, ptr};
use std::time::{Duration, Instant};
use std::sync::{Arc, Weak, RwLock};
use std::sync::atomic::{AtomicUsize, AtomicPtr};
use std::sync::atomic::Ordering::SeqCst;
use std::usize;

/// The timer instance.
#[derive(Debug)]
pub struct Timer<T> {
    /// Shared state
    inner: Arc<Inner>,

    /// Timer state.
    ///
    /// Currently, this is just a set of active timer entries. This should be
    /// changed to a hierarchical hashed wheel.
    state: Vec<Arc<Entry>>,

    /// Thread parker. The `Timer` park implementation delegates to this.
    park: T,
}

/// Handle to the timer
#[derive(Debug, Clone)]
pub struct Handle {
    inner: Weak<Inner>,
}

/// Registration with a timer.
///
/// The association between a `Sleep` instance and a timer is done lazily in
/// `poll`
#[derive(Debug)]
pub struct Registration {
    entry: Arc<Entry>,
}

struct Inner {
    /// Number of active timeouts
    num: AtomicUsize,

    /// Head of the "process" linked list.
    process_head: AtomicPtr<Entry>,

    /// Unparks the timer thread.
    unpark: Box<Unpark>,
}

#[derive(Debug)]
pub struct Entry {
    /// Timer internals
    inner: Weak<Inner>,

    /// Next entry in the "process" linked list.
    /// `Sleep` deadline
    deadline: Instant,

    /// Task to notify once the deadline is reached.
    task: AtomicTask,

    /// Tracks the entry state
    state: AtomicUsize,

    /// Represents a strong Arc ref.
    next: UnsafeCell<*mut Entry>,
}

/// Maximum number of timeouts the system can handle concurrently.
const MAX_TIMEOUTS: usize = usize::MAX >> 1;

/// Flag indicating a timer entry has elapsed
const ELAPSED: usize = 1;

/// Flag indicating a timer entry is in the "process" queue
const QUEUED: usize = 1 << 2;

/// Tracks the timer for the current execution context.
thread_local!(static CURRENT_TIMER: RefCell<Option<Handle>> = RefCell::new(None));

// ===== impl Timer =====

impl Timer<ParkThread> {
    pub fn new() -> Self {
        Timer::new_with_park(ParkThread::new())
    }
}

impl<T> Timer<T>
where T: Park
{
    pub fn new_with_park(park: T) -> Self {
        let unpark = Box::new(park.unpark());
        let inner = Arc::new(Inner::new(unpark));

        Timer {
            inner,
            state: vec![],
            park,
        }
    }

    /// Returns the instant at which the next timeout expires.
    fn next_expiration(&self) -> Option<Instant> {
        self.state.iter()
            .filter_map(|entry| entry.deadline())
            .min()
    }

    /// Run timer related logic
    fn process(&mut self) {
        self.process_queue();

        let now = Instant::now();

        for i in (0..self.state.len()).rev() {
            if self.state[i].deadline <= now {
                self.state[i].fire();
                self.state.remove(i);
            }
        }
    }

    /// Process the entry queue
    ///
    /// This handles adding and canceling timeouts.
    fn process_queue(&mut self) {
        let mut ptr = self.inner.process_head.swap(ptr::null_mut(), SeqCst);

        while !ptr.is_null() {
            let entry = unsafe { Arc::from_raw(ptr) };

            // Get the next entry
            ptr = unsafe { (*entry.next.get()) };

            // Check the entry state
            if entry.is_elapsed() {
                self.clear_entry(entry);
            } else {
                self.add_entry(entry);
            }
        }
    }

    fn clear_entry(&mut self, entry: Arc<Entry>) {
        self.state.retain(|e| {
            &**e as *const _ != &*entry as *const _
        })
    }

    fn add_entry(&mut self, entry: Arc<Entry>) {
        self.state.push(entry);
    }
}

impl<T> Park for Timer<T>
where T: Park,
{
    type Unpark = T::Unpark;
    type Error = T::Error;

    fn unpark(&self) -> Self::Unpark {
        self.park.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        match self.next_expiration() {
            Some(deadline) => {
                let now = Instant::now();

                if deadline > now {
                    self.park.park_timeout(deadline - now)?;
                }
            }
            None => {
                self.park.park()?;
            }
        }

        self.process();

        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        // self.park.park_timeout(duration)
        match self.next_expiration() {
            Some(deadline) => {
                let now = Instant::now();

                if deadline > now {
                    self.park.park_timeout(cmp::min(deadline - now, duration))?;
                }
            }
            None => {
                self.park.park_timeout(duration)?;
            }
        }

        self.process();

        Ok(())
    }
}

impl<T> Drop for Timer<T> {
    fn drop(&mut self) {
        // * Shutdown processing queue
        // * Cancel all active timeouts
        unimplemented!();
    }
}

/// Set the default timer for the duration of the closure
///
/// # Panics
///
/// This function panics if there already is a default timer set.
pub fn with_default<F, R>(handle: &Handle, enter: &mut Enter, f: F) -> R
where F: FnOnce(&mut Enter) -> R
{
    // Ensure that the timer is removed from the thread-local context
    // when leaving the scope. This handles cases that involve panicking.
    struct Reset;

    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT_TIMER.with(|current| {
                let mut current = current.borrow_mut();
                *current = None;
            });
        }
    }

    // This ensures the value for the current timer gets reset even if there is
    // a panic.
    let _r = Reset;

    CURRENT_TIMER.with(|current| {
        {
            let mut current = current.borrow_mut();
            assert!(current.is_none(), "default Tokio timer already set \
                    for execution context");
            *current = Some(handle.clone());
        }

        f(enter)
    })
}

// ===== impl Handle =====

impl Handle {
    /// Returns a handle to the current timer.
    pub fn current() -> Handle {
        Handle::try_current()
            .unwrap_or(Handle { inner: Weak::new() })
    }

    /// Try to get a handle to the current timer.
    ///
    /// Returns `Err` if no handle is found.
    pub(crate) fn try_current() -> Result<Handle, Error> {
        CURRENT_TIMER.with(|current| {
            match *current.borrow() {
                Some(ref handle) => Ok(handle.clone()),
                None => Err(Error::shutdown()),
            }
        })
    }
}

// ===== impl Registration =====

impl Registration {
    pub fn new(deadline: Instant) -> Result<Registration, Error> {
        let handle = Handle::try_current()?;
        Registration::new_with_handle(deadline, handle)
    }

    fn new_with_handle(deadline: Instant, handle: Handle)
        -> Result<Registration, Error>
    {
        let inner = match handle.inner.upgrade() {
            Some(inner) => inner,
            None => return Err(Error::shutdown()),
        };

        // Increment the number of active timeouts
        inner.increment()?;

        let entry = Arc::new(Entry {
            inner: handle.inner.clone(),
            deadline: deadline,
            task: AtomicTask::new(),
            state: AtomicUsize::new(0),
            next: UnsafeCell::new(ptr::null_mut()),
        });

        inner.queue(&entry);

        Ok(Registration { entry })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if ELAPSED == ELAPSED & self.entry.state.fetch_or(ELAPSED, SeqCst) {
            // Nothing more to do
            return;
        }

        let inner = match self.entry.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        inner.queue(&self.entry);
    }
}

// ===== impl Inner =====

impl Inner {
    fn new(unpark: Box<Unpark>) -> Inner {
        Inner {
            num: AtomicUsize::new(0),
            process_head: AtomicPtr::new(ptr::null_mut()),
            unpark,
        }
    }

    /// Increment the number of active timeouts
    fn increment(&self) -> Result<(), Error> {
        let mut curr = self.num.load(SeqCst);

        loop {
            if curr == MAX_TIMEOUTS {
                return Err(Error::at_capacity());
            }

            let actual = self.num.compare_and_swap(curr, curr + 1, SeqCst);

            if curr == actual {
                return Ok(());
            }

            curr = actual;
        }
    }

    /// Decrement the number of active timeouts
    fn decrement(&self) {
        self.num.fetch_sub(1, SeqCst);
    }

    /// Queues an entry for processing
    fn queue(&self, entry: &Arc<Entry>) {
        // First, set the queued bit on the entry
        if QUEUED == QUEUED & entry.state.fetch_or(QUEUED, SeqCst) {
            // Already queued
            return;
        }

        let ptr = Arc::into_raw(entry.clone()) as *mut _;

        let mut curr = self.process_head.load(SeqCst);

        loop {
            // Update the `next` pointer. This is safe because setting the queued
            // bit is a "lock" on this field.
            unsafe {
                *(entry.next.get()) = curr;
            }

            let actual = self.process_head.compare_and_swap(curr, ptr, SeqCst);

            if actual == curr {
                return;
            }

            curr = actual;
        }

        // The timer is notified so that it can process the timeout
        self.unpark.unpark();
    }
}

impl fmt::Debug for Inner {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Inner")
            .finish()
    }
}

// ===== impl Entry =====

impl Entry {
    fn deadline(&self) -> Option<Instant> {
        if self.is_elapsed() {
            return None;
        }

        Some(self.deadline)
    }

    fn is_elapsed(&self) -> bool {
        ELAPSED == ELAPSED & self.state.load(SeqCst)
    }

    fn fire(&self) {
        if ELAPSED == ELAPSED & self.state.fetch_or(ELAPSED, SeqCst) {
            return;
        }

        self.task.notify();
    }
}
