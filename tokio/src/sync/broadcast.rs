//! A multi-producer, multi-consumer broadcast queue. Each sent value is seen by
//! all consumers.
//!
//! A [`Sender`] is used to broadcast values to **all** connected [`Receiver`]
//! values. [`Sender`] handles are clone-able, allowing concurrent send and
//! receive actions. [`Sender`] and [`Receiver`] are both `Send` and `Sync` as
//! long as `T` is `Send`.
//!
//! When a value is sent, **all** [`Receiver`] handles are notified and will
//! receive the value. The value is stored once inside the channel and cloned on
//! demand for each receiver. Once all receivers have received a clone of the
//! value, the value is released from the channel.
//!
//! A channel is created by calling [`channel`], specifying the maximum number
//! of messages the channel can retain at any given time.
//!
//! New [`Receiver`] handles are created by calling [`Sender::subscribe`]. The
//! returned [`Receiver`] will receive values sent **after** the call to
//! `subscribe`.
//!
//! This channel is also suitable for the single-producer multi-consumer
//! use-case, where a single sender broadcasts values to many receivers.
//!
//! ## Lagging
//!
//! As sent messages must be retained until **all** [`Receiver`] handles receive
//! a clone, broadcast channels are susceptible to the "slow receiver" problem.
//! In this case, all but one receiver are able to receive values at the rate
//! they are sent. Because one receiver is stalled, the channel starts to fill
//! up.
//!
//! This broadcast channel implementation handles this case by setting a hard
//! upper bound on the number of values the channel may retain at any given
//! time. This upper bound is passed to the [`channel`] function as an argument.
//!
//! If a value is sent when the channel is at capacity, the oldest value
//! currently held by the channel is released. This frees up space for the new
//! value. Any receiver that has not yet seen the released value will return
//! [`RecvError::Lagged`] the next time [`recv`] is called.
//!
//! Once [`RecvError::Lagged`] is returned, the lagging receiver's position is
//! updated to the oldest value contained by the channel. The next call to
//! [`recv`] will return this value.
//!
//! This behavior enables a receiver to detect when it has lagged so far behind
//! that data has been dropped. The caller may decide how to respond to this:
//! either by aborting its task or by tolerating lost messages and resuming
//! consumption of the channel.
//!
//! ## Closing
//!
//! When **all** [`Sender`] handles have been dropped, no new values may be
//! sent. At this point, the channel is "closed". Once a receiver has received
//! all values retained by the channel, the next call to [`recv`] will return
//! with [`RecvError::Closed`].
//!
//! When a [`Receiver`] handle is dropped, any messages not read by the receiver
//! will be marked as read. If this receiver was the only one not to have read
//! that message, the message will be dropped at this point.
//!
//! [`Sender`]: crate::sync::broadcast::Sender
//! [`Sender::subscribe`]: crate::sync::broadcast::Sender::subscribe
//! [`Receiver`]: crate::sync::broadcast::Receiver
//! [`channel`]: crate::sync::broadcast::channel
//! [`RecvError::Lagged`]: crate::sync::broadcast::error::RecvError::Lagged
//! [`RecvError::Closed`]: crate::sync::broadcast::error::RecvError::Closed
//! [`recv`]: crate::sync::broadcast::Receiver::recv
//!
//! # Examples
//!
//! Basic usage
//!
//! ```
//! use tokio::sync::broadcast;
//!
//! #[tokio::main]
//! async fn main() {
//!     let (tx, mut rx1) = broadcast::channel(16);
//!     let mut rx2 = tx.subscribe();
//!
//!     tokio::spawn(async move {
//!         assert_eq!(rx1.recv().await.unwrap(), 10);
//!         assert_eq!(rx1.recv().await.unwrap(), 20);
//!     });
//!
//!     tokio::spawn(async move {
//!         assert_eq!(rx2.recv().await.unwrap(), 10);
//!         assert_eq!(rx2.recv().await.unwrap(), 20);
//!     });
//!
//!     tx.send(10).unwrap();
//!     tx.send(20).unwrap();
//! }
//! ```
//!
//! Handling lag
//!
//! ```
//! use tokio::sync::broadcast;
//!
//! #[tokio::main]
//! async fn main() {
//!     let (tx, mut rx) = broadcast::channel(2);
//!
//!     tx.send(10).unwrap();
//!     tx.send(20).unwrap();
//!     tx.send(30).unwrap();
//!
//!     // The receiver lagged behind
//!     assert!(rx.recv().await.is_err());
//!
//!     // At this point, we can abort or continue with lost messages
//!
//!     assert_eq!(20, rx.recv().await.unwrap());
//!     assert_eq!(30, rx.recv().await.unwrap());
//! }
//! ```

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};
use crate::util::linked_list::{self, LinkedList};
use crate::util::WakeList;

use std::fmt;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::SeqCst;
use std::task::{Context, Poll, Waker};
use std::usize;

/// Sending-half of the [`broadcast`] channel.
///
/// May be used from many threads. Messages can be sent with
/// [`send`][Sender::send].
///
/// # Examples
///
/// ```
/// use tokio::sync::broadcast;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, mut rx1) = broadcast::channel(16);
///     let mut rx2 = tx.subscribe();
///
///     tokio::spawn(async move {
///         assert_eq!(rx1.recv().await.unwrap(), 10);
///         assert_eq!(rx1.recv().await.unwrap(), 20);
///     });
///
///     tokio::spawn(async move {
///         assert_eq!(rx2.recv().await.unwrap(), 10);
///         assert_eq!(rx2.recv().await.unwrap(), 20);
///     });
///
///     tx.send(10).unwrap();
///     tx.send(20).unwrap();
/// }
/// ```
///
/// [`broadcast`]: crate::sync::broadcast
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// Receiving-half of the [`broadcast`] channel.
///
/// Must not be used concurrently. Messages may be retrieved using
/// [`recv`][Receiver::recv].
///
/// To turn this receiver into a `Stream`, you can use the [`BroadcastStream`]
/// wrapper.
///
/// [`BroadcastStream`]: https://docs.rs/tokio-stream/0.1/tokio_stream/wrappers/struct.BroadcastStream.html
///
/// # Examples
///
/// ```
/// use tokio::sync::broadcast;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, mut rx1) = broadcast::channel(16);
///     let mut rx2 = tx.subscribe();
///
///     tokio::spawn(async move {
///         assert_eq!(rx1.recv().await.unwrap(), 10);
///         assert_eq!(rx1.recv().await.unwrap(), 20);
///     });
///
///     tokio::spawn(async move {
///         assert_eq!(rx2.recv().await.unwrap(), 10);
///         assert_eq!(rx2.recv().await.unwrap(), 20);
///     });
///
///     tx.send(10).unwrap();
///     tx.send(20).unwrap();
/// }
/// ```
///
/// [`broadcast`]: crate::sync::broadcast
pub struct Receiver<T> {
    /// State shared with all receivers and senders.
    shared: Arc<Shared<T>>,

    /// Next position to read from
    next: u64,
}

pub mod error {
    //! Broadcast error types

    use std::fmt;

    /// Error returned by from the [`send`] function on a [`Sender`].
    ///
    /// A **send** operation can only fail if there are no active receivers,
    /// implying that the message could never be received. The error contains the
    /// message being sent as a payload so it can be recovered.
    ///
    /// [`send`]: crate::sync::broadcast::Sender::send
    /// [`Sender`]: crate::sync::broadcast::Sender
    #[derive(Debug)]
    pub struct SendError<T>(pub T);

    impl<T> fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "channel closed")
        }
    }

    impl<T: fmt::Debug> std::error::Error for SendError<T> {}

    /// An error returned from the [`recv`] function on a [`Receiver`].
    ///
    /// [`recv`]: crate::sync::broadcast::Receiver::recv
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum RecvError {
        /// There are no more active senders implying no further messages will ever
        /// be sent.
        Closed,

        /// The receiver lagged too far behind. Attempting to receive again will
        /// return the oldest message still retained by the channel.
        ///
        /// Includes the number of skipped messages.
        Lagged(u64),
    }

    impl fmt::Display for RecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                RecvError::Closed => write!(f, "channel closed"),
                RecvError::Lagged(amt) => write!(f, "channel lagged by {}", amt),
            }
        }
    }

    impl std::error::Error for RecvError {}

    /// An error returned from the [`try_recv`] function on a [`Receiver`].
    ///
    /// [`try_recv`]: crate::sync::broadcast::Receiver::try_recv
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum TryRecvError {
        /// The channel is currently empty. There are still active
        /// [`Sender`] handles, so data may yet become available.
        ///
        /// [`Sender`]: crate::sync::broadcast::Sender
        Empty,

        /// There are no more active senders implying no further messages will ever
        /// be sent.
        Closed,

        /// The receiver lagged too far behind and has been forcibly disconnected.
        /// Attempting to receive again will return the oldest message still
        /// retained by the channel.
        ///
        /// Includes the number of skipped messages.
        Lagged(u64),
    }

    impl fmt::Display for TryRecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TryRecvError::Empty => write!(f, "channel empty"),
                TryRecvError::Closed => write!(f, "channel closed"),
                TryRecvError::Lagged(amt) => write!(f, "channel lagged by {}", amt),
            }
        }
    }

    impl std::error::Error for TryRecvError {}
}

use self::error::*;

/// Data shared between senders and receivers.
struct Shared<T> {
    /// slots in the channel.
    buffer: Box<[RwLock<Slot<T>>]>,

    /// Mask a position -> index.
    mask: usize,

    /// Tail of the queue. Includes the rx wait list.
    tail: Mutex<Tail>,

    /// Number of outstanding Sender handles.
    num_tx: AtomicUsize,
}

/// Next position to write a value.
struct Tail {
    /// Next position to write to.
    pos: u64,

    /// Number of active receivers.
    rx_cnt: usize,

    /// True if the channel is closed.
    closed: bool,

    /// Receivers waiting for a value.
    waiters: LinkedList<Waiter, <Waiter as linked_list::Link>::Target>,
}

/// Slot in the buffer.
struct Slot<T> {
    /// Remaining number of receivers that are expected to see this value.
    ///
    /// When this goes to zero, the value is released.
    ///
    /// An atomic is used as it is mutated concurrently with the slot read lock
    /// acquired.
    rem: AtomicUsize,

    /// Uniquely identifies the `send` stored in the slot.
    pos: u64,

    /// The value being broadcast.
    ///
    /// The value is set by `send` when the write lock is held. When a reader
    /// drops, `rem` is decremented. When it hits zero, the value is dropped.
    val: UnsafeCell<Option<T>>,
}

/// An entry in the wait queue.
struct Waiter {
    /// True if queued.
    queued: bool,

    /// Task waiting on the broadcast channel.
    waker: Option<Waker>,

    /// Intrusive linked-list pointers.
    pointers: linked_list::Pointers<Waiter>,

    /// Should not be `Unpin`.
    _p: PhantomPinned,
}

generate_addr_of_methods! {
    impl<> Waiter {
        unsafe fn addr_of_pointers(self: NonNull<Self>) -> NonNull<linked_list::Pointers<Waiter>> {
            &self.pointers
        }
    }
}

struct RecvGuard<'a, T> {
    slot: RwLockReadGuard<'a, Slot<T>>,
}

/// Receive a value future.
struct Recv<'a, T> {
    /// Receiver being waited on.
    receiver: &'a mut Receiver<T>,

    /// Entry in the waiter `LinkedList`.
    waiter: UnsafeCell<Waiter>,
}

unsafe impl<'a, T: Send> Send for Recv<'a, T> {}
unsafe impl<'a, T: Send> Sync for Recv<'a, T> {}

/// Max number of receivers. Reserve space to lock.
const MAX_RECEIVERS: usize = usize::MAX >> 2;

/// Create a bounded, multi-producer, multi-consumer channel where each sent
/// value is broadcasted to all active receivers.
///
/// All data sent on [`Sender`] will become available on every active
/// [`Receiver`] in the same order as it was sent.
///
/// The `Sender` can be cloned to `send` to the same channel from multiple
/// points in the process or it can be used concurrently from an `Arc`. New
/// `Receiver` handles are created by calling [`Sender::subscribe`].
///
/// If all [`Receiver`] handles are dropped, the `send` method will return a
/// [`SendError`]. Similarly, if all [`Sender`] handles are dropped, the [`recv`]
/// method will return a [`RecvError`].
///
/// [`Sender`]: crate::sync::broadcast::Sender
/// [`Sender::subscribe`]: crate::sync::broadcast::Sender::subscribe
/// [`Receiver`]: crate::sync::broadcast::Receiver
/// [`recv`]: crate::sync::broadcast::Receiver::recv
/// [`SendError`]: crate::sync::broadcast::error::SendError
/// [`RecvError`]: crate::sync::broadcast::error::RecvError
///
/// # Examples
///
/// ```
/// use tokio::sync::broadcast;
///
/// #[tokio::main]
/// async fn main() {
///     let (tx, mut rx1) = broadcast::channel(16);
///     let mut rx2 = tx.subscribe();
///
///     tokio::spawn(async move {
///         assert_eq!(rx1.recv().await.unwrap(), 10);
///         assert_eq!(rx1.recv().await.unwrap(), 20);
///     });
///
///     tokio::spawn(async move {
///         assert_eq!(rx2.recv().await.unwrap(), 10);
///         assert_eq!(rx2.recv().await.unwrap(), 20);
///     });
///
///     tx.send(10).unwrap();
///     tx.send(20).unwrap();
/// }
/// ```
///
/// # Panics
///
/// This will panic if `capacity` is equal to `0` or larger
/// than `usize::MAX / 2`.
#[track_caller]
pub fn channel<T: Clone>(mut capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "capacity is empty");
    assert!(capacity <= usize::MAX >> 1, "requested capacity too large");

    // Round to a power of two
    capacity = capacity.next_power_of_two();

    let mut buffer = Vec::with_capacity(capacity);

    for i in 0..capacity {
        buffer.push(RwLock::new(Slot {
            rem: AtomicUsize::new(0),
            pos: (i as u64).wrapping_sub(capacity as u64),
            val: UnsafeCell::new(None),
        }));
    }

    let shared = Arc::new(Shared {
        buffer: buffer.into_boxed_slice(),
        mask: capacity - 1,
        tail: Mutex::new(Tail {
            pos: 0,
            rx_cnt: 1,
            closed: false,
            waiters: LinkedList::new(),
        }),
        num_tx: AtomicUsize::new(1),
    });

    let rx = Receiver {
        shared: shared.clone(),
        next: 0,
    };

    let tx = Sender { shared };

    (tx, rx)
}

unsafe impl<T: Send> Send for Sender<T> {}
unsafe impl<T: Send> Sync for Sender<T> {}

unsafe impl<T: Send> Send for Receiver<T> {}
unsafe impl<T: Send> Sync for Receiver<T> {}

impl<T> Sender<T> {
    /// Attempts to send a value to all active [`Receiver`] handles, returning
    /// it back if it could not be sent.
    ///
    /// A successful send occurs when there is at least one active [`Receiver`]
    /// handle. An unsuccessful send would be one where all associated
    /// [`Receiver`] handles have already been dropped.
    ///
    /// # Return
    ///
    /// On success, the number of subscribed [`Receiver`] handles is returned.
    /// This does not mean that this number of receivers will see the message as
    /// a receiver may drop before receiving the message.
    ///
    /// # Note
    ///
    /// A return value of `Ok` **does not** mean that the sent value will be
    /// observed by all or any of the active [`Receiver`] handles. [`Receiver`]
    /// handles may be dropped before receiving the sent message.
    ///
    /// A return value of `Err` **does not** mean that future calls to `send`
    /// will fail. New [`Receiver`] handles may be created by calling
    /// [`subscribe`].
    ///
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    /// [`subscribe`]: crate::sync::broadcast::Sender::subscribe
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx1) = broadcast::channel(16);
    ///     let mut rx2 = tx.subscribe();
    ///
    ///     tokio::spawn(async move {
    ///         assert_eq!(rx1.recv().await.unwrap(), 10);
    ///         assert_eq!(rx1.recv().await.unwrap(), 20);
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         assert_eq!(rx2.recv().await.unwrap(), 10);
    ///         assert_eq!(rx2.recv().await.unwrap(), 20);
    ///     });
    ///
    ///     tx.send(10).unwrap();
    ///     tx.send(20).unwrap();
    /// }
    /// ```
    pub fn send(&self, value: T) -> Result<usize, SendError<T>> {
        let mut tail = self.shared.tail.lock();

        if tail.rx_cnt == 0 {
            return Err(SendError(value));
        }

        // Position to write into
        let pos = tail.pos;
        let rem = tail.rx_cnt;
        let idx = (pos & self.shared.mask as u64) as usize;

        // Update the tail position
        tail.pos = tail.pos.wrapping_add(1);

        // Get the slot
        let mut slot = self.shared.buffer[idx].write().unwrap();

        // Track the position
        slot.pos = pos;

        // Set remaining receivers
        slot.rem.with_mut(|v| *v = rem);

        // Write the value
        slot.val = UnsafeCell::new(Some(value));

        // Release the slot lock before notifying the receivers.
        drop(slot);

        // Notify and release the mutex. This must happen after the slot lock is
        // released, otherwise the writer lock bit could be cleared while another
        // thread is in the critical section.
        self.shared.notify_rx(tail);

        Ok(rem)
    }

    /// Creates a new [`Receiver`] handle that will receive values sent **after**
    /// this call to `subscribe`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, _rx) = broadcast::channel(16);
    ///
    ///     // Will not be seen
    ///     tx.send(10).unwrap();
    ///
    ///     let mut rx = tx.subscribe();
    ///
    ///     tx.send(20).unwrap();
    ///
    ///     let value = rx.recv().await.unwrap();
    ///     assert_eq!(20, value);
    /// }
    /// ```
    pub fn subscribe(&self) -> Receiver<T> {
        let shared = self.shared.clone();
        new_receiver(shared)
    }

    /// Returns the number of queued values.
    ///
    /// A value is queued until it has either been seen by all receivers that were alive at the time
    /// it was sent, or has been evicted from the queue by subsequent sends that exceeded the
    /// queue's capacity.
    ///
    /// # Note
    ///
    /// In contrast to [`Receiver::len`], this method only reports queued values and not values that
    /// have been evicted from the queue before being seen by all receivers.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx1) = broadcast::channel(16);
    ///     let mut rx2 = tx.subscribe();
    ///
    ///     tx.send(10).unwrap();
    ///     tx.send(20).unwrap();
    ///     tx.send(30).unwrap();
    ///
    ///     assert_eq!(tx.len(), 3);
    ///
    ///     rx1.recv().await.unwrap();
    ///
    ///     // The len is still 3 since rx2 hasn't seen the first value yet.
    ///     assert_eq!(tx.len(), 3);
    ///
    ///     rx2.recv().await.unwrap();
    ///
    ///     assert_eq!(tx.len(), 2);
    /// }
    /// ```
    pub fn len(&self) -> usize {
        let tail = self.shared.tail.lock();

        let base_idx = (tail.pos & self.shared.mask as u64) as usize;
        let mut low = 0;
        let mut high = self.shared.buffer.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let idx = base_idx.wrapping_add(mid) & self.shared.mask;
            if self.shared.buffer[idx].read().unwrap().rem.load(SeqCst) == 0 {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        self.shared.buffer.len() - low
    }

    /// Returns true if there are no queued values.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx1) = broadcast::channel(16);
    ///     let mut rx2 = tx.subscribe();
    ///
    ///     assert!(tx.is_empty());
    ///
    ///     tx.send(10).unwrap();
    ///
    ///     assert!(!tx.is_empty());
    ///
    ///     rx1.recv().await.unwrap();
    ///
    ///     // The queue is still not empty since rx2 hasn't seen the value.
    ///     assert!(!tx.is_empty());
    ///
    ///     rx2.recv().await.unwrap();
    ///
    ///     assert!(tx.is_empty());
    /// }
    /// ```
    pub fn is_empty(&self) -> bool {
        let tail = self.shared.tail.lock();

        let idx = (tail.pos.wrapping_sub(1) & self.shared.mask as u64) as usize;
        self.shared.buffer[idx].read().unwrap().rem.load(SeqCst) == 0
    }

    /// Returns the number of active receivers
    ///
    /// An active receiver is a [`Receiver`] handle returned from [`channel`] or
    /// [`subscribe`]. These are the handles that will receive values sent on
    /// this [`Sender`].
    ///
    /// # Note
    ///
    /// It is not guaranteed that a sent message will reach this number of
    /// receivers. Active receivers may never call [`recv`] again before
    /// dropping.
    ///
    /// [`recv`]: crate::sync::broadcast::Receiver::recv
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    /// [`Sender`]: crate::sync::broadcast::Sender
    /// [`subscribe`]: crate::sync::broadcast::Sender::subscribe
    /// [`channel`]: crate::sync::broadcast::channel
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, _rx1) = broadcast::channel(16);
    ///
    ///     assert_eq!(1, tx.receiver_count());
    ///
    ///     let mut _rx2 = tx.subscribe();
    ///
    ///     assert_eq!(2, tx.receiver_count());
    ///
    ///     tx.send(10).unwrap();
    /// }
    /// ```
    pub fn receiver_count(&self) -> usize {
        let tail = self.shared.tail.lock();
        tail.rx_cnt
    }

    /// Returns `true` if senders belong to the same channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, _rx) = broadcast::channel::<()>(16);
    ///     let tx2 = tx.clone();
    ///
    ///     assert!(tx.same_channel(&tx2));
    ///
    ///     let (tx3, _rx3) = broadcast::channel::<()>(16);
    ///
    ///     assert!(!tx3.same_channel(&tx2));
    /// }
    /// ```
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    fn close_channel(&self) {
        let mut tail = self.shared.tail.lock();
        tail.closed = true;

        self.shared.notify_rx(tail);
    }
}

/// Create a new `Receiver` which reads starting from the tail.
fn new_receiver<T>(shared: Arc<Shared<T>>) -> Receiver<T> {
    let mut tail = shared.tail.lock();

    if tail.rx_cnt == MAX_RECEIVERS {
        panic!("max receivers");
    }

    tail.rx_cnt = tail.rx_cnt.checked_add(1).expect("overflow");

    let next = tail.pos;

    drop(tail);

    Receiver { shared, next }
}

impl<T> Shared<T> {
    fn notify_rx<'a, 'b: 'a>(&'b self, mut tail: MutexGuard<'a, Tail>) {
        let mut wakers = WakeList::new();
        'outer: loop {
            while wakers.can_push() {
                match tail.waiters.pop_back() {
                    Some(mut waiter) => {
                        // Safety: `tail` lock is still held.
                        let waiter = unsafe { waiter.as_mut() };

                        assert!(waiter.queued);
                        waiter.queued = false;

                        if let Some(waker) = waiter.waker.take() {
                            wakers.push(waker);
                        }
                    }
                    None => {
                        break 'outer;
                    }
                }
            }

            // Release the lock before waking.
            drop(tail);

            // Before we acquire the lock again all sorts of things can happen:
            // some waiters may remove themselves from the list and new waiters
            // may be added. This is fine since at worst we will unnecessarily
            // wake up waiters which will then queue themselves again.

            wakers.wake_all();

            // Acquire the lock again.
            tail = self.tail.lock();
        }

        // Release the lock before waking.
        drop(tail);

        wakers.wake_all();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        let shared = self.shared.clone();
        shared.num_tx.fetch_add(1, SeqCst);

        Sender { shared }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if 1 == self.shared.num_tx.fetch_sub(1, SeqCst) {
            self.close_channel();
        }
    }
}

impl<T> Receiver<T> {
    /// Returns the number of messages that were sent into the channel and that
    /// this [`Receiver`] has yet to receive.
    ///
    /// If the returned value from `len` is larger than the next largest power of 2
    /// of the capacity of the channel any call to [`recv`] will return an
    /// `Err(RecvError::Lagged)` and any call to [`try_recv`] will return an
    /// `Err(TryRecvError::Lagged)`, e.g. if the capacity of the channel is 10,
    /// [`recv`] will start to return `Err(RecvError::Lagged)` once `len` returns
    /// values larger than 16.
    ///
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    /// [`recv`]: crate::sync::broadcast::Receiver::recv
    /// [`try_recv`]: crate::sync::broadcast::Receiver::try_recv
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx1) = broadcast::channel(16);
    ///
    ///     tx.send(10).unwrap();
    ///     tx.send(20).unwrap();
    ///
    ///     assert_eq!(rx1.len(), 2);
    ///     assert_eq!(rx1.recv().await.unwrap(), 10);
    ///     assert_eq!(rx1.len(), 1);
    ///     assert_eq!(rx1.recv().await.unwrap(), 20);
    ///     assert_eq!(rx1.len(), 0);
    /// }
    /// ```
    pub fn len(&self) -> usize {
        let next_send_pos = self.shared.tail.lock().pos;
        (next_send_pos - self.next) as usize
    }

    /// Returns true if there aren't any messages in the channel that the [`Receiver`]
    /// has yet to receive.
    ///
    /// [`Receiver]: create::sync::broadcast::Receiver
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx1) = broadcast::channel(16);
    ///
    ///     assert!(rx1.is_empty());
    ///
    ///     tx.send(10).unwrap();
    ///     tx.send(20).unwrap();
    ///
    ///     assert!(!rx1.is_empty());
    ///     assert_eq!(rx1.recv().await.unwrap(), 10);
    ///     assert_eq!(rx1.recv().await.unwrap(), 20);
    ///     assert!(rx1.is_empty());
    /// }
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if receivers belong to the same channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, rx) = broadcast::channel::<()>(16);
    ///     let rx2 = tx.subscribe();
    ///
    ///     assert!(rx.same_channel(&rx2));
    ///
    ///     let (_tx3, rx3) = broadcast::channel::<()>(16);
    ///
    ///     assert!(!rx3.same_channel(&rx2));
    /// }
    /// ```
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// Locks the next value if there is one.
    fn recv_ref(
        &mut self,
        waiter: Option<(&UnsafeCell<Waiter>, &Waker)>,
    ) -> Result<RecvGuard<'_, T>, TryRecvError> {
        let idx = (self.next & self.shared.mask as u64) as usize;

        // The slot holding the next value to read
        let mut slot = self.shared.buffer[idx].read().unwrap();

        if slot.pos != self.next {
            // Release the `slot` lock before attempting to acquire the `tail`
            // lock. This is required because `send2` acquires the tail lock
            // first followed by the slot lock. Acquiring the locks in reverse
            // order here would result in a potential deadlock: `recv_ref`
            // acquires the `slot` lock and attempts to acquire the `tail` lock
            // while `send2` acquired the `tail` lock and attempts to acquire
            // the slot lock.
            drop(slot);

            let mut old_waker = None;

            let mut tail = self.shared.tail.lock();

            // Acquire slot lock again
            slot = self.shared.buffer[idx].read().unwrap();

            // Make sure the position did not change. This could happen in the
            // unlikely event that the buffer is wrapped between dropping the
            // read lock and acquiring the tail lock.
            if slot.pos != self.next {
                let next_pos = slot.pos.wrapping_add(self.shared.buffer.len() as u64);

                if next_pos == self.next {
                    // At this point the channel is empty for *this* receiver. If
                    // it's been closed, then that's what we return, otherwise we
                    // set a waker and return empty.
                    if tail.closed {
                        return Err(TryRecvError::Closed);
                    }

                    // Store the waker
                    if let Some((waiter, waker)) = waiter {
                        // Safety: called while locked.
                        unsafe {
                            // Only queue if not already queued
                            waiter.with_mut(|ptr| {
                                // If there is no waker **or** if the currently
                                // stored waker references a **different** task,
                                // track the tasks' waker to be notified on
                                // receipt of a new value.
                                match (*ptr).waker {
                                    Some(ref w) if w.will_wake(waker) => {}
                                    _ => {
                                        old_waker = std::mem::replace(
                                            &mut (*ptr).waker,
                                            Some(waker.clone()),
                                        );
                                    }
                                }

                                if !(*ptr).queued {
                                    (*ptr).queued = true;
                                    tail.waiters.push_front(NonNull::new_unchecked(&mut *ptr));
                                }
                            });
                        }
                    }

                    // Drop the old waker after releasing the locks.
                    drop(slot);
                    drop(tail);
                    drop(old_waker);

                    return Err(TryRecvError::Empty);
                }

                // At this point, the receiver has lagged behind the sender by
                // more than the channel capacity. The receiver will attempt to
                // catch up by skipping dropped messages and setting the
                // internal cursor to the **oldest** message stored by the
                // channel.
                let next = tail.pos.wrapping_sub(self.shared.buffer.len() as u64);

                let missed = next.wrapping_sub(self.next);

                drop(tail);

                // The receiver is slow but no values have been missed
                if missed == 0 {
                    self.next = self.next.wrapping_add(1);

                    return Ok(RecvGuard { slot });
                }

                self.next = next;

                return Err(TryRecvError::Lagged(missed));
            }
        }

        self.next = self.next.wrapping_add(1);

        Ok(RecvGuard { slot })
    }
}

impl<T: Clone> Receiver<T> {
    /// Re-subscribes to the channel starting from the current tail element.
    ///
    /// This [`Receiver`] handle will receive a clone of all values sent
    /// **after** it has resubscribed. This will not include elements that are
    /// in the queue of the current receiver. Consider the following example.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///   let (tx, mut rx) = broadcast::channel(2);
    ///
    ///   tx.send(1).unwrap();
    ///   let mut rx2 = rx.resubscribe();
    ///   tx.send(2).unwrap();
    ///
    ///   assert_eq!(rx2.recv().await.unwrap(), 2);
    ///   assert_eq!(rx.recv().await.unwrap(), 1);
    /// }
    /// ```
    pub fn resubscribe(&self) -> Self {
        let shared = self.shared.clone();
        new_receiver(shared)
    }
    /// Receives the next value for this receiver.
    ///
    /// Each [`Receiver`] handle will receive a clone of all values sent
    /// **after** it has subscribed.
    ///
    /// `Err(RecvError::Closed)` is returned when all `Sender` halves have
    /// dropped, indicating that no further values can be sent on the channel.
    ///
    /// If the [`Receiver`] handle falls behind, once the channel is full, newly
    /// sent values will overwrite old values. At this point, a call to [`recv`]
    /// will return with `Err(RecvError::Lagged)` and the [`Receiver`]'s
    /// internal cursor is updated to point to the oldest value still held by
    /// the channel. A subsequent call to [`recv`] will return this value
    /// **unless** it has been since overwritten.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe. If `recv` is used as the event in a
    /// [`tokio::select!`](crate::select) statement and some other branch
    /// completes first, it is guaranteed that no messages were received on this
    /// channel.
    ///
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    /// [`recv`]: crate::sync::broadcast::Receiver::recv
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx1) = broadcast::channel(16);
    ///     let mut rx2 = tx.subscribe();
    ///
    ///     tokio::spawn(async move {
    ///         assert_eq!(rx1.recv().await.unwrap(), 10);
    ///         assert_eq!(rx1.recv().await.unwrap(), 20);
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         assert_eq!(rx2.recv().await.unwrap(), 10);
    ///         assert_eq!(rx2.recv().await.unwrap(), 20);
    ///     });
    ///
    ///     tx.send(10).unwrap();
    ///     tx.send(20).unwrap();
    /// }
    /// ```
    ///
    /// Handling lag
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = broadcast::channel(2);
    ///
    ///     tx.send(10).unwrap();
    ///     tx.send(20).unwrap();
    ///     tx.send(30).unwrap();
    ///
    ///     // The receiver lagged behind
    ///     assert!(rx.recv().await.is_err());
    ///
    ///     // At this point, we can abort or continue with lost messages
    ///
    ///     assert_eq!(20, rx.recv().await.unwrap());
    ///     assert_eq!(30, rx.recv().await.unwrap());
    /// }
    /// ```
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        let fut = Recv::new(self);
        fut.await
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    ///
    /// This is useful for a flavor of "optimistic check" before deciding to
    /// await on a receiver.
    ///
    /// Compared with [`recv`], this function has three failure cases instead of two
    /// (one for closed, one for an empty buffer, one for a lagging receiver).
    ///
    /// `Err(TryRecvError::Closed)` is returned when all `Sender` halves have
    /// dropped, indicating that no further values can be sent on the channel.
    ///
    /// If the [`Receiver`] handle falls behind, once the channel is full, newly
    /// sent values will overwrite old values. At this point, a call to [`recv`]
    /// will return with `Err(TryRecvError::Lagged)` and the [`Receiver`]'s
    /// internal cursor is updated to point to the oldest value still held by
    /// the channel. A subsequent call to [`try_recv`] will return this value
    /// **unless** it has been since overwritten. If there are no values to
    /// receive, `Err(TryRecvError::Empty)` is returned.
    ///
    /// [`recv`]: crate::sync::broadcast::Receiver::recv
    /// [`try_recv`]: crate::sync::broadcast::Receiver::try_recv
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = broadcast::channel(16);
    ///
    ///     assert!(rx.try_recv().is_err());
    ///
    ///     tx.send(10).unwrap();
    ///
    ///     let value = rx.try_recv().unwrap();
    ///     assert_eq!(10, value);
    /// }
    /// ```
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let guard = self.recv_ref(None)?;
        guard.clone_value().ok_or(TryRecvError::Closed)
    }

    /// Blocking receive to call outside of asynchronous contexts.
    ///
    /// # Panics
    ///
    /// This function panics if called within an asynchronous execution
    /// context.
    ///
    /// # Examples
    /// ```
    /// use std::thread;
    /// use tokio::sync::broadcast;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx, mut rx) = broadcast::channel(16);
    ///
    ///     let sync_code = thread::spawn(move || {
    ///         assert_eq!(rx.blocking_recv(), Ok(10));
    ///     });
    ///
    ///     let _ = tx.send(10);
    ///     sync_code.join().unwrap();
    /// }
    pub fn blocking_recv(&mut self) -> Result<T, RecvError> {
        crate::future::block_on(self.recv())
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut tail = self.shared.tail.lock();

        tail.rx_cnt -= 1;
        let until = tail.pos;

        drop(tail);

        while self.next < until {
            match self.recv_ref(None) {
                Ok(_) => {}
                // The channel is closed
                Err(TryRecvError::Closed) => break,
                // Ignore lagging, we will catch up
                Err(TryRecvError::Lagged(..)) => {}
                // Can't be empty
                Err(TryRecvError::Empty) => panic!("unexpected empty broadcast channel"),
            }
        }
    }
}

impl<'a, T> Recv<'a, T> {
    fn new(receiver: &'a mut Receiver<T>) -> Recv<'a, T> {
        Recv {
            receiver,
            waiter: UnsafeCell::new(Waiter {
                queued: false,
                waker: None,
                pointers: linked_list::Pointers::new(),
                _p: PhantomPinned,
            }),
        }
    }

    /// A custom `project` implementation is used in place of `pin-project-lite`
    /// as a custom drop implementation is needed.
    fn project(self: Pin<&mut Self>) -> (&mut Receiver<T>, &UnsafeCell<Waiter>) {
        unsafe {
            // Safety: Receiver is Unpin
            is_unpin::<&mut Receiver<T>>();

            let me = self.get_unchecked_mut();
            (me.receiver, &me.waiter)
        }
    }
}

impl<'a, T> Future for Recv<'a, T>
where
    T: Clone,
{
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        ready!(crate::trace::trace_leaf(cx));

        let (receiver, waiter) = self.project();

        let guard = match receiver.recv_ref(Some((waiter, cx.waker()))) {
            Ok(value) => value,
            Err(TryRecvError::Empty) => return Poll::Pending,
            Err(TryRecvError::Lagged(n)) => return Poll::Ready(Err(RecvError::Lagged(n))),
            Err(TryRecvError::Closed) => return Poll::Ready(Err(RecvError::Closed)),
        };

        Poll::Ready(guard.clone_value().ok_or(RecvError::Closed))
    }
}

impl<'a, T> Drop for Recv<'a, T> {
    fn drop(&mut self) {
        // Acquire the tail lock. This is required for safety before accessing
        // the waiter node.
        let mut tail = self.receiver.shared.tail.lock();

        // safety: tail lock is held
        let queued = self.waiter.with(|ptr| unsafe { (*ptr).queued });

        if queued {
            // Remove the node
            //
            // safety: tail lock is held and the wait node is verified to be in
            // the list.
            unsafe {
                self.waiter.with_mut(|ptr| {
                    tail.waiters.remove((&mut *ptr).into());
                });
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

    unsafe fn pointers(target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
        Waiter::addr_of_pointers(target)
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "broadcast::Sender")
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "broadcast::Receiver")
    }
}

impl<'a, T> RecvGuard<'a, T> {
    fn clone_value(&self) -> Option<T>
    where
        T: Clone,
    {
        self.slot.val.with(|ptr| unsafe { (*ptr).clone() })
    }
}

impl<'a, T> Drop for RecvGuard<'a, T> {
    fn drop(&mut self) {
        // Decrement the remaining counter
        if 1 == self.slot.rem.fetch_sub(1, SeqCst) {
            // Safety: Last receiver, drop the value
            self.slot.val.with_mut(|ptr| unsafe { *ptr = None });
        }
    }
}

fn is_unpin<T: Unpin>() {}
