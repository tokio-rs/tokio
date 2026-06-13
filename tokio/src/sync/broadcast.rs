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
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let (tx, mut rx1) = broadcast::channel(16);
//! let mut rx2 = tx.subscribe();
//!
//! tokio::spawn(async move {
//!     assert_eq!(rx1.recv().await.unwrap(), 10);
//!     assert_eq!(rx1.recv().await.unwrap(), 20);
//! });
//!
//! tokio::spawn(async move {
//!     assert_eq!(rx2.recv().await.unwrap(), 10);
//!     assert_eq!(rx2.recv().await.unwrap(), 20);
//! });
//!
//! tx.send(10).unwrap();
//! tx.send(20).unwrap();
//! # }
//! ```
//!
//! Handling lag
//!
//! ```
//! use tokio::sync::broadcast;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let (tx, mut rx) = broadcast::channel(2);
//!
//! tx.send(10).unwrap();
//! tx.send(20).unwrap();
//! tx.send(30).unwrap();
//!
//! // The receiver lagged behind
//! assert!(rx.recv().await.is_err());
//!
//! // At this point, we can abort or continue with lost messages
//!
//! assert_eq!(20, rx.recv().await.unwrap());
//! assert_eq!(30, rx.recv().await.unwrap());
//! # }
//! ```

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::{Arc, Mutex, MutexGuard};
use crate::task::coop::cooperative;
use crate::util::linked_list;
use crate::util::sharded_list::{ShardedList, ShardedListItem};
use crate::util::WakeList;

use std::fmt;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release, SeqCst};
use std::task::{ready, Context, Poll, Waker};

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
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let (tx, mut rx1) = broadcast::channel(16);
/// let mut rx2 = tx.subscribe();
///
/// tokio::spawn(async move {
///     assert_eq!(rx1.recv().await.unwrap(), 10);
///     assert_eq!(rx1.recv().await.unwrap(), 20);
/// });
///
/// tokio::spawn(async move {
///     assert_eq!(rx2.recv().await.unwrap(), 10);
///     assert_eq!(rx2.recv().await.unwrap(), 20);
/// });
///
/// tx.send(10).unwrap();
/// tx.send(20).unwrap();
/// # }
/// ```
///
/// [`broadcast`]: crate::sync::broadcast
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// A sender that does not prevent the channel from being closed.
///
/// If all [`Sender`] instances of a channel were dropped and only `WeakSender`
/// instances remain, the channel is closed.
///
/// In order to send messages, the `WeakSender` needs to be upgraded using
/// [`WeakSender::upgrade`], which returns `Option<Sender>`. It returns `None`
/// if all `Sender`s have been dropped, and otherwise it returns a `Sender`.
///
/// [`Sender`]: Sender
/// [`WeakSender::upgrade`]: WeakSender::upgrade
///
/// # Examples
///
/// ```
/// use tokio::sync::broadcast::channel;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let (tx, _rx) = channel::<i32>(15);
/// let tx_weak = tx.downgrade();
///
/// // Upgrading will succeed because `tx` still exists.
/// assert!(tx_weak.upgrade().is_some());
///
/// // If we drop `tx`, then it will fail.
/// drop(tx);
/// assert!(tx_weak.clone().upgrade().is_none());
/// # }
/// ```
pub struct WeakSender<T> {
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
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let (tx, mut rx1) = broadcast::channel(16);
/// let mut rx2 = tx.subscribe();
///
/// tokio::spawn(async move {
///     assert_eq!(rx1.recv().await.unwrap(), 10);
///     assert_eq!(rx1.recv().await.unwrap(), 20);
/// });
///
/// tokio::spawn(async move {
///     assert_eq!(rx2.recv().await.unwrap(), 10);
///     assert_eq!(rx2.recv().await.unwrap(), 20);
/// });
///
/// tx.send(10).unwrap();
/// tx.send(20).unwrap();
/// # }
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

    /// Error returned by the [`send`] function on a [`Sender`].
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
                RecvError::Lagged(amt) => write!(f, "channel lagged by {amt}"),
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
                TryRecvError::Lagged(amt) => write!(f, "channel lagged by {amt}"),
            }
        }
    }

    impl std::error::Error for TryRecvError {}
}

use self::error::{RecvError, SendError, TryRecvError};

use super::Notify;

/// Data shared between senders and receivers.
struct Shared<T> {
    /// slots in the channel.
    buffer: Box<[Mutex<Slot<T>>]>,

    /// Mask a position -> index.
    mask: usize,

    /// Tail of the queue. Tracks the next write position and receiver count.
    tail: Mutex<Tail>,

    /// True once the channel is closed.
    ///
    /// Stored as an atomic (rather than inside `Tail`) so that a parking
    /// receiver can check it without taking the `tail` lock. Writes are still
    /// performed under the `tail` lock so they stay ordered with `rx_cnt`.
    closed: AtomicBool,

    /// Receivers waiting for a value, sharded to reduce contention between
    /// parking (`recv_ref`), cancelling (`Recv::drop`), and waking
    /// (`notify_rx`).
    waiters: ShardedList<Waiter, Waiter>,

    /// Round-robin counter used to assign waiters to shards under loom, where
    /// node addresses are not stable across executions. Unused outside of loom.
    #[cfg(loom)]
    next_shard: std::sync::atomic::AtomicUsize,

    /// Number of outstanding Sender handles.
    num_tx: AtomicUsize,

    /// Number of outstanding weak Sender handles.
    num_weak_tx: AtomicUsize,

    /// Notify when the last subscribed [`Receiver`] drops.
    notify_last_rx_drop: Notify,
}

/// Next position to write a value.
struct Tail {
    /// Next position to write to.
    pos: u64,

    /// Number of active receivers.
    rx_cnt: usize,
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
    val: Option<T>,
}

/// An entry in the wait queue.
struct Waiter {
    /// True if queued.
    queued: AtomicBool,

    /// Task waiting on the broadcast channel.
    waker: Option<Waker>,

    /// Intrusive linked-list pointers.
    pointers: linked_list::Pointers<Waiter>,

    /// Shard this waiter is assigned to.
    ///
    /// Under loom, pointer addresses are not stable across executions, so we
    /// cannot derive the shard from the node address (it would make the model
    /// non-deterministic). Instead the shard is assigned from a per-channel
    /// counter when the waiter is created and stored here. Outside of loom the
    /// shard is derived from the (stable) node address, so this field is unused.
    #[cfg(loom)]
    shard: usize,

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
    slot: MutexGuard<'a, Slot<T>>,
}

/// Receive a value future.
struct Recv<'a, T> {
    /// Receiver being waited on.
    receiver: &'a mut Receiver<T>,

    /// Entry in the sharded waiter list.
    waiter: WaiterCell,
}

// The wrapper around `UnsafeCell` isolates the unsafe impl `Send` and `Sync`
// from `Recv`.
struct WaiterCell(UnsafeCell<Waiter>);

unsafe impl Send for WaiterCell {}
unsafe impl Sync for WaiterCell {}

/// Max number of receivers. Reserve space to lock.
const MAX_RECEIVERS: usize = usize::MAX >> 2;

/// Number of shards the waiter list is split into. A small power of two keeps
/// the per-`send` cost of visiting every shard low while spreading the lock that
/// parking and cancelling receivers contend on. Under loom the shard count is
/// reduced to keep the explored state space small.
#[cfg(not(loom))]
const WAITER_SHARDS: usize = 8;
#[cfg(loom)]
const WAITER_SHARDS: usize = 2;

/// Create a bounded, multi-producer, multi-consumer channel where each sent
/// value is broadcasted to all active receivers.
///
/// **Note:** The actual capacity may be greater than the provided `capacity`.
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
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let (tx, mut rx1) = broadcast::channel(16);
/// let mut rx2 = tx.subscribe();
///
/// tokio::spawn(async move {
///     assert_eq!(rx1.recv().await.unwrap(), 10);
///     assert_eq!(rx1.recv().await.unwrap(), 20);
/// });
///
/// tokio::spawn(async move {
///     assert_eq!(rx2.recv().await.unwrap(), 10);
///     assert_eq!(rx2.recv().await.unwrap(), 20);
/// });
///
/// tx.send(10).unwrap();
/// tx.send(20).unwrap();
/// # }
/// ```
///
/// # Panics
///
/// This will panic if `capacity` is equal to `0`.
///
/// This pre-allocates space for `capacity` messages. Allocation failure may result in a panic or
/// [an allocation error](std::alloc::handle_alloc_error).
#[track_caller]
pub fn channel<T: Clone>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    // SAFETY: In the line below we are creating one extra receiver, so there will be 1 in total.
    let tx = unsafe { Sender::new_with_receiver_count(1, capacity) };
    let rx = Receiver {
        shared: tx.shared.clone(),
        next: 0,
    };
    (tx, rx)
}

impl<T> Sender<T> {
    /// Creates the sending-half of the [`broadcast`] channel.
    ///
    /// See the documentation of [`broadcast::channel`] for more information on this method.
    ///
    /// [`broadcast`]: crate::sync::broadcast
    /// [`broadcast::channel`]: crate::sync::broadcast::channel
    #[track_caller]
    pub fn new(capacity: usize) -> Self {
        // SAFETY: We don't create extra receivers, so there are 0.
        unsafe { Self::new_with_receiver_count(0, capacity) }
    }

    /// Creates the sending-half of the [`broadcast`](self) channel, and provide the receiver
    /// count.
    ///
    /// See the documentation of [`broadcast::channel`](self::channel) for more errors when
    /// calling this function.
    ///
    /// # Safety:
    ///
    /// The caller must ensure that the amount of receivers for this Sender is correct before
    /// the channel functionalities are used, the count is zero by default, as this function
    /// does not create any receivers by itself.
    #[track_caller]
    unsafe fn new_with_receiver_count(receiver_count: usize, mut capacity: usize) -> Self {
        assert!(capacity > 0, "broadcast channel capacity cannot be zero");
        assert!(
            capacity <= usize::MAX >> 1,
            "broadcast channel capacity exceeded `usize::MAX / 2`"
        );

        // Round to a power of two
        capacity = capacity.next_power_of_two();

        let buffer = (0..capacity).map(|i| {
            Mutex::new(Slot {
                rem: AtomicUsize::new(0),
                pos: (i as u64).wrapping_sub(capacity as u64),
                val: None,
            })
        });

        let shared = Arc::new(Shared {
            buffer: buffer.collect(),
            mask: capacity - 1,
            tail: Mutex::new(Tail {
                pos: 0,
                rx_cnt: receiver_count,
            }),
            closed: AtomicBool::new(receiver_count == 0),
            waiters: ShardedList::new(WAITER_SHARDS),
            #[cfg(loom)]
            next_shard: std::sync::atomic::AtomicUsize::new(0),
            num_tx: AtomicUsize::new(1),
            num_weak_tx: AtomicUsize::new(0),
            notify_last_rx_drop: Notify::new(),
        });

        Sender { shared }
    }

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
    /// a receiver may drop or lag ([see lagging](self#lagging)) before receiving
    /// the message.
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel(16);
    /// let mut rx2 = tx.subscribe();
    ///
    /// tokio::spawn(async move {
    ///     assert_eq!(rx1.recv().await.unwrap(), 10);
    ///     assert_eq!(rx1.recv().await.unwrap(), 20);
    /// });
    ///
    /// tokio::spawn(async move {
    ///     assert_eq!(rx2.recv().await.unwrap(), 10);
    ///     assert_eq!(rx2.recv().await.unwrap(), 20);
    /// });
    ///
    /// tx.send(10).unwrap();
    /// tx.send(20).unwrap();
    /// # }
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
        let mut slot = self.shared.buffer[idx].lock();

        // Track the position
        slot.pos = pos;

        // Set remaining receivers
        slot.rem.with_mut(|v| *v = rem);

        // Write the value
        slot.val = Some(value);

        // Release the slot lock before notifying the receivers.
        drop(slot);

        // Fast path: if no receiver is parked, there is nothing to wake, so skip
        // locking every shard. A single relaxed load suffices: a receiver waiting
        // for the value we just wrote enqueued itself before observing the old
        // (empty) slot, so — having written the slot above — its increment of the
        // waiter count is ordered before this load through the slot lock. (Once
        // there is any parked receiver this load is non-zero and we take the slow
        // path; only the genuinely-empty case is skipped.)
        if self.shared.waiters.is_empty() {
            drop(tail);
        } else {
            // Notify and release the mutex. This must happen after the slot lock
            // is released, otherwise the writer lock bit could be cleared while
            // another thread is in the critical section.
            self.shared.notify_rx(tail);
        }

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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, _rx) = broadcast::channel(16);
    ///
    /// // Will not be seen
    /// tx.send(10).unwrap();
    ///
    /// let mut rx = tx.subscribe();
    ///
    /// tx.send(20).unwrap();
    ///
    /// let value = rx.recv().await.unwrap();
    /// assert_eq!(20, value);
    /// # }
    /// ```
    pub fn subscribe(&self) -> Receiver<T> {
        let shared = self.shared.clone();
        new_receiver(shared)
    }

    /// Converts the `Sender` to a [`WeakSender`] that does not count
    /// towards RAII semantics, i.e. if all `Sender` instances of the
    /// channel were dropped and only `WeakSender` instances remain,
    /// the channel is closed.
    #[must_use = "Downgrade creates a WeakSender without destroying the original non-weak sender."]
    pub fn downgrade(&self) -> WeakSender<T> {
        self.shared.num_weak_tx.fetch_add(1, Relaxed);
        WeakSender {
            shared: self.shared.clone(),
        }
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel(16);
    /// let mut rx2 = tx.subscribe();
    ///
    /// tx.send(10).unwrap();
    /// tx.send(20).unwrap();
    /// tx.send(30).unwrap();
    ///
    /// assert_eq!(tx.len(), 3);
    ///
    /// rx1.recv().await.unwrap();
    ///
    /// // The len is still 3 since rx2 hasn't seen the first value yet.
    /// assert_eq!(tx.len(), 3);
    ///
    /// rx2.recv().await.unwrap();
    ///
    /// assert_eq!(tx.len(), 2);
    /// # }
    /// ```
    pub fn len(&self) -> usize {
        let tail = self.shared.tail.lock();

        let base_idx = (tail.pos & self.shared.mask as u64) as usize;
        let mut low = 0;
        let mut high = self.shared.buffer.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let idx = base_idx.wrapping_add(mid) & self.shared.mask;
            if self.shared.buffer[idx].lock().rem.load(SeqCst) == 0 {
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel(16);
    /// let mut rx2 = tx.subscribe();
    ///
    /// assert!(tx.is_empty());
    ///
    /// tx.send(10).unwrap();
    ///
    /// assert!(!tx.is_empty());
    ///
    /// rx1.recv().await.unwrap();
    ///
    /// // The queue is still not empty since rx2 hasn't seen the value.
    /// assert!(!tx.is_empty());
    ///
    /// rx2.recv().await.unwrap();
    ///
    /// assert!(tx.is_empty());
    /// # }
    /// ```
    pub fn is_empty(&self) -> bool {
        let tail = self.shared.tail.lock();

        let idx = (tail.pos.wrapping_sub(1) & self.shared.mask as u64) as usize;
        self.shared.buffer[idx].lock().rem.load(SeqCst) == 0
    }

    /// Returns the number of active receivers.
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, _rx1) = broadcast::channel(16);
    ///
    /// assert_eq!(1, tx.receiver_count());
    ///
    /// let mut _rx2 = tx.subscribe();
    ///
    /// assert_eq!(2, tx.receiver_count());
    ///
    /// tx.send(10).unwrap();
    /// # }
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, _rx) = broadcast::channel::<()>(16);
    /// let tx2 = tx.clone();
    ///
    /// assert!(tx.same_channel(&tx2));
    ///
    /// let (tx3, _rx3) = broadcast::channel::<()>(16);
    ///
    /// assert!(!tx3.same_channel(&tx2));
    /// # }
    /// ```
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// A future which completes when the number of [Receiver]s subscribed to this `Sender` reaches
    /// zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures::FutureExt;
    /// use tokio::sync::broadcast;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel::<u32>(16);
    /// let mut rx2 = tx.subscribe();
    ///
    /// let _ = tx.send(10);
    ///
    /// assert_eq!(rx1.recv().await.unwrap(), 10);
    /// drop(rx1);
    /// assert!(tx.closed().now_or_never().is_none());
    ///
    /// assert_eq!(rx2.recv().await.unwrap(), 10);
    /// drop(rx2);
    /// assert!(tx.closed().now_or_never().is_some());
    /// # }
    /// ```
    pub async fn closed(&self) {
        loop {
            let notified = self.shared.notify_last_rx_drop.notified();

            {
                // Hold the tail lock while checking `closed` so this stays
                // ordered with `Receiver::drop`, which sets `closed` and notifies
                // `notify_last_rx_drop` under the same lock.
                let _tail = self.shared.tail.lock();
                if self.shared.closed.load(Acquire) {
                    return;
                }
            }

            notified.await;
        }
    }

    fn close_channel(&self) {
        let tail = self.shared.tail.lock();
        // Set `closed` (under the tail lock, so it stays ordered with reopen in
        // `new_receiver`) before draining, so a receiver that re-checks `closed`
        // after enqueueing observes it.
        self.shared.closed.store(true, Release);

        self.shared.notify_rx(tail);
    }

    /// Returns the number of [`Sender`] handles.
    pub fn strong_count(&self) -> usize {
        self.shared.num_tx.load(Acquire)
    }

    /// Returns the number of [`WeakSender`] handles.
    pub fn weak_count(&self) -> usize {
        self.shared.num_weak_tx.load(Acquire)
    }
}

/// Create a new `Receiver` which reads starting from the tail.
fn new_receiver<T>(shared: Arc<Shared<T>>) -> Receiver<T> {
    let mut tail = shared.tail.lock();

    assert!(tail.rx_cnt != MAX_RECEIVERS, "max receivers");

    if tail.rx_cnt == 0 {
        // Potentially need to re-open the channel, if a new receiver has been added between calls
        // to poll(). Note that we use rx_cnt == 0 instead of is_closed since is_closed also
        // applies if the sender has been dropped
        shared.closed.store(false, Release);
    }

    tail.rx_cnt = tail.rx_cnt.checked_add(1).expect("overflow");
    let next = tail.pos;

    drop(tail);

    Receiver { shared, next }
}

impl<T> Shared<T> {
    fn notify_rx(&self, tail: MutexGuard<'_, Tail>) {
        // The waiter list lives in its own sharded structure, independent of the
        // `tail` lock. Release `tail` before waking so that senders and parking
        // receivers do not contend on it while wakers run.
        //
        // This drains every shard unconditionally. `send` skips the call entirely
        // when there are no waiters (a relaxed `is_empty` it can make safely,
        // because it has just written the slot); `close_channel` cannot make that
        // check safely — a receiver may enqueue and re-check `closed` without any
        // happens-before to a count load here — so it must always drain.
        drop(tail);

        let mut wakers = WakeList::new();
        for shard_id in 0..self.waiters.shard_size() {
            'shard: loop {
                // Drain a batch of waiters from this shard under a single lock
                // acquisition (the lock is held for the whole `while` below), so
                // we don't re-lock the shard per waiter. The waker is taken and
                // `queued` is cleared while the shard lock is held, so we cannot
                // race a concurrent `Recv::drop` removing the same node.
                {
                    let mut shard = self.waiters.lock_shard_by_id(shard_id);
                    while wakers.can_push() {
                        match shard.pop_back() {
                            Some(waiter) => {
                                let ptr = waiter.as_ptr();
                                // Safety: the node was just popped under the shard
                                // lock and is a valid, pinned `Waiter`;
                                // `Recv::drop` cannot reclaim it while the shard
                                // lock is held.
                                unsafe {
                                    if let Some(waker) = (*ptr).waker.take() {
                                        wakers.push(waker);
                                    }

                                    // Safety: `queued` is atomic.
                                    let queued = &(*ptr).queued;
                                    // The node was in the list, so it must be
                                    // queued.
                                    debug_assert!(queued.load(Relaxed));
                                    // `Release` synchronizes with the `Acquire`
                                    // load in `Recv::drop`. It is critical to set
                                    // this **after** the waker is extracted,
                                    // otherwise we may data race with `Recv::drop`.
                                    queued.store(false, Release);
                                }
                            }
                            // This shard is drained; release the lock and wake the
                            // last batch before moving on to the next shard.
                            None => {
                                drop(shard);
                                wakers.wake_all();
                                break 'shard;
                            }
                        }
                    }
                    // The `WakeList` filled up; the shard lock is released at the
                    // end of this block.
                }

                // Wake outside of the shard lock (waking may run arbitrary code),
                // then keep draining this shard.
                wakers.wake_all();
            }
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        let shared = self.shared.clone();
        shared.num_tx.fetch_add(1, Relaxed);

        Sender { shared }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if 1 == self.shared.num_tx.fetch_sub(1, AcqRel) {
            self.close_channel();
        }
    }
}

impl<T> WeakSender<T> {
    /// Tries to convert a `WeakSender` into a [`Sender`].
    ///
    /// This will return `Some` if there are other `Sender` instances alive and
    /// the channel wasn't previously dropped, otherwise `None` is returned.
    #[must_use]
    pub fn upgrade(&self) -> Option<Sender<T>> {
        let mut tx_count = self.shared.num_tx.load(Acquire);

        loop {
            if tx_count == 0 {
                // channel is closed so this WeakSender can not be upgraded
                return None;
            }

            match self
                .shared
                .num_tx
                .compare_exchange_weak(tx_count, tx_count + 1, Relaxed, Acquire)
            {
                Ok(_) => {
                    return Some(Sender {
                        shared: self.shared.clone(),
                    })
                }
                Err(prev_count) => tx_count = prev_count,
            }
        }
    }

    /// Returns the number of [`Sender`] handles.
    pub fn strong_count(&self) -> usize {
        self.shared.num_tx.load(Acquire)
    }

    /// Returns the number of [`WeakSender`] handles.
    pub fn weak_count(&self) -> usize {
        self.shared.num_weak_tx.load(Acquire)
    }
}

impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> WeakSender<T> {
        let shared = self.shared.clone();
        shared.num_weak_tx.fetch_add(1, Relaxed);

        Self { shared }
    }
}

impl<T> Drop for WeakSender<T> {
    fn drop(&mut self) {
        self.shared.num_weak_tx.fetch_sub(1, AcqRel);
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel(16);
    ///
    /// tx.send(10).unwrap();
    /// tx.send(20).unwrap();
    ///
    /// assert_eq!(rx1.len(), 2);
    /// assert_eq!(rx1.recv().await.unwrap(), 10);
    /// assert_eq!(rx1.len(), 1);
    /// assert_eq!(rx1.recv().await.unwrap(), 20);
    /// assert_eq!(rx1.len(), 0);
    /// # }
    /// ```
    pub fn len(&self) -> usize {
        let next_send_pos = self.shared.tail.lock().pos;
        (next_send_pos - self.next) as usize
    }

    /// Returns true if there aren't any messages in the channel that the [`Receiver`]
    /// has yet to receive.
    ///
    /// [`Receiver`]: crate::sync::broadcast::Receiver
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel(16);
    ///
    /// assert!(rx1.is_empty());
    ///
    /// tx.send(10).unwrap();
    /// tx.send(20).unwrap();
    ///
    /// assert!(!rx1.is_empty());
    /// assert_eq!(rx1.recv().await.unwrap(), 10);
    /// assert_eq!(rx1.recv().await.unwrap(), 20);
    /// assert!(rx1.is_empty());
    /// # }
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, rx) = broadcast::channel::<()>(16);
    /// let rx2 = tx.subscribe();
    ///
    /// assert!(rx.same_channel(&rx2));
    ///
    /// let (_tx3, rx3) = broadcast::channel::<()>(16);
    ///
    /// assert!(!rx3.same_channel(&rx2));
    /// # }
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

        // Set to `true` once this receiver has enqueued its waiter, so the loop
        // re-checks the slot after enqueueing instead of enqueueing again.
        let mut enqueued = false;

        loop {
            // The slot holding the next value to read.
            let slot = self.shared.buffer[idx].lock();

            if slot.pos == self.next {
                // The value is available.
                self.next = self.next.wrapping_add(1);
                return Ok(RecvGuard { slot });
            }

            let next_pos = slot.pos.wrapping_add(self.shared.buffer.len() as u64);

            if next_pos == self.next {
                // The channel is empty for *this* receiver: it is waiting for the
                // next value to be written to this slot.
                //
                // `closed` is read with an atomic load instead of under the
                // `tail` lock, so that parking does not contend on `tail`.
                if self.shared.closed.load(Acquire) {
                    return Err(TryRecvError::Closed);
                }

                let (waiter, waker) = match waiter {
                    Some(waiter) => waiter,
                    // `try_recv` (and the drain in `Receiver::drop`) cannot park.
                    None => return Err(TryRecvError::Empty),
                };

                if enqueued {
                    // Second pass: the waiter is enqueued and the slot is still
                    // empty, so we are genuinely parked. If a sender writes our
                    // value after our enqueue, its drain wakes us (the enqueue is
                    // ordered before the drain); if it already popped us, that pop
                    // also woke us, so we will be polled again. Either way no
                    // wakeup is lost.
                    return Err(TryRecvError::Empty);
                }

                // First pass: enqueue the waiter without holding `tail`, then loop
                // to re-check the slot. Releasing the slot lock before enqueueing
                // lets a concurrent `send` write the slot; the re-check then
                // observes the value (and returns it), and if it does not, our
                // enqueue happened-before that send's drain, so we are woken.
                drop(slot);

                let mut old_waker = None;
                // Safety: the shard lock taken below is the synchronization edge
                // for `waker`/`queued` shared with `notify_rx` and `Recv::drop`.
                unsafe {
                    waiter.with_mut(|ptr| {
                        let node = NonNull::new_unchecked(ptr);
                        let shard = self.shared.waiters.lock_shard(&node);

                        // If there is no waker, or the stored waker is for a
                        // different task, store the current task's waker.
                        match (*ptr).waker {
                            Some(ref w) if w.will_wake(waker) => {}
                            _ => {
                                old_waker = (*ptr).waker.replace(waker.clone());
                            }
                        }

                        // Enqueue into this waiter's shard if not already queued.
                        // `Relaxed` suffices: the shard lock is the
                        // synchronization edge with `notify_rx`/`Recv::drop`.
                        if !(*ptr).queued.load(Relaxed) {
                            (*ptr).queued.store(true, Relaxed);
                            shard.push(node);
                        } else {
                            drop(shard);
                        }
                    });
                }
                // Drop the previous waker now that the shard lock is released.
                drop(old_waker);

                enqueued = true;
                continue;
            }

            // The receiver has lagged behind the sender by more than the channel
            // capacity. This is the slow path: take the `tail` lock to read the
            // write position and catch up to the oldest retained message.
            drop(slot);
            let tail = self.shared.tail.lock();

            // Re-acquire the slot lock and re-check: the buffer may have wrapped
            // between dropping and re-acquiring the slot lock.
            let slot = self.shared.buffer[idx].lock();
            if slot.pos == self.next {
                drop(tail);
                self.next = self.next.wrapping_add(1);
                return Ok(RecvGuard { slot });
            }

            let next_pos = slot.pos.wrapping_add(self.shared.buffer.len() as u64);
            if next_pos == self.next {
                // It became empty under `tail` (a wrap-around). Release both locks
                // and retry the lock-free empty path at the top of the loop.
                drop(slot);
                drop(tail);
                continue;
            }

            let next = tail.pos.wrapping_sub(self.shared.buffer.len() as u64);
            let missed = next.wrapping_sub(self.next);
            drop(tail);

            // The receiver is slow but no values have actually been missed.
            if missed == 0 {
                self.next = self.next.wrapping_add(1);
                return Ok(RecvGuard { slot });
            }

            self.next = next;
            return Err(TryRecvError::Lagged(missed));
        }
    }

    /// Returns the number of [`Sender`] handles.
    pub fn sender_strong_count(&self) -> usize {
        self.shared.num_tx.load(Acquire)
    }

    /// Returns the number of [`WeakSender`] handles.
    pub fn sender_weak_count(&self) -> usize {
        self.shared.num_weak_tx.load(Acquire)
    }

    /// Checks if a channel is closed.
    ///
    /// This method returns `true` if the channel has been closed. The channel is closed
    /// when all [`Sender`] have been dropped.
    ///
    /// [`Sender`]: crate::sync::broadcast::Sender
    ///
    /// # Examples
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, rx) = broadcast::channel::<()>(10);
    /// assert!(!rx.is_closed());
    ///
    /// drop(tx);
    ///
    /// assert!(rx.is_closed());
    /// # }
    /// ```
    pub fn is_closed(&self) -> bool {
        // Channel is closed when there are no strong senders left active
        self.shared.num_tx.load(Acquire) == 0
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx) = broadcast::channel(2);
    ///
    /// tx.send(1).unwrap();
    /// let mut rx2 = rx.resubscribe();
    /// tx.send(2).unwrap();
    ///
    /// assert_eq!(rx2.recv().await.unwrap(), 2);
    /// assert_eq!(rx.recv().await.unwrap(), 1);
    /// # }
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx1) = broadcast::channel(16);
    /// let mut rx2 = tx.subscribe();
    ///
    /// tokio::spawn(async move {
    ///     assert_eq!(rx1.recv().await.unwrap(), 10);
    ///     assert_eq!(rx1.recv().await.unwrap(), 20);
    /// });
    ///
    /// tokio::spawn(async move {
    ///     assert_eq!(rx2.recv().await.unwrap(), 10);
    ///     assert_eq!(rx2.recv().await.unwrap(), 20);
    /// });
    ///
    /// tx.send(10).unwrap();
    /// tx.send(20).unwrap();
    /// # }
    /// ```
    ///
    /// Handling lag
    ///
    /// ```
    /// use tokio::sync::broadcast;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx) = broadcast::channel(2);
    ///
    /// tx.send(10).unwrap();
    /// tx.send(20).unwrap();
    /// tx.send(30).unwrap();
    ///
    /// // The receiver lagged behind
    /// assert!(rx.recv().await.is_err());
    ///
    /// // At this point, we can abort or continue with lost messages
    ///
    /// assert_eq!(20, rx.recv().await.unwrap());
    /// assert_eq!(30, rx.recv().await.unwrap());
    /// # }
    /// ```
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        cooperative(Recv::new(self)).await
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let (tx, mut rx) = broadcast::channel(16);
    ///
    /// assert!(rx.try_recv().is_err());
    ///
    /// tx.send(10).unwrap();
    ///
    /// let value = rx.try_recv().unwrap();
    /// assert_eq!(10, value);
    /// # }
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
    /// # #[cfg(not(target_family = "wasm"))]
    /// # {
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
    /// # }
    /// ```
    pub fn blocking_recv(&mut self) -> Result<T, RecvError> {
        crate::future::block_on(self.recv())
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut tail = self.shared.tail.lock();

        tail.rx_cnt -= 1;
        let until = tail.pos;
        let remaining_rx = tail.rx_cnt;

        if remaining_rx == 0 {
            self.shared.notify_last_rx_drop.notify_waiters();
            self.shared.closed.store(true, Release);
        }

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
        // Under loom, assign the shard up front from a per-channel counter (node
        // addresses are not stable across loom executions). The value is fixed
        // for the life of the waiter, so it stays consistent between the push in
        // `recv_ref` and the removal in `Recv::drop`.
        #[cfg(loom)]
        let shard = receiver
            .shared
            .next_shard
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % WAITER_SHARDS;

        Recv {
            receiver,
            waiter: WaiterCell(UnsafeCell::new(Waiter {
                queued: AtomicBool::new(false),
                waker: None,
                pointers: linked_list::Pointers::new(),
                #[cfg(loom)]
                shard,
                _p: PhantomPinned,
            })),
        }
    }

    /// A custom `project` implementation is used in place of `pin-project-lite`
    /// as a custom drop implementation is needed.
    fn project(self: Pin<&mut Self>) -> (&mut Receiver<T>, &UnsafeCell<Waiter>) {
        unsafe {
            // Safety: Receiver is Unpin
            is_unpin::<&mut Receiver<T>>();

            let me = self.get_unchecked_mut();
            (me.receiver, &me.waiter.0)
        }
    }
}

impl<'a, T> Future for Recv<'a, T>
where
    T: Clone,
{
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        ready!(crate::trace::trace_leaf());

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
        // Safety: `waiter.queued` is atomic.
        // Acquire ordering is required to synchronize with
        // `Shared::notify_rx` before we drop the object.
        let queued = self
            .waiter
            .0
            .with(|ptr| unsafe { (*ptr).queued.load(Acquire) });

        // If the waiter is queued, we need to unlink it from its shard. If not,
        // no further synchronization is required, since the waiter is not in any
        // list and, as such, is not shared with any other threads.
        if queued {
            self.waiter.0.with_mut(|ptr| {
                // Safety: `queued` was observed to be `true`, so the node is (or
                // recently was) in its shard. `remove` locks that shard and
                // unlinks the node if it is still present; if `notify_rx` already
                // removed it concurrently, `remove` returns `None` and does
                // nothing. The node's shard id is stable, so it cannot be in any
                // other shard.
                let node = unsafe { NonNull::new_unchecked(ptr) };
                let _ = unsafe { self.receiver.shared.waiters.remove(node) };
            });
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
        unsafe { Waiter::addr_of_pointers(target) }
    }
}

/// # Safety
///
/// A `Waiter` is pinned for as long as it is queued, so its address — and
/// therefore the shard id derived from it — does not change between being pushed
/// and being removed.
unsafe impl ShardedListItem for Waiter {
    unsafe fn get_shard_id(target: NonNull<Self::Target>) -> usize {
        // Under loom, pointer addresses are not stable across executions, so the
        // shard is assigned from a per-channel counter when the waiter is created
        // and read back here. This keeps the chosen shard deterministic for a
        // given execution, which loom requires.
        #[cfg(loom)]
        {
            // Safety: the caller guarantees `target` points at a valid waiter,
            // and `shard` is set at construction and never mutated afterwards.
            unsafe { (*target.as_ptr()).shard }
        }
        // Outside of loom, mix the node address with Fibonacci hashing so that
        // adjacently allocated (and thus aligned) waiters are spread across
        // shards instead of all landing in the same one. The high bits of the
        // product are the well-mixed ones; `ShardedList` masks the low bits, so
        // shift the high bits down. The arithmetic is done in `u64` so it is
        // correct on 32-bit targets too.
        #[cfg(not(loom))]
        {
            let addr = target.as_ptr() as usize as u64;
            (addr.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as usize
        }
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "broadcast::Sender")
    }
}

impl<T> fmt::Debug for WeakSender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "broadcast::WeakSender")
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
        self.slot.val.clone()
    }
}

impl<'a, T> Drop for RecvGuard<'a, T> {
    fn drop(&mut self) {
        // Decrement the remaining counter
        if 1 == self.slot.rem.fetch_sub(1, SeqCst) {
            self.slot.val = None;
        }
    }
}

fn is_unpin<T: Unpin>() {}

#[cfg(not(loom))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_count_on_sender_constructor() {
        let sender = Sender::<i32>::new(16);
        assert_eq!(sender.receiver_count(), 0);

        let rx_1 = sender.subscribe();
        assert_eq!(sender.receiver_count(), 1);

        let rx_2 = rx_1.resubscribe();
        assert_eq!(sender.receiver_count(), 2);

        let rx_3 = sender.subscribe();
        assert_eq!(sender.receiver_count(), 3);

        drop(rx_3);
        drop(rx_1);
        assert_eq!(sender.receiver_count(), 1);

        drop(rx_2);
        assert_eq!(sender.receiver_count(), 0);
    }

    #[cfg(not(loom))]
    #[test]
    fn receiver_count_on_channel_constructor() {
        let (sender, rx) = channel::<i32>(16);
        assert_eq!(sender.receiver_count(), 1);

        let _rx_2 = rx.resubscribe();
        assert_eq!(sender.receiver_count(), 2);
    }
}
