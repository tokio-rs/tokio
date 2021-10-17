// Allow `unreachable_pub` warnings when sync is not enabled
// due to the usage of `Notify` within the `rt` feature set.
// When this module is compiled with `sync` enabled we will warn on
// this lint. When `rt` is enabled we use `pub(crate)` which
// triggers this warning but it is safe to ignore in this case.
#![cfg_attr(not(feature = "sync"), allow(unreachable_pub, dead_code))]

use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Mutex;
use crate::util::linked_list::{self, LinkedList};
use crate::util::WakeList;

use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll, Waker};

type WaitList = LinkedList<Waiter, <Waiter as linked_list::Link>::Target>;

/// Notifies a single task to wake up.
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
    // This uses 2 bits to store one of `EMPTY`,
    // `WAITING` or `NOTIFIED`. The rest of the bits
    // are used to store the number of times `notify_waiters`
    // was called.
    state: AtomicUsize,
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
    /// Intrusive linked-list pointers.
    pointers: linked_list::Pointers<Waiter>,

    /// Waiting task's waker.
    waker: Option<Waker>,

    /// `true` if the notification has been assigned to this waiter.
    notified: Option<NotificationType>,

    /// Should not be `Unpin`.
    _p: PhantomPinned,
}

/// Future returned from [`Notify::notified()`]
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
    Init(usize),
    Waiting,
    Done,
}

const NOTIFY_WAITERS_SHIFT: usize = 2;
const STATE_MASK: usize = (1 << NOTIFY_WAITERS_SHIFT) - 1;
const NOTIFY_WAITERS_CALLS_MASK: usize = !STATE_MASK;

/// Initial "idle" state.
const EMPTY: usize = 0;

/// One or more threads are currently waiting to be notified.
const WAITING: usize = 1;

/// Pending notification.
const NOTIFIED: usize = 2;

fn set_state(data: usize, state: usize) -> usize {
    (data & NOTIFY_WAITERS_CALLS_MASK) | (state & STATE_MASK)
}

fn get_state(data: usize) -> usize {
    data & STATE_MASK
}

fn get_num_notify_waiters_calls(data: usize) -> usize {
    (data & NOTIFY_WAITERS_CALLS_MASK) >> NOTIFY_WAITERS_SHIFT
}

fn inc_num_notify_waiters_calls(data: usize) -> usize {
    data + (1 << NOTIFY_WAITERS_SHIFT)
}

fn atomic_inc_num_notify_waiters_calls(data: &AtomicUsize) {
    data.fetch_add(1 << NOTIFY_WAITERS_SHIFT, SeqCst);
}

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
            state: AtomicUsize::new(0),
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
            state: AtomicUsize::new(0),
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
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute notifications in the order
    /// they were requested. Cancelling a call to `notified` makes you lose your
    /// place in the queue.
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
        // we load the number of times notify_waiters
        // was called and store that in our initial state
        let state = self.state.load(SeqCst);
        Notified {
            notify: self,
            state: State::Init(state >> NOTIFY_WAITERS_SHIFT),
            waiter: UnsafeCell::new(Waiter {
                pointers: linked_list::Pointers::new(),
                waker: None,
                notified: None,
                _p: PhantomPinned,
            }),
        }
    }

    /// Notifies a waiting task.
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
    // Alias for old name in 0.x
    #[cfg_attr(docsrs, doc(alias = "notify"))]
    pub fn notify_one(&self) {
        // Load the current state
        let mut curr = self.state.load(SeqCst);

        // If the state is `EMPTY`, transition to `NOTIFIED` and return.
        while let EMPTY | NOTIFIED = get_state(curr) {
            // The compare-exchange from `NOTIFIED` -> `NOTIFIED` is intended. A
            // happens-before synchronization must happen between this atomic
            // operation and a task calling `notified().await`.
            let new = set_state(curr, NOTIFIED);
            let res = self.state.compare_exchange(curr, new, SeqCst, SeqCst);

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

    /// Notifies all waiting tasks.
    ///
    /// If a task is currently waiting, that task is notified. Unlike with
    /// `notify_one()`, no permit is stored to be used by the next call to
    /// `notified().await`. The purpose of this method is to notify all
    /// already registered waiters. Registering for notification is done by
    /// acquiring an instance of the `Notified` future via calling `notified()`.
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
    ///     let notified1 = notify.notified();
    ///     let notified2 = notify.notified();
    ///
    ///     let handle = tokio::spawn(async move {
    ///         println!("sending notifications");
    ///         notify2.notify_waiters();
    ///     });
    ///
    ///     notified1.await;
    ///     notified2.await;
    ///     println!("received notifications");
    /// }
    /// ```
    pub fn notify_waiters(&self) {
        let mut wakers = WakeList::new();

        // There are waiters, the lock must be acquired to notify.
        let mut waiters = self.waiters.lock();

        // The state must be reloaded while the lock is held. The state may only
        // transition out of WAITING while the lock is held.
        let curr = self.state.load(SeqCst);

        if let EMPTY | NOTIFIED = get_state(curr) {
            // There are no waiting tasks. All we need to do is increment the
            // number of times this method was called.
            atomic_inc_num_notify_waiters_calls(&self.state);
            return;
        }

        // At this point, it is guaranteed that the state will not
        // concurrently change, as holding the lock is required to
        // transition **out** of `WAITING`.
        'outer: loop {
            while wakers.can_push() {
                match waiters.pop_back() {
                    Some(mut waiter) => {
                        // Safety: `waiters` lock is still held.
                        let waiter = unsafe { waiter.as_mut() };

                        assert!(waiter.notified.is_none());

                        waiter.notified = Some(NotificationType::AllWaiters);

                        if let Some(waker) = waiter.waker.take() {
                            wakers.push(waker);
                        }
                    }
                    None => {
                        break 'outer;
                    }
                }
            }

            drop(waiters);

            wakers.wake_all();

            // Acquire the lock again.
            waiters = self.waiters.lock();
        }

        // All waiters will be notified, the state must be transitioned to
        // `EMPTY`. As transitioning **from** `WAITING` requires the lock to be
        // held, a `store` is sufficient.
        let new = set_state(inc_num_notify_waiters_calls(curr), EMPTY);
        self.state.store(new, SeqCst);

        // Release the lock before notifying
        drop(waiters);

        wakers.wake_all();
    }
}

impl Default for Notify {
    fn default() -> Notify {
        Notify::new()
    }
}

fn notify_locked(waiters: &mut WaitList, state: &AtomicUsize, curr: usize) -> Option<Waker> {
    loop {
        match get_state(curr) {
            EMPTY | NOTIFIED => {
                let res = state.compare_exchange(curr, set_state(curr, NOTIFIED), SeqCst, SeqCst);

                match res {
                    Ok(_) => return None,
                    Err(actual) => {
                        let actual_state = get_state(actual);
                        assert!(actual_state == EMPTY || actual_state == NOTIFIED);
                        state.store(set_state(actual, NOTIFIED), SeqCst);
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
                    state.store(set_state(curr, EMPTY), SeqCst);
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
            is_unpin::<AtomicUsize>();

            let me = self.get_unchecked_mut();
            (me.notify, &mut me.state, &me.waiter)
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
                Init(initial_notify_waiters_calls) => {
                    let curr = notify.state.load(SeqCst);

                    // Optimistically try acquiring a pending notification
                    let res = notify.state.compare_exchange(
                        set_state(curr, NOTIFIED),
                        set_state(curr, EMPTY),
                        SeqCst,
                        SeqCst,
                    );

                    if res.is_ok() {
                        // Acquired the notification
                        *state = Done;
                        return Poll::Ready(());
                    }

                    // Clone the waker before locking, a waker clone can be
                    // triggering arbitrary code.
                    let waker = cx.waker().clone();

                    // Acquire the lock and attempt to transition to the waiting
                    // state.
                    let mut waiters = notify.waiters.lock();

                    // Reload the state with the lock held
                    let mut curr = notify.state.load(SeqCst);

                    // if notify_waiters has been called after the future
                    // was created, then we are done
                    if get_num_notify_waiters_calls(curr) != initial_notify_waiters_calls {
                        *state = Done;
                        return Poll::Ready(());
                    }

                    // Transition the state to WAITING.
                    loop {
                        match get_state(curr) {
                            EMPTY => {
                                // Transition to WAITING
                                let res = notify.state.compare_exchange(
                                    set_state(curr, EMPTY),
                                    set_state(curr, WAITING),
                                    SeqCst,
                                    SeqCst,
                                );

                                if let Err(actual) = res {
                                    assert_eq!(get_state(actual), NOTIFIED);
                                    curr = actual;
                                } else {
                                    break;
                                }
                            }
                            WAITING => break,
                            NOTIFIED => {
                                // Try consuming the notification
                                let res = notify.state.compare_exchange(
                                    set_state(curr, NOTIFIED),
                                    set_state(curr, EMPTY),
                                    SeqCst,
                                    SeqCst,
                                );

                                match res {
                                    Ok(_) => {
                                        // Acquired the notification
                                        *state = Done;
                                        return Poll::Ready(());
                                    }
                                    Err(actual) => {
                                        assert_eq!(get_state(actual), EMPTY);
                                        curr = actual;
                                    }
                                }
                            }
                            _ => unreachable!(),
                        }
                    }

                    // Safety: called while locked.
                    unsafe {
                        (*waiter.get()).waker = Some(waker);
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
            let mut waiters = notify.waiters.lock();
            let mut notify_state = notify.state.load(SeqCst);

            // remove the entry from the list (if not already removed)
            //
            // safety: the waiter is only added to `waiters` by virtue of it
            // being the only `LinkedList` available to the type.
            unsafe { waiters.remove(NonNull::new_unchecked(waiter.get())) };

            if waiters.is_empty() {
                if let WAITING = get_state(notify_state) {
                    notify_state = set_state(notify_state, EMPTY);
                    notify.state.store(notify_state, SeqCst);
                }
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
