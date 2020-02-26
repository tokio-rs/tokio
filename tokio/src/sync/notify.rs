use crate::loom::sync::atomic::AtomicU8;
use crate::loom::sync::Mutex;
use crate::util::linked_list::{self, LinkedList};

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll, Waker};

/// Notify a single task to wake up.
///
/// `Notify` provides a basic mechanism to notify a single task of an event.
/// `Notify` itself does not carry any data. Instead, it is to be used to signal
/// another task to perform an operation.
///
/// `Notify` can be thought of as a [`Semaphore`] starting with 0 permits.
/// [`Notify::recv`] waits for a permit to become available and
/// [`Notify::notify_one`] sets a permit **if there currently are no available
/// permits**.
///
/// The synchronization details of `Notify` are similar to
/// [`thread::park`][park] and [`Thread::unpark`][unpark] from std. A [`Notify`]
/// value contains a single permit. [`Notify::recv`] waits for the permit to be
/// made available, consumes the permit, and resumes. [`Notify::notify_one`]
/// sets the permit, waking a pending task if there is one.
///
/// If `notify_one` is called **before** `recv()`, then the next call to
/// `recv()` will complete immediately, consuming the permit. Any subsequent
/// calls to `recv()` will wait for a new permit.
///
/// If `notify_one` is called **multiple** times before `recv()`, only a
/// **single** permit is stored. The next call to `recv()` will complete
/// immediately, but the one after will wait for a new permit.
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
///         notify2.recv().await;
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
///             self.notify.recv().await;
///         }
///     }
/// }
/// ```
///
/// [park]: std::thread::park
/// [unpark]: std::thread::Thread::unpark
/// [`Notify::recv`]: Notify::recv()
/// [`Notify::notify_one`]: Notify::notify_one()
/// [`Semaphore`]: crate::sync::Semaphore
#[derive(Debug)]
pub struct Notify {
    state: AtomicU8,
    waiters: Mutex<LinkedList<Waiter>>,
}

#[derive(Debug)]
struct Waiter {
    /// Waiting task's waker
    waker: Option<Waker>,

    /// `true` if the notification has been assigned to this waiter.
    notified: bool,
}

#[derive(Debug)]
struct RecvFuture<'a> {
    /// The `Notify` being received on.
    notify: &'a Notify,

    /// The current state of the receiving process.
    state: RecvState,

    /// Entry in the waiter `LinkedList`.
    waiter: linked_list::Entry<Waiter>,
}

#[derive(Debug)]
enum RecvState {
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

    /// Wait for a notification.
    ///
    /// Each `Notify` value holds a single permit. If a permit is available from
    /// an earlier call to `notify_one`, then `recv` will complete immediately,
    /// consuming that permit. Otherwise, `recv()` waits for a permit to be made
    /// available by the next call to `notify_one`
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
    ///         notify2.recv().await;
    ///         println!("received a notification");
    ///     });
    ///
    ///     notify.notify_one();
    /// }
    /// ```
    pub async fn recv(&self) {
        RecvFuture {
            notify: self,
            state: RecvState::Init,
            waiter: linked_list::Entry::new(Waiter {
                waker: None,
                notified: false,
            }),
        }
        .await
    }

    /// Notifies a waiting task
    ///
    /// If a task is currently waiting, that task is notified. Otherwise, a
    /// permit is stored in this `Notify` value and the **next** call to
    /// `recv()` will complete immediately consuming the permit made available
    /// by this call to `notify_one()`.
    ///
    /// At most one permit may be stored by `Notify`. Many sequential calls to
    /// `notify_one` will result in a single permit being stored. The next call
    /// to `recv()` will complete immediately, but the one after that will wait.
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
    ///         notify2.recv().await;
    ///         println!("received a notification");
    ///     });
    ///
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
            // operation and a task calling `recv()`.
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
        let mut waiters = self.waiters.lock().unwrap();

        // The state must be reloaded while the lock is held. The state may only
        // transition out of WAITING while the lock is held.
        curr = self.state.load(SeqCst);

        if let Some(waker) = notify_locked(&mut waiters, &self.state, curr) {
            drop(waiters);
            waker.wake();
        }
    }
}

impl Default for Notify {
    fn default() -> Notify {
        Notify::new()
    }
}

fn notify_locked(waiters: &mut LinkedList<Waiter>, state: &AtomicU8, curr: u8) -> Option<Waker> {
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

                assert!(!waiter.notified);

                waiter.notified = true;
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

// ===== impl RecvFuture =====

impl RecvFuture<'_> {
    fn project(
        self: Pin<&mut Self>,
    ) -> (
        &Notify,
        &mut RecvState,
        Pin<&mut linked_list::Entry<Waiter>>,
    ) {
        unsafe {
            // Safety: both `notify` and `state` are `Unpin`.

            is_unpin::<&Notify>();
            is_unpin::<AtomicU8>();

            let me = self.get_unchecked_mut();
            (
                &me.notify,
                &mut me.state,
                Pin::new_unchecked(&mut me.waiter),
            )
        }
    }
}

impl Future for RecvFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        use RecvState::*;

        let (notify, state, mut waiter) = self.project();

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
                    let mut waiters = notify.waiters.lock().unwrap();

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
                        (*waiter.as_mut().get()).waker = Some(cx.waker().clone());

                        // Insert the waiter into the linked list
                        waiters.push_front(waiter.as_mut());
                    }

                    *state = Waiting;
                }
                Waiting => {
                    // Currently in the "Waiting" state, implying the caller has
                    // a waiter stored in the waiter list (guarded by
                    // `notify.waiters`). In order to access the waker fields,
                    // we must hold the lock.

                    let waiters = notify.waiters.lock().unwrap();

                    // Safety: called while locked
                    let w = unsafe { &mut *waiter.as_mut().get() };

                    if w.notified {
                        // Our waker has been notified. Reset the fields and
                        // remove it from the list.
                        w.waker = None;
                        w.notified = false;

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

impl Drop for RecvFuture<'_> {
    fn drop(&mut self) {
        use RecvState::*;

        // Safety: The type only transitions to a "Waiting" state when pinned.
        let (notify, state, mut waiter) = unsafe { Pin::new_unchecked(self).project() };

        // This is where we ensure safety. The `RecvFuture` is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        if let Waiting = *state {
            let mut notify_state = WAITING;
            let mut waiters = notify.waiters.lock().unwrap();

            // `Notify.state` may be in any of the three states (Empty, Waiting,
            // Notified). It doesn't actually matter what the atomic is set to
            // at this point. We hold the lock and will ensure the atomic is in
            // the correct state once th elock is dropped.
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
            unsafe { waiters.remove(waiter.as_mut()) };

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

            // See if the node was notified but not received. In this case, the
            // notification must be sent to another waiter.
            //
            // Safety: with the entry removed from the linked list, there can be
            // no concurrent access to the entry
            let notified = unsafe { (*waiter.as_mut().get()).notified };

            if notified {
                if let Some(waker) = notify_locked(&mut waiters, &notify.state, notify_state) {
                    drop(waiters);
                    waker.wake();
                }
            }
        }
    }
}

fn is_unpin<T: Unpin>() {}
