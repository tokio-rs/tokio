// Allow `unreachable_pub` warnings when sync is not enabled
// due to the usage of `Notify` within the `rt-core` feature set.
// When this module is compiled with `sync` enabled we will warn on
// this lint. When `rt-core` is enabled we use `pub(crate)` which
// triggers this warning but it is safe to ignore in this case.
#![cfg_attr(not(feature = "sync"), allow(unreachable_pub, dead_code))]

use crate::loom::sync::atomic::AtomicU8;
use crate::loom::sync::Mutex;
use crate::util::linked_list::{self, LinkedList};

use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll, Waker};

type WaitList = LinkedList<Waiter, <Waiter as linked_list::Link>::Target>;

/// Notify a single task to wake up.
///
/// `Notify` provides a basic mechanism to notify a single task of an event.
/// `Notify` itself does not carry any data. Instead, it is to be used to signal
/// another task to perform an operation.
///
/// `Notify` can be thought of as a [`Semaphore`] starting with 0 permits.
/// [`notified().await`] waits for a permit to become available, and [`notify_one()`]
/// sets a permit **if there currently are no available permits**.
///
/// The synchronization details of `Notify` are similar to
/// [`thread::park`][park] and [`Thread::unpark`][unpark] from std. A [`Notify`]
/// value contains a single permit. [`notified().await`] waits for the permit to
/// be made available, consumes the permit, and resumes.  [`notify_one()`] sets the
/// permit, waking a pending task if there is one.
///
/// If `notify_one()` is called **before** `notified().await`, then the next call to
/// `notified().await` will complete immediately, consuming the permit. Any
/// subsequent calls to `notified().await` will wait for a new permit.
///
/// If `notify_one()` is called **multiple** times before `notified().await`, only a
/// **single** permit is stored. The next call to `notified().await` will
/// complete immediately, but the one after will wait for a new permit.
///
/// # Examples
///
/// Basic usage.
///
/// ```
/// use tokio::sync::Notify;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let notify = Arc::new(Notify::new());
///     let notify2 = notify.clone();
///
///     tokio::spawn(async move {
///         notify2.notified().await;
///         println!("received notification");
///     });
///
///     println!("sending notification");
///     notify.notify_one();
/// }
/// ```
///
/// Unbound mpsc channel.
///
/// ```
/// use tokio::sync::Notify;
///
/// use std::collections::VecDeque;
/// use std::sync::Mutex;
///
/// struct Channel<T> {
///     values: Mutex<VecDeque<T>>,
///     notify: Notify,
/// }
///
/// impl<T> Channel<T> {
///     pub fn send(&self, value: T) {
///         self.values.lock().unwrap()
///             .push_back(value);
///
///         // Notify the consumer a value is available
///         self.notify.notify_one();
///     }
///
///     pub async fn recv(&self) -> T {
///         loop {
///             // Drain values
///             if let Some(value) = self.values.lock().unwrap().pop_front() {
///                 return value;
///             }
///
///             // Wait for values to be available
///             self.notify.notified().await;
///         }
///     }
/// }
/// ```
///
/// [park]: std::thread::park
/// [unpark]: std::thread::Thread::unpark
/// [`notified().await`]: Notify::notified()
/// [`notify_one()`]: Notify::notify_one()
/// [`Semaphore`]: crate::sync::Semaphore
#[derive(Debug)]
pub struct Notify {
    state: AtomicU8,
    waiters: Mutex<WaitList>,
}

#[derive(Debug, Clone, Copy)]
enum NotificationType {
    // Notification triggered by calling `notify_waiters`
    AllWaiters,
    // Notification triggered by calling `notify_one`
    OneWaiter,
}

#[derive(Debug)]
struct Waiter {
    /// Intrusive linked-list pointers
    pointers: linked_list::Pointers<Waiter>,

    /// Waiting task's waker
    waker: Option<Waker>,

    /// `true` if the notification has been assigned to this waiter.
    notified: Option<NotificationType>,

    /// Should not be `Unpin`.
    _p: PhantomPinned,
}

/// Future returned from `notified()`
#[derive(Debug)]
pub struct Notified<'a> {
    /// The `Notify` being received on.
    notify: &'a Notify,

    /// The current state of the receiving process.
    state: State,

    /// Entry in the waiter `LinkedList`.
    waiter: UnsafeCell<Waiter>,
}

unsafe impl<'a> Send for Notified<'a> {}
unsafe impl<'a> Sync for Notified<'a> {}

#[derive(Debug)]
enum State {
    Init,
    Waiting,
    Done,
}

/// Initial "idle" state
const EMPTY: u8 = 0;

/// One or more threads are currently waiting to be notified.
const WAITING: u8 = 1;

/// Pending notification
const NOTIFIED: u8 = 2;

impl Notify {
    /// Create a new `Notify`, initialized without a permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Notify;
    ///
    /// let notify = Notify::new();
    /// ```
    pub fn new() -> Notify {
        Notify {
            state: AtomicU8::new(0),
            waiters: Mutex::new(LinkedList::new()),
        }
    }

    /// Create a new `Notify`, initialized without a permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Notify;
    ///
    /// static NOTIFY: Notify = Notify::const_new();
    /// ```
    #[cfg(all(feature = "parking_lot", not(all(loom, test))))]
    #[cfg_attr(docsrs, doc(cfg(feature = "parking_lot")))]
    pub const fn const_new() -> Notify {
        Notify {
            state: AtomicU8::new(0),
            waiters: Mutex::const_new(LinkedList::new()),
        }
    }

    /// Wait for a notification.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn notified(&self);
    /// ```
    ///
    /// Each `Notify` value holds a single permit. If a permit is available from
    /// an earlier call to [`notify_one()`], then `notified().await` will complete
    /// immediately, consuming that permit. Otherwise, `notified().await` waits
    /// for a permit to be made available by the next call to `notify_one()`.
    ///
    /// [`notify_one()`]: Notify::notify_one
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Notify;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let notify = Arc::new(Notify::new());
    ///     let notify2 = notify.clone();
    ///
    ///     tokio::spawn(async move {
    ///         notify2.notified().await;
    ///         println!("received notification");
    ///     });
    ///
    ///     println!("sending notification");
    ///     notify.notify_one();
    /// }
    /// ```
    pub fn notified(&self) -> Notified<'_> {
        Notified {
            notify: self,
            state: State::Init,
            waiter: UnsafeCell::new(Waiter {
                pointers: linked_list::Pointers::new(),
                waker: None,
                notified: None,
                _p: PhantomPinned,
            }),
        }
    }

    /// Notifies a waiting task
    ///
    /// If a task is currently waiting, that task is notified. Otherwise, a
    /// permit is stored in this `Notify` value and the **next** call to
    /// [`notified().await`] will complete immediately consuming the permit made
    /// available by this call to `notify_one()`.
    ///
    /// At most one permit may be stored by `Notify`. Many sequential calls to
    /// `notify_one` will result in a single permit being stored. The next call to
    /// `notified().await` will complete immediately, but the one after that
    /// will wait.
    ///
    /// [`notified().await`]: Notify::notified()
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::Notify;
    /// use std::sync::Arc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let notify = Arc::new(Notify::new());
    ///     let notify2 = notify.clone();
    ///
    ///     tokio::spawn(async move {
    ///         notify2.notified().await;
    ///         println!("received notification");
    ///     });
    ///
    ///     println!("sending notification");
    ///     notify.notify_one();
    /// }
    /// ```
    pub fn notify_one(&self) {
        // Load the current state
        let mut curr = self.state.load(SeqCst);

        // If the state is `EMPTY`, transition to `NOTIFIED` and return.
        while let EMPTY | NOTIFIED = curr {
            // The compare-exchange from `NOTIFIED` -> `NOTIFIED` is intended. A
            // happens-before synchronization must happen between this atomic
            // operation and a task calling `notified().await`.
            let res = self.state.compare_exchange(curr, NOTIFIED, SeqCst, SeqCst);

            match res {
                // No waiters, no further work to do
                Ok(_) => return,
                Err(actual) => {
                    curr = actual;
                }
            }
        }

        // There are waiters, the lock must be acquired to notify.
        let mut waiters = self.waiters.lock();

        // The state must be reloaded while the lock is held. The state may only
        // transition out of WAITING while the lock is held.
        curr = self.state.load(SeqCst);

        if let Some(waker) = notify_locked(&mut waiters, &self.state, curr) {
            drop(waiters);
            waker.wake();
        }
    }

    /// Notifies all waiting tasks
    pub(crate) fn notify_waiters(&self) {
        // There are waiters, the lock must be acquired to notify.
        let mut waiters = self.waiters.lock();

        // The state must be reloaded while the lock is held. The state may only
        // transition out of WAITING while the lock is held.
        let curr = self.state.load(SeqCst);

        if let EMPTY | NOTIFIED = curr {
            // There are no waiting tasks. In this case, no synchronization is
            // established between `notify` and `notified().await`.
            return;
        }

        // At this point, it is guaranteed that the state will not
        // concurrently change, as holding the lock is required to
        // transition **out** of `WAITING`.
        //
        // Get pending waiters
        while let Some(mut waiter) = waiters.pop_back() {
            // Safety: `waiters` lock is still held.
            let waiter = unsafe { waiter.as_mut() };

            assert!(waiter.notified.is_none());

            waiter.notified = Some(NotificationType::AllWaiters);

            if let Some(waker) = waiter.waker.take() {
                waker.wake();
            }
        }

        // All waiters have been notified, the state must be transitioned to
        // `EMPTY`. As transitioning **from** `WAITING` requires the lock to be
        // held, a `store` is sufficient.
        self.state.store(EMPTY, SeqCst);
    }
}

impl Default for Notify {
    fn default() -> Notify {
        Notify::new()
    }
}

fn notify_locked(waiters: &mut WaitList, state: &AtomicU8, curr: u8) -> Option<Waker> {
    loop {
        match curr {
            EMPTY | NOTIFIED => {
                let res = state.compare_exchange(curr, NOTIFIED, SeqCst, SeqCst);

                match res {
                    Ok(_) => return None,
                    Err(actual) => {
                        assert!(actual == EMPTY || actual == NOTIFIED);
                        state.store(NOTIFIED, SeqCst);
                        return None;
                    }
                }
            }
            WAITING => {
                // At this point, it is guaranteed that the state will not
                // concurrently change as holding the lock is required to
                // transition **out** of `WAITING`.
                //
                // Get a pending waiter
                let mut waiter = waiters.pop_back().unwrap();

                // Safety: `waiters` lock is still held.
                let waiter = unsafe { waiter.as_mut() };

                assert!(waiter.notified.is_none());

                waiter.notified = Some(NotificationType::OneWaiter);
                let waker = waiter.waker.take();

                if waiters.is_empty() {
                    // As this the **final** waiter in the list, the state
                    // must be transitioned to `EMPTY`. As transitioning
                    // **from** `WAITING` requires the lock to be held, a
                    // `store` is sufficient.
                    state.store(EMPTY, SeqCst);
                }

                return waker;
            }
            _ => unreachable!(),
        }
    }
}

// ===== impl Notified =====

impl Notified<'_> {
    /// A custom `project` implementation is used in place of `pin-project-lite`
    /// as a custom drop implementation is needed.
    fn project(self: Pin<&mut Self>) -> (&Notify, &mut State, &UnsafeCell<Waiter>) {
        unsafe {
            // Safety: both `notify` and `state` are `Unpin`.

            is_unpin::<&Notify>();
            is_unpin::<AtomicU8>();

            let me = self.get_unchecked_mut();
            (&me.notify, &mut me.state, &me.waiter)
        }
    }
}

impl Future for Notified<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        use State::*;

        let (notify, state, waiter) = self.project();

        loop {
            match *state {
                Init => {
                    // Optimistically try acquiring a pending notification
                    let res = notify
                        .state
                        .compare_exchange(NOTIFIED, EMPTY, SeqCst, SeqCst);

                    if res.is_ok() {
                        // Acquired the notification
                        *state = Done;
                        return Poll::Ready(());
                    }

                    // Acquire the lock and attempt to transition to the waiting
                    // state.
                    let mut waiters = notify.waiters.lock();

                    // Reload the state with the lock held
                    let mut curr = notify.state.load(SeqCst);

                    // Transition the state to WAITING.
                    loop {
                        match curr {
                            EMPTY => {
                                // Transition to WAITING
                                let res = notify
                                    .state
                                    .compare_exchange(EMPTY, WAITING, SeqCst, SeqCst);

                                if let Err(actual) = res {
                                    assert_eq!(actual, NOTIFIED);
                                    curr = actual;
                                } else {
                                    break;
                                }
                            }
                            WAITING => break,
                            NOTIFIED => {
                                // Try consuming the notification
                                let res = notify
                                    .state
                                    .compare_exchange(NOTIFIED, EMPTY, SeqCst, SeqCst);

                                match res {
                                    Ok(_) => {
                                        // Acquired the notification
                                        *state = Done;
                                        return Poll::Ready(());
                                    }
                                    Err(actual) => {
                                        assert_eq!(actual, EMPTY);
                                        curr = actual;
                                    }
                                }
                            }
                            _ => unreachable!(),
                        }
                    }

                    // Safety: called while locked.
                    unsafe {
                        (*waiter.get()).waker = Some(cx.waker().clone());
                    }

                    // Insert the waiter into the linked list
                    //
                    // safety: pointers from `UnsafeCell` are never null.
                    waiters.push_front(unsafe { NonNull::new_unchecked(waiter.get()) });

                    *state = Waiting;

                    return Poll::Pending;
                }
                Waiting => {
                    // Currently in the "Waiting" state, implying the caller has
                    // a waiter stored in the waiter list (guarded by
                    // `notify.waiters`). In order to access the waker fields,
                    // we must hold the lock.

                    let waiters = notify.waiters.lock();

                    // Safety: called while locked
                    let w = unsafe { &mut *waiter.get() };

                    if w.notified.is_some() {
                        // Our waker has been notified. Reset the fields and
                        // remove it from the list.
                        w.waker = None;
                        w.notified = None;

                        *state = Done;
                    } else {
                        // Update the waker, if necessary.
                        if !w.waker.as_ref().unwrap().will_wake(cx.waker()) {
                            w.waker = Some(cx.waker().clone());
                        }

                        return Poll::Pending;
                    }

                    // Explicit drop of the lock to indicate the scope that the
                    // lock is held. Because holding the lock is required to
                    // ensure safe access to fields not held within the lock, it
                    // is helpful to visualize the scope of the critical
                    // section.
                    drop(waiters);
                }
                Done => {
                    return Poll::Ready(());
                }
            }
        }
    }
}

impl Drop for Notified<'_> {
    fn drop(&mut self) {
        use State::*;

        // Safety: The type only transitions to a "Waiting" state when pinned.
        let (notify, state, waiter) = unsafe { Pin::new_unchecked(self).project() };

        // This is where we ensure safety. The `Notified` value is being
        // dropped, which means we must ensure that the waiter entry is no
        // longer stored in the linked list.
        if let Waiting = *state {
            let mut notify_state = WAITING;
            let mut waiters = notify.waiters.lock();

            // `Notify.state` may be in any of the three states (Empty, Waiting,
            // Notified). It doesn't actually matter what the atomic is set to
            // at this point. We hold the lock and will ensure the atomic is in
            // the correct state once the lock is dropped.
            //
            // Because the atomic state is not checked, at first glance, it may
            // seem like this routine does not handle the case where the
            // receiver is notified but has not yet observed the notification.
            // If this happens, no matter how many notifications happen between
            // this receiver being notified and the receive future dropping, all
            // we need to do is ensure that one notification is returned back to
            // the `Notify`. This is done by calling `notify_locked` if `self`
            // has the `notified` flag set.

            // remove the entry from the list
            //
            // safety: the waiter is only added to `waiters` by virtue of it
            // being the only `LinkedList` available to the type.
            unsafe { waiters.remove(NonNull::new_unchecked(waiter.get())) };

            if waiters.is_empty() {
                notify_state = EMPTY;
                // If the state *should* be `NOTIFIED`, the call to
                // `notify_locked` below will end up doing the
                // `store(NOTIFIED)`. If a concurrent receiver races and
                // observes the incorrect `EMPTY` state, it will then obtain the
                // lock and block until `notify.state` is in the correct final
                // state.
                notify.state.store(EMPTY, SeqCst);
            }

            // See if the node was notified but not received. In this case, if
            // the notification was triggered via `notify_one`, it must be sent
            // to the next waiter.
            //
            // Safety: with the entry removed from the linked list, there can be
            // no concurrent access to the entry
            if let Some(NotificationType::OneWaiter) = unsafe { (*waiter.get()).notified } {
                if let Some(waker) = notify_locked(&mut waiters, &notify.state, notify_state) {
                    drop(waiters);
                    waker.wake();
                }
            }
        }
    }
}

/// # Safety
///
/// `Waiter` is forced to be !Unpin.
unsafe impl linked_list::Link for Waiter {
    type Handle = NonNull<Waiter>;
    type Target = Waiter;

    fn as_raw(handle: &NonNull<Waiter>) -> NonNull<Waiter> {
        *handle
    }

    unsafe fn from_raw(ptr: NonNull<Waiter>) -> NonNull<Waiter> {
        ptr
    }

    unsafe fn pointers(mut target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
        NonNull::from(&mut target.as_mut().pointers)
    }
}

fn is_unpin<T: Unpin>() {}
