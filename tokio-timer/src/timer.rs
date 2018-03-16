use {Error, Now, SystemNow};

use futures::Poll;
use futures::task::AtomicTask;
use tokio_executor::Enter;
use tokio_executor::park::{Park, Unpark, ParkThread};

use std::cell::{RefCell, UnsafeCell};
use std::{cmp, fmt, ptr};
use std::time::{Duration, Instant};
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicUsize, AtomicPtr};
use std::sync::atomic::Ordering::SeqCst;
use std::usize;

/// The timer instance.
#[derive(Debug)]
pub struct Timer<T, N = SystemNow> {
    /// Shared state
    inner: Arc<Inner>,

    /// Timer state.
    ///
    /// Currently, this is just a set of active timer entries. This should be
    /// changed to a hierarchical hashed wheel.
    state: Vec<Arc<Entry>>,

    /// Thread parker. The `Timer` park implementation delegates to this.
    park: T,

    /// Source of "now" instances
    now: N,
}

/// Handle to the timer
#[derive(Debug, Clone)]
pub struct Handle {
    inner: Weak<Inner>,
}

/// Return value from the `turn` method on `Timer`.
///
/// Currently this value doesn't actually provide any functionality, but it may
/// in the future give insight into what happened during `turn`.
#[derive(Debug)]
pub struct Turn(());

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

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct State(usize);

/// Maximum number of timeouts the system can handle concurrently.
const MAX_TIMEOUTS: usize = usize::MAX >> 1;

/// Flag indicating a timer entry has elapsed
const ELAPSED: usize = 1;

/// Flag indicating a timer entry has reached an error state
const ERROR: usize = 1 << 2;

/// Flag indicating a timer entry is in the "process" queue
const QUEUED: usize = 1 << 3;

/// Used to indicate that the timer has shutdown.
const SHUTDOWN: *mut Entry = 1 as *mut _;

/// Tracks the timer for the current execution context.
thread_local!(static CURRENT_TIMER: RefCell<Option<Handle>> = RefCell::new(None));

// ===== impl Timer =====

impl<T> Timer<T>
where T: Park
{
    pub fn new(park: T) -> Self {
        Timer::new_with_now(park, SystemNow::new())
    }
}

impl<T, N> Timer<T, N>
where T: Park,
      N: Now,
{
    pub fn new_with_now(park: T, now: N) -> Self {
        let unpark = Box::new(park.unpark());

        Timer {
            inner: Arc::new(Inner::new(unpark)),
            state: vec![],
            park,
            now,
        }
    }

    /// Performs one iteration of the timer loop.
    pub fn turn(&mut self, max_wait: Option<Duration>) -> Result<Turn, T::Error> {
        match max_wait {
            Some(timeout) => self.park_timeout(timeout)?,
            None => self.park()?,
        }

        Ok(Turn(()))
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

        let now = self.now.now();

        for i in (0..self.state.len()).rev() {
            if self.state[i].deadline <= now {
                // Decrement the number of outstanding timeouts
                self.inner.decrement();

                // Fire the timeout
                self.state[i].fire();

                // Remove associated timer state
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

impl Default for Timer<ParkThread, SystemNow> {
    fn default() -> Self {
        Timer::new(ParkThread::new())
    }
}

impl<T, N> Park for Timer<T, N>
where T: Park,
      N: Now,
{
    type Unpark = T::Unpark;
    type Error = T::Error;

    fn unpark(&self) -> Self::Unpark {
        self.park.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        match self.next_expiration() {
            Some(deadline) => {
                let now = self.now.now();

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
                let now = self.now.now();

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

impl<T, N> Drop for Timer<T, N> {
    fn drop(&mut self) {
        // Shutdown the processing queue
        let mut ptr = self.inner.process_head.swap(SHUTDOWN, SeqCst);

        while !ptr.is_null() {
            let entry = unsafe { Arc::from_raw(ptr) };

            // Get the next entry
            ptr = unsafe { (*entry.next.get()) };

            // The entry must be flagged as errored
            entry.error();
        }

        for entry in self.state.drain(..) {
            entry.error();
        }
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

        inner.queue(&entry)?;

        Ok(Registration { entry })
    }

    pub fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    pub fn poll_elapsed(&self) -> Poll<(), Error> {
        self.entry.poll_elapsed()
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let state: State = self.entry.state.fetch_or(ELAPSED, SeqCst).into();

        if state.is_elapsed() {
            // Nothing more to do
            return;
        }

        let inner = match self.entry.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        let _ = inner.queue(&self.entry);
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
    fn queue(&self, entry: &Arc<Entry>) -> Result<(), Error> {
        // First, set the queued bit on the entry
        let state: State = entry.state.fetch_or(QUEUED, SeqCst).into();

        if state.is_queued() {
            // Already queued, nothing more to do
            return Ok(());
        }

        let ptr = Arc::into_raw(entry.clone()) as *mut _;

        let mut curr = self.process_head.load(SeqCst);

        loop {
            if curr == SHUTDOWN {
                // Don't leak the entry node
                let _ = unsafe { Arc::from_raw(ptr) };

                return Err(Error::shutdown());
            }

            // Update the `next` pointer. This is safe because setting the queued
            // bit is a "lock" on this field.
            unsafe {
                *(entry.next.get()) = curr;
            }

            let actual = self.process_head.compare_and_swap(curr, ptr, SeqCst);

            if actual == curr {
                break;
            }

            curr = actual;
        }

        // The timer is notified so that it can process the timeout
        self.unpark.unpark();

        Ok(())
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
        let state: State = self.state.load(SeqCst).into();
        state.is_elapsed()
    }

    fn fire(&self) {
        let state: State = self.state.fetch_or(ELAPSED, SeqCst).into();

        if state.is_elapsed() {
            return;
        }

        self.task.notify();
    }

    fn error(&self) {
        // Only transition to the error state if not currently elapsed
        let mut curr: State = self.state.load(SeqCst).into();

        loop {
            if curr.is_elapsed() {
                return;
            }

            let mut next = curr;
            next.set_error();

            let actual = self.state.compare_and_swap(
                curr.into(), next.into(), SeqCst).into();

            if curr == actual {
                break;
            }

            curr = actual;
        }

        self.task.notify();
    }

    fn poll_elapsed(&self) -> Poll<(), Error> {
        use futures::Async::NotReady;

        let mut curr: State = self.state.load(SeqCst).into();

        if curr.is_elapsed() {
            if curr.is_error() {
                return Err(Error::shutdown());
            } else {
                return Ok(().into());
            }
        }

        self.task.register();

        curr = self.state.load(SeqCst).into();

        if curr.is_elapsed() {
            if curr.is_error() {
                return Err(Error::shutdown());
            } else {
                return Ok(().into());
            }
        }

        Ok(NotReady)
    }
}

// ===== impl State =====

impl State {
    fn is_elapsed(&self) -> bool {
        self.0 & ELAPSED == ELAPSED
    }

    fn is_error(&self) -> bool {
        self.0 & ERROR == ERROR
    }

    fn set_error(&mut self) {
        self.0 |= ELAPSED | ERROR;
    }

    fn is_queued(&self) -> bool {
        self.0 & QUEUED == QUEUED
    }
}

impl From<usize> for State {
    fn from(src: usize) -> Self {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> Self {
        src.0
    }
}
