//! A multi-producer, multi-consumer broadcast queue. Each sent value is seen by
//! all consumers.
//!
//! A [`Sender`] is used to broadcast values to **all** connected [`Receiver`]
//! values. [`Sender`] handles are clone-able, allowing concurrent send and
//! receive actions. [`Sender`] and [`Receiver`] are both `Send` and `Sync` as
//! long as `T` is also `Send` or `Sync` respectively.
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
//! ## Lagging
//!
//! As sent messages must be retained until **all** [`Receiver`] handles receive
//! a clone, broadcast channels are suspectible to the "slow receiver" problem.
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
//! [`Sender`]: crate::sync::broadcast::Sender
//! [`Sender::subscribe`]: crate::sync::broadcast::Sender::subscribe
//! [`Receiver`]: crate::sync::broadcast::Receiver
//! [`channel`]: crate::sync::broadcast::channel
//! [`RecvError::Lagged`]: crate::sync::broadcast::RecvError::Lagged
//! [`RecvError::Closed`]: crate::sync::broadcast::RecvError::Closed
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

use crate::loom::cell::UnsafeCell;
use crate::loom::future::AtomicWaker;
use crate::loom::sync::atomic::{spin_loop_hint, AtomicBool, AtomicPtr, AtomicUsize};
use crate::loom::sync::{Arc, Condvar, Mutex};

use std::fmt;
use std::mem;
use std::ptr;
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

    /// Waiter state
    wait: Arc<WaitNode>,
}

/// Error returned by [`Sender::send`][Sender::send].
///
/// A **send** operation can only fail if there are no active receivers,
/// implying that the message could never be received. The error contains the
/// message being sent as a payload so it can be recovered.
#[derive(Debug)]
pub struct SendError<T>(pub T);

/// An error returned from the [`recv`] function on a [`Receiver`].
///
/// [`recv`]: crate::sync::broadcast::Receiver::recv
/// [`Receiver`]: crate::sync::broadcast::Receiver
#[derive(Debug, PartialEq)]
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

/// An error returned from the [`try_recv`] function on a [`Receiver`].
///
/// [`try_recv`]: crate::sync::broadcast::Receiver::try_recv
/// [`Receiver`]: crate::sync::broadcast::Receiver
#[derive(Debug, PartialEq)]
pub enum TryRecvError {
    /// The channel is currently empty. There are still active
    /// [`Sender`][Sender] handles, so data may yet become available.
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

/// Data shared between senders and receivers
struct Shared<T> {
    /// slots in the channel
    buffer: Box<[Slot<T>]>,

    /// Mask a position -> index
    mask: usize,

    /// Tail of the queue
    tail: Mutex<Tail>,

    /// Notifies a sender that the slot is unlocked
    condvar: Condvar,

    /// Stack of pending waiters
    wait_stack: AtomicPtr<WaitNode>,

    /// Number of outstanding Sender handles
    num_tx: AtomicUsize,
}

/// Next position to write a value
struct Tail {
    /// Next position to write to
    pos: u64,

    /// Number of active receivers
    rx_cnt: usize,
}

/// Slot in the buffer
struct Slot<T> {
    /// Remaining number of receivers that are expected to see this value.
    ///
    /// When this goes to zero, the value is released.
    rem: AtomicUsize,

    /// Used to lock the `write` field.
    lock: AtomicUsize,

    /// The value being broadcast
    ///
    /// Synchronized by `state`
    write: Write<T>,
}

/// A write in the buffer
struct Write<T> {
    /// Uniquely identifies this write
    pos: UnsafeCell<u64>,

    /// The written value
    val: UnsafeCell<Option<T>>,
}

/// Tracks a waiting receiver
#[derive(Debug)]
struct WaitNode {
    /// `true` if queued
    queued: AtomicBool,

    /// Task to wake when a permit is made available.
    waker: AtomicWaker,

    /// Next pointer in the stack of waiting senders.
    next: UnsafeCell<*const WaitNode>,
}

struct RecvGuard<'a, T> {
    slot: &'a Slot<T>,
    tail: &'a Mutex<Tail>,
    condvar: &'a Condvar,
}

/// Max number of receivers. Reserve space to lock.
const MAX_RECEIVERS: usize = usize::MAX >> 1;

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
/// [`SendError`]: crate::sync::broadcast::SendError
/// [`RecvError`]: crate::sync::broadcast::RecvError
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
pub fn channel<T>(mut capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "capacity is empty");
    assert!(capacity <= usize::MAX >> 1, "requested capacity too large");

    // Round to a power of two
    capacity = capacity.next_power_of_two();

    let mut buffer = Vec::with_capacity(capacity);

    for i in 0..capacity {
        buffer.push(Slot {
            rem: AtomicUsize::new(0),
            lock: AtomicUsize::new(0),
            write: Write {
                pos: UnsafeCell::new((i as u64).wrapping_sub(capacity as u64)),
                val: UnsafeCell::new(None),
            },
        });
    }

    let shared = Arc::new(Shared {
        buffer: buffer.into_boxed_slice(),
        mask: capacity - 1,
        tail: Mutex::new(Tail { pos: 0, rx_cnt: 1 }),
        condvar: Condvar::new(),
        wait_stack: AtomicPtr::new(ptr::null_mut()),
        num_tx: AtomicUsize::new(1),
    });

    let rx = Receiver {
        shared: shared.clone(),
        next: 0,
        wait: Arc::new(WaitNode {
            queued: AtomicBool::new(false),
            waker: AtomicWaker::new(),
            next: UnsafeCell::new(ptr::null()),
        }),
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
        self.send2(Some(value))
            .map_err(|SendError(maybe_v)| SendError(maybe_v.unwrap()))
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

        let mut tail = shared.tail.lock().unwrap();

        if tail.rx_cnt == MAX_RECEIVERS {
            panic!("max receivers");
        }

        tail.rx_cnt = tail.rx_cnt.checked_add(1).expect("overflow");
        let next = tail.pos;

        drop(tail);

        Receiver {
            shared,
            next,
            wait: Arc::new(WaitNode {
                queued: AtomicBool::new(false),
                waker: AtomicWaker::new(),
                next: UnsafeCell::new(ptr::null()),
            }),
        }
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
        let tail = self.shared.tail.lock().unwrap();
        tail.rx_cnt
    }

    fn send2(&self, value: Option<T>) -> Result<usize, SendError<Option<T>>> {
        let mut tail = self.shared.tail.lock().unwrap();

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
        let slot = &self.shared.buffer[idx];

        // Acquire the write lock
        let mut prev = slot.lock.fetch_or(1, SeqCst);

        while prev & !1 != 0 {
            // Concurrent readers, we must go to sleep
            tail = self.shared.condvar.wait(tail).unwrap();

            prev = slot.lock.load(SeqCst);

            if prev & 1 == 0 {
                // The writer lock bit was cleared while this thread was
                // sleeping. This can only happen if a newer write happened on
                // this slot by another thread. Bail early as an optimization,
                // there is nothing left to do.
                return Ok(rem);
            }
        }

        if tail.pos.wrapping_sub(pos) > self.shared.buffer.len() as u64 {
            // There is a newer pending write to the same slot.
            return Ok(rem);
        }

        // Slot lock acquired
        slot.write.pos.with_mut(|ptr| unsafe { *ptr = pos });
        slot.write.val.with_mut(|ptr| unsafe { *ptr = value });

        // Set remaining receivers
        slot.rem.store(rem, SeqCst);

        // Release the slot lock
        slot.lock.store(0, SeqCst);

        // Release the mutex. This must happen after the slot lock is released,
        // otherwise the writer lock bit could be cleared while another thread
        // is in the critical section.
        drop(tail);

        // Notify waiting receivers
        self.notify_rx();

        Ok(rem)
    }

    fn notify_rx(&self) {
        let mut curr = self.shared.wait_stack.swap(ptr::null_mut(), SeqCst) as *const WaitNode;

        while !curr.is_null() {
            let waiter = unsafe { Arc::from_raw(curr) };

            // Update `curr` before toggling `queued` and waking
            curr = waiter.next.with(|ptr| unsafe { *ptr });

            // Unset queued
            waiter.queued.store(false, SeqCst);

            // Wake
            waiter.waker.wake();
        }
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
            let _ = self.send2(None);
        }
    }
}

impl<T> Receiver<T> {
    /// Locks the next value if there is one.
    ///
    /// The caller is responsible for unlocking
    fn recv_ref(&mut self, spin: bool) -> Result<RecvGuard<'_, T>, TryRecvError> {
        let idx = (self.next & self.shared.mask as u64) as usize;

        // The slot holding the next value to read
        let slot = &self.shared.buffer[idx];

        // Lock the slot
        if !slot.try_rx_lock() {
            if spin {
                while !slot.try_rx_lock() {
                    spin_loop_hint();
                }
            } else {
                return Err(TryRecvError::Empty);
            }
        }

        let guard = RecvGuard {
            slot,
            tail: &self.shared.tail,
            condvar: &self.shared.condvar,
        };

        if guard.pos() != self.next {
            let pos = guard.pos();

            guard.drop_no_rem_dec();

            if pos.wrapping_add(self.shared.buffer.len() as u64) == self.next {
                return Err(TryRecvError::Empty);
            } else {
                let tail = self.shared.tail.lock().unwrap();

                // `tail.pos` points to the slot the **next** send writes to.
                // Because a receiver is lagging, this slot also holds the
                // oldest value. To make the positions match, we subtract the
                // capacity.
                let next = tail.pos.wrapping_sub(self.shared.buffer.len() as u64);
                let missed = next.wrapping_sub(self.next);

                self.next = next;

                return Err(TryRecvError::Lagged(missed));
            }
        }

        self.next = self.next.wrapping_add(1);

        Ok(guard)
    }
}

impl<T> Receiver<T>
where
    T: Clone,
{
    /// Attempts to return a pending value on this receiver without awaiting.
    ///
    /// This is useful for a flavor of "optimistic check" before deciding to
    /// await on a receiver.
    ///
    /// Compared with [`recv`], this function has three failure cases instead of one
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
        let guard = self.recv_ref(false)?;
        guard.clone_value().ok_or(TryRecvError::Closed)
    }

    #[doc(hidden)] // TODO: document
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Result<T, RecvError>> {
        if let Some(value) = ok_empty(self.try_recv())? {
            return Poll::Ready(Ok(value));
        }

        self.register_waker(cx.waker());

        if let Some(value) = ok_empty(self.try_recv())? {
            Poll::Ready(Ok(value))
        } else {
            Poll::Pending
        }
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
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        use crate::future::poll_fn;

        poll_fn(|cx| self.poll_recv(cx)).await
    }

    fn register_waker(&self, cx: &Waker) {
        self.wait.waker.register_by_ref(cx);

        if !self.wait.queued.load(SeqCst) {
            // Set `queued` before queuing.
            self.wait.queued.store(true, SeqCst);

            let mut curr = self.shared.wait_stack.load(SeqCst);

            // The ref count is decremented in `notify_rx` when all nodes are
            // removed from the waiter stack.
            let node = Arc::into_raw(self.wait.clone()) as *mut _;

            loop {
                // Safety: `queued == false` means the caller has exclusive
                // access to `self.wait.next`.
                self.wait.next.with_mut(|ptr| unsafe { *ptr = curr });

                let res = self
                    .shared
                    .wait_stack
                    .compare_exchange(curr, node, SeqCst, SeqCst);

                match res {
                    Ok(_) => return,
                    Err(actual) => curr = actual,
                }
            }
        }
    }
}

#[cfg(feature = "stream")]
impl<T> crate::stream::Stream for Receiver<T>
where
    T: Clone,
{
    type Item = Result<T, RecvError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<T, RecvError>>> {
        self.poll_recv(cx).map(|v| match v {
            Ok(v) => Some(Ok(v)),
            lag @ Err(RecvError::Lagged(_)) => Some(lag),
            Err(RecvError::Closed) => None,
        })
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut tail = self.shared.tail.lock().unwrap();

        tail.rx_cnt -= 1;
        let until = tail.pos;

        drop(tail);

        while self.next != until {
            match self.recv_ref(true) {
                // Ignore the value
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

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        // Clear the wait stack
        let mut curr = self.wait_stack.with_mut(|ptr| *ptr as *const WaitNode);

        while !curr.is_null() {
            let waiter = unsafe { Arc::from_raw(curr) };
            curr = waiter.next.with(|ptr| unsafe { *ptr });
        }
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

impl<T> Slot<T> {
    /// Tries to lock the slot for a receiver. If `false`, then a sender holds the
    /// lock and the calling task will be notified once the sender has released
    /// the lock.
    fn try_rx_lock(&self) -> bool {
        let mut curr = self.lock.load(SeqCst);

        loop {
            if curr & 1 == 1 {
                // Locked by sender
                return false;
            }

            // Only increment (by 2) if the LSB "lock" bit is not set.
            let res = self.lock.compare_exchange(curr, curr + 2, SeqCst, SeqCst);

            match res {
                Ok(_) => return true,
                Err(actual) => curr = actual,
            }
        }
    }

    fn rx_unlock(&self, tail: &Mutex<Tail>, condvar: &Condvar, rem_dec: bool) {
        if rem_dec {
            // Decrement the remaining counter
            if 1 == self.rem.fetch_sub(1, SeqCst) {
                // Last receiver, drop the value
                self.write.val.with_mut(|ptr| unsafe { *ptr = None });
            }
        }

        if 1 == self.lock.fetch_sub(2, SeqCst) - 2 {
            // First acquire the lock to make sure our sender is waiting on the
            // condition variable, otherwise the notification could be lost.
            mem::drop(tail.lock().unwrap());
            // Wake up senders
            condvar.notify_all();
        }
    }
}

impl<'a, T> RecvGuard<'a, T> {
    fn pos(&self) -> u64 {
        self.slot.write.pos.with(|ptr| unsafe { *ptr })
    }

    fn clone_value(&self) -> Option<T>
    where
        T: Clone,
    {
        self.slot.write.val.with(|ptr| unsafe { (*ptr).clone() })
    }

    fn drop_no_rem_dec(self) {
        self.slot.rx_unlock(self.tail, self.condvar, false);

        mem::forget(self);
    }
}

impl<'a, T> Drop for RecvGuard<'a, T> {
    fn drop(&mut self) {
        self.slot.rx_unlock(self.tail, self.condvar, true)
    }
}

fn ok_empty<T>(res: Result<T, TryRecvError>) -> Result<Option<T>, RecvError> {
    match res {
        Ok(value) => Ok(Some(value)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Lagged(n)) => Err(RecvError::Lagged(n)),
        Err(TryRecvError::Closed) => Err(RecvError::Closed),
    }
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
