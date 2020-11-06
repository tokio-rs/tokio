//! Timer state structures.
//!
//! This module contains the heart of the intrusive timer implementation, and as
//! such the structures inside are full of tricky concurrency and unsafe code.
//!
//! # Ground rules
//!
//! The heart of the timer implementation here is the `TimerShared` structure,
//! shared between the `TimerEntry` and the driver. Generally, we permit access
//! to `TimerShared` ONLY via either 1) a mutable reference to `TimerEntry` or
//! 2) a held driver lock.
//!
//! It follows from this that any changes made while holding BOTH 1 and 2 will
//! be reliably visible, regardless of ordering. This is because of the acq/rel
//! fences on the driver lock ensuring ordering with 2, and rust mutable
//! reference rules for 1 (a mutable reference to an object can't be passed
//! between threads without an acq/rel barrier, and same-thread we have local
//! happens-before ordering).
//!
//! # State transitions
//!
//! Each timer has a state associated with it. During polls, this state is
//! observed using a relaxed read to rapidly check whether the timer has
//! expired. In most cases, we expect timer completion to be signalled without
//! requiring explicit synchronization.
//!
//! The detailed synchronization rules are documented under [`EntryState`].
//!
//! # Cached vs true timeouts
//!
//! To allow for the use case of a timeout that is periodically reset before
//! expiration to be as lightweight as possible, we support optimistically
//! lock-free timer resets, in the case where a timer is rescheduled to a later
//! point than it was originally scheduled for.
//!
//! This is accomplished by lazily rescheduling timers. That is, we update a
//! field indicating the true expiration of the timer from the holder of the
//! [`TimerEntry`]. When the driver services timers (ie, whenever it's walking
//! lists of timers), it checks this "true when" value, and reschedules based on
//! it.
//!
//! We do, however, also need to track what the expiration time was when we
//! originally registered the timer; this is used to locate the right linked
//! list when the timer is being cancelled. This is referred to as the "cached
//! when" internally.
//!
//! There is of course a race condition between timer reset and timer
//! expiration. If the driver fails to observe the updated expiration time, it
//! could trigger expiration of the timer too early. We deal with this on poll
//! by checking that the cached time and true time match (indicating expiration
//! happened at the last-set timeout); if not, we re-register the timer under
//! its true expiration time and suppress the timer expiration.

use crate::loom::sync::atomic::{AtomicU64, Ordering};
use crate::sync::AtomicWaker;
use crate::time::Instant;
use crate::util::linked_list;

use super::{InternalHandle, TimeSource};

use std::cell::UnsafeCell as StdUnsafeCell;
use std::task::{Context, Poll, Waker};
use std::{marker::PhantomPinned, pin::Pin, ptr::NonNull};

/// The state of a timer. This indicates both the current "owner" of the timer
/// (the driver or the [`TimerEntry`]) and, if completed, how it completed.
///
/// Generally, a holder of the driver lock can manipulate the [`TimerShared`]
/// structure only if either the state is Registered or FiringInProgress, or if
/// it also holds an exclusive reference to the [`TimerEntry`].
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(super) enum EntryState {
    /// Indicates that this EntryState is for a timer which is unexpired but not
    /// yet registered with the driver. This most commonly is the case for
    /// timers that have not yet been polled.
    NotRegistered,
    /// This EntryState is unexpired and registered in the driver. Transitioning
    /// into this state requires holding the driver lock AND an exclusive
    /// reference to the Entry.
    ///
    /// Transitioning out happens via FiringInProgress, as a release-ordered
    /// write, followed by a transition to a completed state.
    Registered,
    /// This EntryState is currently being fired. In order to determine the
    /// final state of the timer, polls need to synchronize with the driver by
    /// acquiring the driver lock.
    ///
    /// This state can represent one of two conditions: Either the driver is
    /// working on firing this timer _right now_ (and we'll leave the state
    /// before unlocking), or it's on the pending_firing queue in the timer
    /// wheel. In the former case, after writing this state, the driver will
    /// take the waker (as an acq/rel barrier) and arrange to invoke it later,
    /// before moving on to the true completed state.
    ///
    /// There are two reasons this state exists. First, because we cannot take
    /// the waker after transitioning to a completed state (the [`TimerEntry`]
    /// could at any time observe this and destroy the [`TimerShared`]); however
    /// we also can't take the waker immediately before transitioning to a
    /// completed state, as the future might be in the process of being polled
    /// and registering a new waker.
    ///
    /// To resolve this, we add this state as an intermediate phase where the
    /// driver can take the waker. If the future is polled at this point, it
    /// will contend on the driver lock; once it acquires and releases the
    /// driver lock, it can reliably observe the final completed timer state.
    ///
    /// The second reason this exists is that, in general, when we want to
    /// advance the 'elapsed' time, we might need to process a slot first; when
    /// doing so, we need to _atomically_ move all of the timers in that slot
    /// into their new positions, before dropping the lock. However, during this
    /// time some timers might also need to be fired. If there are too many
    /// timers awaiting firing to collect their wakers while holding the lock,
    /// we queue them onto a separate linked list. This state indicates that we
    /// need to remove the timer from that pending list instead of a timer wheel
    /// slot.
    FiringInProgress,
    /// This timer has been fired due to its timeout being reached. This state
    /// can be transitioned back to NotRegistered, either by an explicit reset()
    /// call, or by implicitly detecting a race due to cached and true timeouts
    /// not matching.
    Fired,
    /// This timer has been cancelled. This state exists only on timers in the
    /// process of being dropped.
    Cancelled,
    /// An error has occurred. This state cannot be transitioned out of.
    Error(crate::time::error::Kind),
}

impl EntryState {
    fn is_elapsed(self) -> bool {
        !matches!(
            self,
            EntryState::NotRegistered | EntryState::Registered | EntryState::FiringInProgress
        )
    }
}

impl EntryState {
    fn to_repr(self) -> u8 {
        unsafe { std::mem::transmute(self) }
    }

    unsafe fn from_repr(n: u8) -> Self {
        unsafe { std::mem::transmute(n) }
    }
}

#[cfg(not(loom))]
mod state_cell {
    use super::EntryState;
    use std::sync::atomic::{AtomicU8, Ordering};

    /// We use an AtomicU8 to represent our state, but the moment we write a
    /// completed state to it, it might be destroyed by another thread - despite
    /// a reference still existing for a brief moment. This is actually a
    /// problem on stable rust right now, and ideally we'd want atomic functions
    /// on an `*const AtomicU8` to help resolve this, but since `Arc` and
    /// friends rely on this more-or-less working right now we're probably okay
    /// in the short term.
    ///
    /// However, on loom, there's additional state on the AtomicU8, and so we
    /// can't have it be destroyed under us. On loom - only - we wrap this in an
    /// Arc to help resolve this issue.
    pub(super) struct StateCell(AtomicU8);
    impl StateCell {
        pub(super) fn new() -> Self {
            Self(AtomicU8::new(EntryState::NotRegistered.to_repr()))
        }

        /// Erases lifetime information from this StateCell. The resulting
        /// `StateRef` should be considered to be a kind of raw pointer.
        pub(super) fn erase(&self) -> StateRef {
            StateRef(&self.0)
        }

        /// Gets the current state.
        pub(super) fn get(&self, ordering: Ordering) -> EntryState {
            unsafe { self.erase().get(ordering) }
        }

        /// Sets the current state.
        pub(super) fn set(&self, s: EntryState, ordering: Ordering) {
            unsafe { self.erase().set(s, ordering) }
        }
    }

    pub(super) struct StateRef(*const AtomicU8);

    impl StateRef {
        /// Gets the current state.
        ///
        /// SAFETY: The underlying [`EntryState`] must still exist.
        pub(super) unsafe fn get(&self, ordering: Ordering) -> EntryState {
            EntryState::from_repr(unsafe { (*self.0).load(ordering) })
        }

        /// Sets the current state.
        ///
        /// SAFETY: The underlying [`EntryState`] must still exist.
        pub(super) unsafe fn set(&self, s: EntryState, ordering: Ordering) {
            unsafe { (*self.0).store(s.to_repr(), ordering) }
        }
    }
}

#[cfg(loom)]
mod state_cell {
    use super::EntryState;
    use crate::loom::sync::atomic::{AtomicU8, Ordering};

    /// Because loom needs extra tracking around when we start and stop using an UnsafeCell,
    /// we need to make sure the UnsafeCell isn't destroyed before we finish up our bookkeeping.
    ///
    /// As such, when running under loom, we set up an Arc to keep the cell alive.
    /// (We don't use a loom Arc because this is not part of the production model)
    pub(super) struct StateCell(std::sync::Arc<AtomicU8>);
    impl StateCell {
        pub(super) fn new() -> Self {
            Self(std::sync::Arc::new(AtomicU8::new(
                EntryState::NotRegistered.to_repr(),
            )))
        }

        pub(super) fn erase(&self) -> StateRef {
            StateRef(self.0.clone())
        }

        pub(super) fn get(&self, ordering: Ordering) -> EntryState {
            unsafe { EntryState::from_repr(self.0.load(ordering)) }
        }

        pub(super) fn set(&self, s: EntryState, ordering: Ordering) {
            self.0.store(s.to_repr(), ordering)
        }
    }

    pub(super) struct StateRef(std::sync::Arc<AtomicU8>);

    impl StateRef {
        pub(super) fn get(&self, ordering: Ordering) -> EntryState {
            unsafe { EntryState::from_repr(self.0.load(ordering)) }
        }

        pub(super) fn set(&self, s: EntryState, ordering: Ordering) {
            self.0.store(s.to_repr(), ordering)
        }
    }
}

use state_cell::StateCell;

impl std::fmt::Debug for StateCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StateCell({:?})", self.get(Ordering::SeqCst))
    }
}

/// A timer entry.
///
/// This is the handle to a timer that is controlled by the requester of the
/// timer. As this participates in intrusive data structures, it must be pinned
/// before polling.
#[derive(Debug)]
pub(super) struct TimerEntry<TS: TimeSource> {
    /// Arc reference to the driver. We can only free the driver after
    /// deregistering everything from their respective timer wheels.
    driver: InternalHandle<TS>,
    /// Shared inner structure; this is part of an intrusive linked list, and
    /// therefore other references can exist to it while mutable references to
    /// Entry exist.
    ///
    /// This is manipulated only under the inner mutex. TODO: Can we use loom
    /// cells for this?
    inner: StdUnsafeCell<TimerShared>,
}

unsafe impl<TS: TimeSource> Send for TimerEntry<TS> {}
unsafe impl<TS: TimeSource> Sync for TimerEntry<TS> {}

/// An EntryHandle is the (non-enforced) "unique" pointer from the driver to the
/// timer entry. Generally, at most one EntryHandle exists for a timer at a time
/// (enforced by the timer state machine).
///
/// SAFETY: An EntryHandle is essentially a raw pointer, and the usual caveats
/// of pointer safety apply. In particular, EntryHandle does not itself enforce
/// that the timer does still exist; however, normally an EntryHandle is created
/// immediately before registering the timer, and is consumed when firing the
/// timer, to help minimize mistakes. Still, because EntryHandle cannot enforce
/// memory safety, all operations are unsafe.
#[derive(Debug)]
pub(crate) struct TimerHandle {
    inner: NonNull<TimerShared>,
}

pub(super) type EntryList = crate::util::linked_list::LinkedList<TimerShared, TimerShared>;

/// The shared state structure of a timer. This structure is shared between the
/// frontend (`Entry`) and driver backend.
///
/// Note that this structure is located inside the `TimerEntry` structure.
#[derive(Debug)]
pub(crate) struct TimerShared {
    /// Current state. This records whether the timer entry is currently under
    /// the ownership of the driver, and if not, its current state (not
    /// complete, fired, error, etc).
    ///
    /// This value is accessed atomically from multiple threads and so some care
    /// is needed to use this properly. The general rules are:
    ///
    /// 1. If the state is not Registered or FiringInProgress, the driver does
    ///    not own the timer and is not allowed to touch it at all. As such, the
    ///    timer future (Entry) is free to manipulate the timer freely.
    /// 2. Entering or leaving Registered or FiringInProgress requires the
    ///    driver lock. Entering Registered further requires exclusive ownership
    ///    of the timer future.
    /// 3. The state of a timer can only be FiringInProgress while the driver
    ///    lock is held by the driver itself.
    ///
    /// It follows that cancellation of a timer requires acquiring the driver
    /// lock if the state is Registered or FiringInProgress.
    state: state_cell::StateCell,

    /// AtomicWaker for notifying the owning thread. May be updated without
    /// holding locks.
    ///
    /// There is a risk of a race between the driver updating the timer state
    /// and the waker being fired. To resolve this, we only guarantee that the
    /// waker will be fired if the state can be observed to be Registered
    /// _after__ setting the waker. In particular, if FiringInProgress is
    /// observed, we do not guarantee that the waker will be invoked (but it may
    /// be invoked).
    waker: AtomicWaker,

    /// Cache-padded contents (data mostly manipulated by the IO driver). This
    /// is separated to avoid contention between threads polling the timer and
    /// the driver thread itself.
    padded: CachePadded<TimerSharedPadded>,

    _p: PhantomPinned,
}

impl TimerShared {
    /// Gets the cached time-of-expiration value with relaxed memory order
    pub(super) fn cached_when(&self) -> u64 {
        self.padded.0.cached_when.load(Ordering::Relaxed)
    }

    /// Gets the true time-of-expiration value, and copies it into the cached
    /// time-of-expiration value.
    ///
    /// SAFETY: Must be called with the driver lock held, and when this entry is
    /// not in any timer wheel lists.
    pub(super) unsafe fn sync_when(&self) -> u64 {
        let true_when = self.true_when();

        self.padded
            .0
            .cached_when
            .store(true_when, Ordering::Relaxed);

        true_when
    }

    /// Returns the true time-of-expiration value, with relaxed memory ordering.
    pub(super) fn true_when(&self) -> u64 {
        self.padded.0.true_when.load(Ordering::Relaxed)
    }

    /// Sets the true time-of-expiration value, with relaxed memory ordering.
    /// Returns the old value.
    pub(super) fn set_when(&self, t: u64) -> u64 {
        let old_when = self.padded.0.true_when.load(Ordering::Relaxed);
        self.padded.0.true_when.store(t, Ordering::Relaxed);

        old_when
    }

    /// Returns an EntryHandle for this timer.
    pub(super) fn handle(&self) -> TimerHandle {
        TimerHandle {
            inner: NonNull::new(self as *const _ as *mut _).unwrap(),
        }
    }

    /// Returns true if the state of this timer indicates that the timer might
    /// be registered with the driver. This check is performed with relaxed
    /// ordering, but is conservative - if it returns false, the timer is
    /// definitely _not_ registered.
    pub(super) fn is_registered(&self) -> bool {
        let state = self.state.get(Ordering::Relaxed);

        matches!(state, EntryState::Registered | EntryState::FiringInProgress)
    }

    /// Returns true if this entry is in the FiringInProgress state
    pub(super) fn is_pending(&self) -> bool {
        unsafe { self.state.get(Ordering::Relaxed) == EntryState::FiringInProgress }
    }
}

/// Additional shared state between the driver and the timer which is cache
/// padded. This contains the information that the driver thread accesses most
/// frequently to minimize contention. In particular, we move it away from the
/// waker, as the waker is updated on every poll.
struct TimerSharedPadded {
    /// The expiration time for which this entry is currently registered.
    /// Generally owned by the driver, but is accessed by the entry when not
    /// registered.
    cached_when: AtomicU64,

    /// The true expiration time. Set by the timer future, read by the driver.
    true_when: AtomicU64,

    /// A link within the doubly-linked list of timers on a particular level and
    /// slot. Valid only if state is equal to Registered.
    ///
    /// Only accessed under the entry lock.
    pointers: StdUnsafeCell<linked_list::Pointers<TimerShared>>,
}

impl std::fmt::Debug for TimerSharedPadded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryInner")
            .field("when", &self.true_when.load(Ordering::Relaxed))
            .finish()
    }
}

impl TimerSharedPadded {
    fn new(when: u64) -> CachePadded<Self> {
        CachePadded(TimerSharedPadded {
            cached_when: AtomicU64::new(when),
            true_when: AtomicU64::new(when),
            pointers: StdUnsafeCell::new(linked_list::Pointers::new()),
        })
    }
}

unsafe impl Send for TimerShared {}
unsafe impl Sync for TimerShared {}

unsafe impl linked_list::Link for TimerShared {
    type Handle = TimerHandle;

    type Target = TimerShared;

    fn as_raw(handle: &Self::Handle) -> NonNull<Self::Target> {
        handle.inner
    }

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle {
        TimerHandle { inner: ptr }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<Self::Target>> {
        unsafe { NonNull::new(target.as_ref().padded.0.pointers.get()).unwrap() }
    }
}

// ===== impl Entry =====

impl<TS: TimeSource> TimerEntry<TS> {
    pub(crate) fn new(handle: &InternalHandle<TS>, deadline: Instant) -> TimerEntry<TS> {
        let deadline = handle.time_source().deadline_to_tick(deadline);
        let driver = handle.clone();

        Self {
            driver,
            inner: StdUnsafeCell::new(TimerShared {
                waker: AtomicWaker::new(),
                state: state_cell::StateCell::new(),
                padded: TimerSharedPadded::new(deadline),
                _p: PhantomPinned,
            }),
        }
    }

    fn inner(&self) -> &TimerShared {
        unsafe { &*self.inner.get() }
    }

    /// Gets an EntryHandle to this Entry
    pub(super) fn handle(&mut self) -> TimerHandle {
        TimerHandle {
            inner: NonNull::new(self.inner.get()).unwrap(),
        }
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        self.inner().state.get(Ordering::Relaxed).is_elapsed()
    }

    /// Cancels and deregisters the timer. This operation is irreversible.
    pub(crate) fn cancel(self: Pin<&mut Self>) {
        // We need to perform an acq/rel fence with the driver thread, and the
        // simplest way to do so is to grab the driver lock.
        //
        // Why is this necessary? We're about to release this timer's memory for
        // some other non-timer use. However, we've been doing a bunch of
        // relaxed (or even non-atomic) writes from the driver thread, and we'll
        // be doing more from _this thread_ (as this memory is interpreted as
        // something else).
        //
        // It is critical to ensure that, from the point of view of the driver,
        // those future non-timer writes happen-after the timer is fully fired,
        // and from the purpose of this thread, the driver's writes all
        // happen-before we drop the timer. This in turn requires us to perform
        // an acquire-release barrier in _both_ directions between the driver
        // and dropping thread.
        //
        // This lock acquisition serves this purpose. All of the driver
        // manipulations happen with the lock held, so we can just take the lock
        // and be sure that this drop happens-after everything the driver did so
        // far and happens-before everything the driver does in the future.
        // While we have the lock held, we also go ahead and deregister the
        // entry if necessary.
        unsafe {
            self.driver
                .clear_entry(NonNull::new(self.inner() as *const _ as *mut _).unwrap())
        };
    }

    pub(crate) fn reset(self: Pin<&mut Self>, new_time: Instant) {
        let tick = self.driver.time_source().deadline_to_tick(new_time);
        let old_time = self.inner().set_when(tick);

        let need_reregister = match self.inner().state.get(Ordering::Relaxed) {
            // Need to cancel and re-register if this is being rescheduled to
            // earlier than before, as we can't guarantee when the driver will
            // notice the reschedule.
            EntryState::Registered => tick < old_time,
            // Not registered in the first place
            EntryState::FiringInProgress | EntryState::Fired | EntryState::NotRegistered => true,
            EntryState::Cancelled => panic!("Trying to reset a cancelled timer"),
            EntryState::Error(_) => false,
        };

        if need_reregister {
            unsafe {
                self.driver.reregister(self.inner().into());
            }
        }
    }

    pub(crate) fn poll_elapsed(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), super::Error>> {
        let this = unsafe { self.get_unchecked_mut() };

        // Most of the time we just can do a quick check of our state value without taking the lock.
        loop {
            // Implicit acquire fence. This fence synchronizes with the
            // take_waker in fire(). This means that, if the waker will be
            // ignored, the FiringInProgress write will be a visible side-effect
            // in this thread after this point. It also means that any writes to
            // cached_when performed before firing the timer _also_ become
            // visible side-effects in this thread.
            //
            // Note that this is only, in general, guaranteed to be an acquire
            // fence, as it can in some cases decide to simply invoke its own
            // waker without performing a store. If we do write a waker here, it
            // is an acq-rel fence.
            this.inner().waker.register_by_ref(cx.waker());

            let state = this.inner().state.get(Ordering::Relaxed);
            match state {
                EntryState::NotRegistered => {
                    unsafe {
                        let handle = this.handle();
                        this.driver.add_entry(handle);
                    }

                    // Re-poll afterward - the driver will communicate
                    // synchronous completion or error states by updating our
                    // state. If there was no such error, we'll go into the
                    // Registered branch below to set the waker.
                }
                EntryState::Registered => {
                    // We registered the waker before observing ourselves in the
                    // Registered state. This means that, if there is a
                    // concurrent call to fire() happening, the update to the
                    // waker happens-before the take of the waker in the fire()
                    // call, and so we can be sure that we will be awoken.
                    //
                    // Thus, it is safe to return Pending here.
                    break Poll::Pending;
                }
                EntryState::FiringInProgress => {
                    // Synchronize with the driver, once we finish the true
                    // completed state should be visible.
                    unsafe {
                        this.driver
                            .sync_timer(NonNull::new(this.inner.get()).unwrap())
                    };

                    // We should now be in a completed state, so loop back
                    // around to figure out what we want to return.
                    debug_assert!(this.inner().state.get(Ordering::Relaxed).is_elapsed());
                }
                EntryState::Fired => {
                    // We're probably done, but if we were reset, it's possible we
                    // raced with being fired by the driver. If this happened, we
                    // reset our state and try again.

                    // To check, it's sufficient to see if cached_when and
                    // true_when are in sync. cached_when will always reflect
                    // the last value written by the driver. This is because the
                    // waker register_by_ref's implicit fence (above) ensures
                    // that any cached_when writes are visible side-effects
                    // here.
                    if unsafe { this.inner().cached_when() } == this.inner().true_when() {
                        break Ok(()).into();
                    } else {
                        // We're unsure of the state of this timer, so
                        // re-register it, and loop again to figure out what's
                        // up.
                        unsafe {
                            this.driver
                                .reregister(NonNull::new(this.inner.get()).unwrap())
                        }
                    }
                }
                EntryState::Cancelled => break Ok(()).into(),
                EntryState::Error(e) => break Err(e.into()).into(),
            }
        }
    }
}

impl TimerHandle {
    pub(super) unsafe fn cached_when(&self) -> u64 {
        unsafe { self.inner.as_ref().cached_when() }
    }

    pub(super) unsafe fn sync_when(&self) -> u64 {
        unsafe { self.inner.as_ref().sync_when() }
    }

    pub(super) unsafe fn is_pre_registration(&self) -> bool {
        let state = unsafe { self.inner.as_ref().state.get(Ordering::Relaxed) };

        state == EntryState::NotRegistered
    }

    /// Tries to set this entry to registered state.
    ///
    /// Returns true if successful, or false if already in an error state.
    ///
    /// SAFETY: The caller must ensure that the handle remains valid and that
    /// the state transition rules are not violated.

    pub(super) unsafe fn set_registered(&self) -> bool {
        unsafe {
            match self.inner.as_ref().state.get(Ordering::Relaxed) {
                EntryState::Error(_) => return false,
                EntryState::Cancelled => panic!("Trying to register a cancelled timer"),
                EntryState::FiringInProgress => panic!("Unexpected FiringInProgress state"),
                EntryState::Fired | EntryState::Registered => (), // ok - reregistration
                EntryState::NotRegistered => (),
            };

            self.inner
                .as_ref()
                .state
                .set(EntryState::Registered, Ordering::Relaxed);

            true
        }
    }

    /// Transitions to the FiringInProgress state
    pub(super) unsafe fn set_pending(&self) {
        unsafe {
            assert_eq!(
                self.inner.as_ref().state.get(Ordering::Relaxed),
                EntryState::Registered
            );
            self.inner
                .as_ref()
                .state
                .set(EntryState::FiringInProgress, Ordering::Relaxed);
        }
    }

    /// Attempts to transition to a terminal state. If the state is already a
    /// terminal state, does nothing.
    ///
    /// Because the entry might be dropped after the state is moved to a
    /// terminal state, this function consumes the handle to ensure we don't
    /// access the entry afterwards.
    ///
    /// Returns the last-registered waker, if any.
    ///
    /// SAFETY: The driver lock must be held while invoking this function.
    pub(super) unsafe fn fire(self, completed_state: EntryState) -> Option<Waker> {
        debug_assert!(completed_state.is_elapsed());

        // Get a lifetime-erased pointer to the state. We'll use this to set the
        // elapsed state below.
        let pstate = unsafe { self.inner.as_ref() }.state.erase();

        // If we're already in a terminal state, do nothing. This typically
        // happens when trying to cancel a timer that's already fired.
        //
        // Ordering: State is changed only under the driver lock.
        match pstate.get(Ordering::Relaxed) {
            EntryState::FiringInProgress | EntryState::NotRegistered | EntryState::Registered => { /* continue */
            }
            EntryState::Fired | EntryState::Cancelled | EntryState::Error(_) => {
                return None;
            }
        };

        // Set the state. This will become visible along with the waker take
        // operation below.
        pstate.set(EntryState::FiringInProgress, Ordering::Relaxed);

        // Acq/rel barrier.
        //
        // We rely on the waker update/take acq/rel fences to ensure that the
        // final state write is not visible until after we have read any
        // possible waker change (ie, any thread that might have changed the
        // waker in between the state change and our taking the waker will
        // definitely see the state change _after_ updating the waker).
        let waker: Option<Waker> = unsafe { self.inner.as_ref().waker.take_waker() };

        // We do not need a release barrier here; it need only be visible by the
        // time our later call to the waker causes the task to be awoken
        // (implied by wake).
        pstate.set(completed_state, Ordering::Relaxed);

        waker
    }
}

impl<TS: TimeSource> Drop for TimerEntry<TS> {
    fn drop(&mut self) {
        unsafe { Pin::new_unchecked(self) }.as_mut().cancel()
    }
}

#[cfg_attr(target_arch = "x86_64", repr(align(128)))]
#[cfg_attr(not(target_arch = "x86_64"), repr(align(64)))]
#[derive(Debug)]
struct CachePadded<T>(T);
