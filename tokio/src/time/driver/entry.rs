use crate::loom::sync::atomic::AtomicU64;
use crate::sync::AtomicWaker;
use crate::time::driver::{Handle, Inner};
use crate::time::{Duration, Error, Instant};

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Weak};
use std::task::{self, Poll};
use std::u64;

/// Internal state shared between a `Delay` instance and the timer.
///
/// This struct is used as a node in two intrusive data structures:
///
/// * An atomic stack used to signal to the timer thread that the entry state
///   has changed. The timer thread will observe the entry on this stack and
///   perform any actions as necessary.
///
/// * A doubly linked list used **only** by the timer thread. Each slot in the
///   timer wheel is a head pointer to the list of entries that must be
///   processed during that timer tick.
#[derive(Debug)]
pub(crate) struct Entry {
    /// Only accessed from `Registration`.
    time: CachePadded<UnsafeCell<Time>>,

    /// Timer internals. Using a weak pointer allows the timer to shutdown
    /// without all `Delay` instances having completed.
    ///
    /// When `None`, the entry has not yet been linked with a timer instance.
    inner: Weak<Inner>,

    /// Tracks the entry state. This value contains the following information:
    ///
    /// * The deadline at which the entry must be "fired".
    /// * A flag indicating if the entry has already been fired.
    /// * Whether or not the entry transitioned to the error state.
    ///
    /// When an `Entry` is created, `state` is initialized to the instant at
    /// which the entry must be fired. When a timer is reset to a different
    /// instant, this value is changed.
    state: AtomicU64,

    /// Task to notify once the deadline is reached.
    waker: AtomicWaker,

    /// True when the entry is queued in the "process" stack. This value
    /// is set before pushing the value and unset after popping the value.
    ///
    /// TODO: This could possibly be rolled up into `state`.
    pub(super) queued: AtomicBool,

    /// Next entry in the "process" linked list.
    ///
    /// Access to this field is coordinated by the `queued` flag.
    ///
    /// Represents a strong Arc ref.
    pub(super) next_atomic: UnsafeCell<*mut Entry>,

    /// When the entry expires, relative to the `start` of the timer
    /// (Inner::start). This is only used by the timer.
    ///
    /// A `Delay` instance can be reset to a different deadline by the thread
    /// that owns the `Delay` instance. In this case, the timer thread will not
    /// immediately know that this has happened. The timer thread must know the
    /// last deadline that it saw as it uses this value to locate the entry in
    /// its wheel.
    ///
    /// Once the timer thread observes that the instant has changed, it updates
    /// the wheel and sets this value. The idea is that this value eventually
    /// converges to the value of `state` as the timer thread makes updates.
    when: UnsafeCell<Option<u64>>,

    /// Next entry in the State's linked list.
    ///
    /// This is only accessed by the timer
    pub(super) next_stack: UnsafeCell<Option<Arc<Entry>>>,

    /// Previous entry in the State's linked list.
    ///
    /// This is only accessed by the timer and is used to unlink a canceled
    /// entry.
    ///
    /// This is a weak reference.
    pub(super) prev_stack: UnsafeCell<*const Entry>,
}

/// Stores the info for `Delay`.
#[derive(Debug)]
pub(crate) struct Time {
    pub(crate) deadline: Instant,
    pub(crate) duration: Duration,
}

/// Flag indicating a timer entry has elapsed
const ELAPSED: u64 = 1 << 63;

/// Flag indicating a timer entry has reached an error state
const ERROR: u64 = u64::MAX;

// ===== impl Entry =====

impl Entry {
    pub(crate) fn new(handle: &Handle, deadline: Instant, duration: Duration) -> Arc<Entry> {
        let inner = handle.inner().unwrap();
        let entry: Entry;

        // Increment the number of active timeouts
        if inner.increment().is_err() {
            entry = Entry::new2(deadline, duration, Weak::new(), ERROR)
        } else {
            let when = inner.normalize_deadline(deadline);
            let state = if when <= inner.elapsed() {
                ELAPSED
            } else {
                when
            };
            entry = Entry::new2(deadline, duration, Arc::downgrade(&inner), state);
        }

        let entry = Arc::new(entry);
        if inner.queue(&entry).is_err() {
            entry.error();
        }

        entry
    }

    /// Only called by `Registration`
    pub(crate) fn time_ref(&self) -> &Time {
        unsafe { &*self.time.0.get() }
    }

    /// Only called by `Registration`
    #[allow(clippy::mut_from_ref)] // https://github.com/rust-lang/rust-clippy/issues/4281
    pub(crate) unsafe fn time_mut(&self) -> &mut Time {
        &mut *self.time.0.get()
    }

    /// The current entry state as known by the timer. This is not the value of
    /// `state`, but lets the timer know how to converge its state to `state`.
    pub(crate) fn when_internal(&self) -> Option<u64> {
        unsafe { *self.when.get() }
    }

    pub(crate) fn set_when_internal(&self, when: Option<u64>) {
        unsafe {
            *self.when.get() = when;
        }
    }

    /// Called by `Timer` to load the current value of `state` for processing
    pub(crate) fn load_state(&self) -> Option<u64> {
        let state = self.state.load(SeqCst);

        if is_elapsed(state) {
            None
        } else {
            Some(state)
        }
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        let state = self.state.load(SeqCst);
        is_elapsed(state)
    }

    pub(crate) fn fire(&self, when: u64) {
        let mut curr = self.state.load(SeqCst);

        loop {
            if is_elapsed(curr) || curr > when {
                return;
            }

            let next = ELAPSED | curr;
            let actual = self.state.compare_and_swap(curr, next, SeqCst);

            if curr == actual {
                break;
            }

            curr = actual;
        }

        self.waker.wake();
    }

    pub(crate) fn error(&self) {
        // Only transition to the error state if not currently elapsed
        let mut curr = self.state.load(SeqCst);

        loop {
            if is_elapsed(curr) {
                return;
            }

            let next = ERROR;

            let actual = self.state.compare_and_swap(curr, next, SeqCst);

            if curr == actual {
                break;
            }

            curr = actual;
        }

        self.waker.wake();
    }

    pub(crate) fn cancel(entry: &Arc<Entry>) {
        let state = entry.state.fetch_or(ELAPSED, SeqCst);

        if is_elapsed(state) {
            // Nothing more to do
            return;
        }

        // If registered with a timer instance, try to upgrade the Arc.
        let inner = match entry.upgrade_inner() {
            Some(inner) => inner,
            None => return,
        };

        let _ = inner.queue(entry);
    }

    pub(crate) fn poll_elapsed(&self, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        let mut curr = self.state.load(SeqCst);

        if is_elapsed(curr) {
            return Poll::Ready(if curr == ERROR {
                Err(Error::shutdown())
            } else {
                Ok(())
            });
        }

        self.waker.register_by_ref(cx.waker());

        curr = self.state.load(SeqCst);

        if is_elapsed(curr) {
            return Poll::Ready(if curr == ERROR {
                Err(Error::shutdown())
            } else {
                Ok(())
            });
        }

        Poll::Pending
    }

    /// Only called by `Registration`
    pub(crate) fn reset(entry: &mut Arc<Entry>) {
        let inner = match entry.upgrade_inner() {
            Some(inner) => inner,
            None => return,
        };

        let deadline = entry.time_ref().deadline;
        let when = inner.normalize_deadline(deadline);
        let elapsed = inner.elapsed();

        let mut curr = entry.state.load(SeqCst);
        let mut notify;

        loop {
            // In these two cases, there is no work to do when resetting the
            // timer. If the `Entry` is in an error state, then it cannot be
            // used anymore. If resetting the entry to the current value, then
            // the reset is a noop.
            if curr == ERROR || curr == when {
                return;
            }

            let next;

            if when <= elapsed {
                next = ELAPSED;
                notify = !is_elapsed(curr);
            } else {
                next = when;
                notify = true;
            }

            let actual = entry.state.compare_and_swap(curr, next, SeqCst);

            if curr == actual {
                break;
            }

            curr = actual;
        }

        if notify {
            let _ = inner.queue(entry);
        }
    }

    fn new2(deadline: Instant, duration: Duration, inner: Weak<Inner>, state: u64) -> Self {
        Self {
            time: CachePadded(UnsafeCell::new(Time { deadline, duration })),
            inner,
            waker: AtomicWaker::new(),
            state: AtomicU64::new(state),
            queued: AtomicBool::new(false),
            next_atomic: UnsafeCell::new(ptr::null_mut()),
            when: UnsafeCell::new(None),
            next_stack: UnsafeCell::new(None),
            prev_stack: UnsafeCell::new(ptr::null_mut()),
        }
    }

    fn upgrade_inner(&self) -> Option<Arc<Inner>> {
        self.inner.upgrade()
    }
}

fn is_elapsed(state: u64) -> bool {
    state & ELAPSED == ELAPSED
}

impl Drop for Entry {
    fn drop(&mut self) {
        let inner = match self.upgrade_inner() {
            Some(inner) => inner,
            None => return,
        };

        inner.decrement();
    }
}

unsafe impl Send for Entry {}
unsafe impl Sync for Entry {}

#[cfg_attr(target_arch = "x86_64", repr(align(128)))]
#[cfg_attr(not(target_arch = "x86_64"), repr(align(64)))]
#[derive(Debug)]
struct CachePadded<T>(T);
