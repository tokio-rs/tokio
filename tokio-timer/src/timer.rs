use {Error, Handle, Now, SystemNow};
use atomic::AtomicU64;

use futures::Poll;
use futures::task::AtomicTask;
use tokio_executor::park::{Park, Unpark, ParkThread};

use std::cell::{UnsafeCell};
use std::{cmp, fmt, ptr};
use std::time::{Duration, Instant};
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicUsize, AtomicPtr};
use std::sync::atomic::Ordering::{SeqCst, Relaxed};
use std::usize;

/// The timer instance.
#[derive(Debug)]
pub struct Timer<T, N = SystemNow> {
    /// Shared state
    inner: Arc<Inner>,

    /// The number of milliseconds elapsed since the timer started.
    elapsed: u64,

    /// Timer wheel.
    ///
    /// Levels:
    ///
    /// * 1 ms slots / 64 ms range
    /// * 64 ms slots / ~ 4 sec range
    /// * ~ 4 sec slots / ~ 4 min range
    /// * ~ 4 min slots / ~ 4 hr range
    /// * ~ 4 hr slots / ~ 12 day range
    /// * ~ 12 day slots / ~ 2 yr range
    levels: Vec<Level>,

    /// Thread parker. The `Timer` park implementation delegates to this.
    park: T,

    /// Source of "now" instances
    now: N,
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
    entry: Option<Arc<Entry>>,
}

pub(crate) struct Inner {
    /// The instant at which the timer started running.
    start: Instant,

    /// The last published timer `elapsed` value.
    elapsed: AtomicU64,

    /// Number of active timeouts
    num: AtomicUsize,

    /// Head of the "process" linked list.
    process_head: AtomicPtr<Entry>,

    /// Unparks the timer thread.
    unpark: Box<Unpark>,
}

#[derive(Debug)]
struct Entry {
    /// Timer internals
    inner: Weak<Inner>,

    /// Task to notify once the deadline is reached.
    task: AtomicTask,

    /// Tracks the entry state
    state: AtomicUsize,

    /// Next entry in the "process" linked list.
    ///
    /// Represents a strong Arc ref.
    next_queue: UnsafeCell<*mut Entry>,

    /// `Sleep` deadline
    deadline: Instant,

    /// Next entry in the State's linked list.
    ///
    /// This is only accessed by the timer
    next_state: UnsafeCell<Option<Arc<Entry>>>,

    /// Previous entry in the State's linked list.
    ///
    /// This is only accessed by the timer and is used to unlink a canceled
    /// entry.
    ///
    /// This is a weak reference.
    prev_state: UnsafeCell<*const Entry>,
}

struct Level {
    level: usize,

    /// Tracks which slot entries are occupied.
    occupied: u64,

    /// Slots
    slot: [Option<Arc<Entry>>; LEVEL_MULT],
}

struct Expiration {
    level: usize,
    slot: usize,
    deadline: u64,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct State(usize);

/// Number of levels
const NUM_LEVELS: usize = 6;

/// Level multiplier.
///
/// Being a power of 2 is very important.
const LEVEL_MULT: usize = 64;

/// The maximum duration of a sleep
const MAX_DURATION: u64 = 1 << (6 * NUM_LEVELS);

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
    pub fn new_with_now(park: T, mut now: N) -> Self {
        let unpark = Box::new(park.unpark());

        let levels = (0..NUM_LEVELS)
            .map(Level::new)
            .collect();

        Timer {
            inner: Arc::new(Inner::new(now.now(), unpark)),
            elapsed: 0,
            levels,
            park,
            now,
        }
    }

    /// Returns a handle to the timer
    pub fn handle(&self) -> Handle {
        Handle::new(Arc::downgrade(&self.inner))
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
    fn next_expiration(&self) -> Option<Expiration> {
        // Check all levels
        for level in 0..NUM_LEVELS {
            let slot = match self.levels[level].next_occupied_slot(self.elapsed) {
                Some(slot) => slot,
                None => continue,
            };

            let level_start = self.elapsed - (self.elapsed % level_range(level));
            let deadline = level_start + slot as u64 * slot_range(level);

            return Some(Expiration {
                level,
                slot,
                deadline,
            });
        }

        None
    }

    /// Converts an `Expiration` to an `Instant`.
    fn expiration_instant(&self, expiration: &Expiration) -> Instant {
        self.inner.start + Duration::from_millis(expiration.deadline)
    }

    /// Run timer related logic
    fn process(&mut self) {
        let now = ms(self.now.now() - self.inner.start);

        loop {
            let expiration = match self.next_expiration() {
                Some(expiration) => expiration,
                None => break,
            };

            if expiration.deadline > now {
                // This expiration should not fire on this tick
                break;
            }

            // Prcess the slot, either moving it down a level or firing the
            // timeout if currently at the final (boss) level.
            self.process_expiration(&expiration);

            self.set_elapsed(expiration.deadline);
        }

        self.set_elapsed(now);
    }

    fn set_elapsed(&mut self, when: u64) {
        assert!(self.elapsed <= when);

        if when > self.elapsed {
            self.elapsed = when;
            self.inner.elapsed.store(when, Relaxed);
        } else {
            assert_eq!(self.elapsed, when);
        }
    }

    fn process_expiration(&mut self, expiration: &Expiration) {
        while let Some(entry) = self.pop_entry(expiration) {
            if expiration.level == 0 {
                entry.fire();
            } else {
                let when = ms(entry.deadline - self.inner.start);

                self.levels[expiration.level - 1]
                    .add_entry(entry, when);
            }
        }
    }

    fn pop_entry(&mut self, expiration: &Expiration) -> Option<Arc<Entry>> {
        self.levels[expiration.level].pop_entry_slot(expiration.slot)
    }

    /// Process the entry queue
    ///
    /// This handles adding and canceling timeouts.
    fn process_queue(&mut self) {
        let mut ptr = self.inner.process_head.swap(ptr::null_mut(), SeqCst);

        while !ptr.is_null() {
            let entry = unsafe { Arc::from_raw(ptr) };

            // Get the next entry
            ptr = unsafe { (*entry.next_queue.get()) };

            // Check the entry state
            if entry.is_elapsed() {
                self.clear_entry(entry);
            } else {
                self.add_entry(entry);
            }
        }
    }

    fn clear_entry(&mut self, entry: Arc<Entry>) {
        let when = self.normalize_deadline(&entry);

        if when <= self.elapsed {
            // The entry is no longer contained by the timer.
            return;
        }

        // Get the level at which the entry should be stored
        let level = level_for(when - self.elapsed);

        self.levels[level].remove_entry(&entry, when);
    }

    fn add_entry(&mut self, entry: Arc<Entry>) {
        // Avoid an underflow subtraction
        if entry.deadline < self.inner.start {
            entry.fire();
            return;
        }

        // Convert the duration to millis
        let when = self.normalize_deadline(&entry);

        if when > MAX_DURATION {
            entry.error();
            return;
        }

        // If the entry's deadline is in the past or present, trigger it.
        if when <= self.elapsed {
            entry.fire();
            return;
        }

        // Get the level at which the entry should be stored
        let level = level_for(when - self.elapsed);

        self.levels[level].add_entry(entry, when);
    }

    fn normalize_deadline(&self, entry: &Entry) -> u64 {
        // Convert the duration to millis
        ms(entry.deadline - self.inner.start)
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
        self.process_queue();

        match self.next_expiration() {
            Some(expiration) => {
                let now = self.now.now();
                let deadline = self.expiration_instant(&expiration);

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
        self.process_queue();

        match self.next_expiration() {
            Some(expiration) => {
                let now = self.now.now();
                let deadline = self.expiration_instant(&expiration);

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
            ptr = unsafe { (*entry.next_queue.get()) };

            // The entry must be flagged as errored
            entry.error();
        }
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
        let inner = match handle.inner() {
            Some(inner) => inner,
            None => return Err(Error::shutdown()),
        };

        if deadline <= inner.now() {
            // The deadline has already elapsed, ther eis no point creating the
            // structures.
            return Ok(Registration {
                entry: None
            });
        }

        // Increment the number of active timeouts
        inner.increment()?;

        let entry = Arc::new(Entry {
            inner: handle.into_inner(),
            task: AtomicTask::new(),
            state: AtomicUsize::new(0),
            next_queue: UnsafeCell::new(ptr::null_mut()),
            deadline: deadline,
            next_state: UnsafeCell::new(None),
            prev_state: UnsafeCell::new(ptr::null_mut()),
        });

        inner.queue(&entry)?;

        Ok(Registration {
            entry: Some(entry),
        })
    }

    pub fn is_elapsed(&self) -> bool {
        self.entry.as_ref()
            .map(|e| e.is_elapsed())
            .unwrap_or(true)
    }

    pub fn poll_elapsed(&self) -> Poll<(), Error> {
        self.entry.as_ref()
            .map(|e| e.poll_elapsed())
            .unwrap_or(Ok(().into()))
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let entry = match self.entry {
            Some(ref e) => e,
            None => return,
        };

        let state: State = entry.state.fetch_or(ELAPSED, SeqCst).into();

        if state.is_elapsed() {
            // Nothing more to do
            return;
        }

        let inner = match entry.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        let _ = inner.queue(entry);
    }
}

// ===== impl Inner =====

impl Inner {
    fn new(start: Instant, unpark: Box<Unpark>) -> Inner {
        Inner {
            num: AtomicUsize::new(0),
            elapsed: AtomicU64::new(0),
            process_head: AtomicPtr::new(ptr::null_mut()),
            start,
            unpark,
        }
    }

    fn now(&self) -> Instant {
        self.start + Duration::from_millis(self.elapsed.load(Relaxed))
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
                *(entry.next_queue.get()) = curr;
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

// ===== impl Level =====

impl Level {
    fn new(level: usize) -> Level {
        Level {
            level,
            occupied: 0,
            slot: [
                // It does not look like the necessary traits are
                // derived for [T; 64].
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
                None, None, None, None, None, None, None, None,
            ],
        }
    }

    fn next_occupied_slot(&self, now: u64) -> Option<usize> {
        if self.occupied == 0 {
            return None;
        }

        // Get the slot for now using Maths
        let now_slot = (now / slot_range(self.level)) as usize;
        let occupied = self.occupied.rotate_right(now_slot as u32);
        let zeros = occupied.trailing_zeros() as usize;
        let slot = (zeros + now_slot) % 64;

        Some(slot)
    }

    fn add_entry(&mut self, entry: Arc<Entry>, when: u64) {
        let slot = slot_for(when, self.level);

        push_entry(&mut self.slot[slot], entry);
        self.occupied |= occupied_bit(slot);
    }

    fn remove_entry(&mut self, entry: &Entry, when: u64) {
        let slot = slot_for(when, self.level);

        remove_entry(&mut self.slot[slot], entry);

        if self.slot[slot].is_none() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            // Unset the bit
            self.occupied ^= occupied_bit(slot);
        }
    }

    fn pop_entry_slot(&mut self, slot: usize) -> Option<Arc<Entry>> {
        let ret = pop_entry(&mut self.slot[slot]);

        if ret.is_some() && self.slot[slot].is_none() {
            // The bit is currently set
            debug_assert!(self.occupied & occupied_bit(slot) != 0);

            self.occupied ^= occupied_bit(slot);
        }

        ret
    }
}

fn occupied_bit(slot: usize) -> u64 {
    (1 << slot)
}

fn slot_range(level: usize) -> u64 {
    LEVEL_MULT.pow(level as u32) as u64
}

fn level_range(level: usize) -> u64 {
    LEVEL_MULT as u64 * slot_range(level)
}

/// Push an entry to the head of the linked list
fn push_entry(head: &mut Option<Arc<Entry>>, entry: Arc<Entry>) {
    // Get a pointer to the entry to for the prev link
    let ptr = &*entry as *const _;

    // Remove the old head entry
    let old = head.take();

    unsafe {
        if let Some(ref entry) = old.as_ref() {
            // Set the previous link on the old head
            *entry.prev_state.get() = ptr;
        }

        // Set this entry's next pointer
        *entry.next_state.get() = old;

    }

    // Update the head pointer
    *head = Some(entry);
}

/// Pop the head of the linked list
fn pop_entry(head: &mut Option<Arc<Entry>>) -> Option<Arc<Entry>> {
    let entry = head.take();

    unsafe {
        if let Some(entry) = entry.as_ref() {
            *head = (*entry.next_state.get()).take();

            if let Some(entry) = head.as_ref() {
                *entry.prev_state.get() = ptr::null();
            }

            *entry.prev_state.get() = ptr::null();
        }
    }

    entry
}

/// Remove the entry from the linked list
///
/// The caller must ensure that the entry actually is contained by the list.
fn remove_entry(head: &mut Option<Arc<Entry>>, entry: &Entry) {
    unsafe {
        // Unlink `entry` from the next node
        let next = (*entry.next_state.get()).take();

        if let Some(next) = next.as_ref() {
            (*next.prev_state.get()) = *entry.prev_state.get();
        }

        // Unlink `entry` from the prev node

        if let Some(prev) = (*entry.prev_state.get()).as_ref() {
            *prev.next_state.get() = next;
        } else {
            // It is the head
            *head = next;
        }

        // Unset the prev pointer
        *entry.prev_state.get() = ptr::null();
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        while let Some(slot) = self.next_occupied_slot(0) {
            // This should always have one
            let entry = self.pop_entry_slot(slot)
                .expect("occupied bit set invalid");

            entry.error();
        }
    }
}

impl fmt::Debug for Level {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Level")
            .field("occupied", &self.occupied)
            .finish()
    }
}

// ===== impl Entry =====

impl Entry {
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

impl Drop for Entry {
    fn drop(&mut self) {
        let inner = match self.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };

        inner.decrement();
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


/// Convert a `Duration` to milliseconds, rounding up and saturating at
/// `u64::MAX`.
///
/// The saturating is fine because `u64::MAX` milliseconds are still many
/// million years.
fn ms(duration: Duration) -> u64 {
    const NANOS_PER_MILLI: u32 = 1_000_000;
    const MILLIS_PER_SEC: u64 = 1_000;

    // Round up.
    let millis = (duration.subsec_nanos() + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI;
    duration.as_secs().saturating_mul(MILLIS_PER_SEC).saturating_add(millis as u64)
}

/// Convert a duration (milliseconds) to a level
fn level_for(duration: u64) -> usize {
    debug_assert!(duration > 0);

    // 63 is picked to offset this by 1. A duration of 0 is prohibited, so this
    // cannot underflow.
    let significant = 63 - duration.leading_zeros() as usize;
    significant / 6
}

/// Convert a duration (milliseconds) and a level to a slot position
fn slot_for(duration: u64, level: usize) -> usize {
    ((duration >> (level * 6)) % LEVEL_MULT as u64) as usize
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_level_and_slot_for() {
        for pos in 1..64 {
            assert_eq!(0, level_for(pos), "level_for({}) -- binary = {:b}", pos, pos);
            assert_eq!(pos as usize, slot_for(pos, 0));
        }

        for level in 1..5 {
            for pos in level..64 {
                let a = pos * 64_usize.pow(level as u32);
                assert_eq!(level, level_for(a as u64),
                           "level_for({}) -- binary = {:b}", a, a);

                assert_eq!(pos as usize, slot_for(a as u64, level));

                if pos > level {
                    let a = a - 1;
                    assert_eq!(level, level_for(a as u64),
                               "level_for({}) -- binary = {:b}", a, a);
                }

                if pos < 64 {
                    let a = a + 1;
                    assert_eq!(level, level_for(a as u64),
                               "level_for({}) -- binary = {:b}", a, a);
                }
            }
        }
    }
}
