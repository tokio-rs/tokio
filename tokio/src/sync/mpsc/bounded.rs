use crate::sync::mpsc::chan;
use crate::sync::mpsc::error::{ClosedError, SendError, TryRecvError, TrySendError};
use crate::sync::semaphore_ll as semaphore;

cfg_time! {
    use crate::sync::mpsc::error::SendTimeoutError;
    use crate::time::Duration;
}

use std::fmt;
use std::task::{Context, Poll};

/// Send values to the associated `Receiver`.
///
/// Instances are created by the [`channel`](channel) function.
pub struct Sender<T> {
    chan: chan::Tx<T, Semaphore>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            chan: self.chan.clone(),
        }
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Sender")
            .field("chan", &self.chan)
            .finish()
    }
}

/// Receive values from the associated `Sender`.
///
/// Instances are created by the [`channel`](channel) function.
pub struct Receiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Receiver")
            .field("chan", &self.chan)
            .finish()
    }
}

/// Creates a bounded mpsc channel for communicating between asynchronous tasks,
/// returning the sender/receiver halves.
///
/// All data sent on `Sender` will become available on `Receiver` in the same
/// order as it was sent.
///
/// The `Sender` can be cloned to `send` to the same channel from multiple code
/// locations. Only one `Receiver` is supported.
///
/// If the `Receiver` is disconnected while trying to `send`, the `send` method
/// will return a `SendError`. Similarly, if `Sender` is disconnected while
/// trying to `recv`, the `recv` method will return a `RecvError`.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///     let (mut tx, mut rx) = mpsc::channel(100);
///
///     tokio::spawn(async move {
///         for i in 0..10 {
///             if let Err(_) = tx.send(i).await {
///                 println!("receiver dropped");
///                 return;
///             }
///         }
///     });
///
///     while let Some(i) = rx.recv().await {
///         println!("got = {}", i);
///     }
/// }
/// ```
pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    assert!(buffer > 0, "mpsc bounded channel requires buffer > 0");
    let semaphore = (semaphore::Semaphore::new(buffer), buffer);
    let (tx, rx) = chan::channel(semaphore);

    let tx = Sender::new(tx);
    let rx = Receiver::new(rx);

    (tx, rx)
}

/// Channel semaphore is a tuple of the semaphore implementation and a `usize`
/// representing the channel bound.
type Semaphore = (semaphore::Semaphore, usize);

impl<T> Receiver<T> {
    pub(crate) fn new(chan: chan::Rx<T, Semaphore>) -> Receiver<T> {
        Receiver { chan }
    }

    /// Receives the next value for this receiver.
    ///
    /// `None` is returned when all `Sender` halves have dropped, indicating
    /// that no further values can be sent on the channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, mut rx) = mpsc::channel(100);
    ///
    ///     tokio::spawn(async move {
    ///         tx.send("hello").await.unwrap();
    ///     });
    ///
    ///     assert_eq!(Some("hello"), rx.recv().await);
    ///     assert_eq!(None, rx.recv().await);
    /// }
    /// ```
    ///
    /// Values are buffered:
    ///
    /// ```
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, mut rx) = mpsc::channel(100);
    ///
    ///     tx.send("hello").await.unwrap();
    ///     tx.send("world").await.unwrap();
    ///
    ///     assert_eq!(Some("hello"), rx.recv().await);
    ///     assert_eq!(Some("world"), rx.recv().await);
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<T> {
        use crate::future::poll_fn;

        poll_fn(|cx| self.poll_recv(cx)).await
    }

    #[doc(hidden)] // TODO: document
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.chan.recv(cx)
    }

    /// Attempts to return a pending value on this receiver without blocking.
    ///
    /// This method will never block the caller in order to wait for data to
    /// become available. Instead, this will always return immediately with
    /// a possible option of pending data on the channel.
    ///
    /// This is useful for a flavor of "optimistic check" before deciding to
    /// block on a receiver.
    ///
    /// Compared with recv, this function has two failure cases instead of
    /// one (one for disconnection, one for an empty buffer).
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.chan.try_recv()
    }

    /// Closes the receiving half of a channel, without dropping it.
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.chan.close();
    }
}

impl<T> Unpin for Receiver<T> {}

cfg_stream! {
    impl<T> crate::stream::Stream for Receiver<T> {
        type Item = T;

        fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
            self.poll_recv(cx)
        }
    }
}

impl<T> Sender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> Sender<T> {
        Sender { chan }
    }

    /// Sends a value, waiting until there is capacity.
    ///
    /// A successful send occurs when it is determined that the other end of the
    /// channel has not hung up already. An unsuccessful send would be one where
    /// the corresponding receiver has already been closed. Note that a return
    /// value of `Err` means that the data will never be received, but a return
    /// value of `Ok` does not mean that the data will be received. It is
    /// possible for the corresponding receiver to hang up immediately after
    /// this function returns `Ok`.
    ///
    /// # Errors
    ///
    /// If the receive half of the channel is closed, either due to [`close`]
    /// being called or the [`Receiver`] handle dropping, the function returns
    /// an error. The error includes the value passed to `send`.
    ///
    /// [`close`]: Receiver::close
    /// [`Receiver`]: Receiver
    ///
    /// # Examples
    ///
    /// In the following example, each call to `send` will block until the
    /// previously sent value was received.
    ///
    /// ```rust
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, mut rx) = mpsc::channel(1);
    ///
    ///     tokio::spawn(async move {
    ///         for i in 0..10 {
    ///             if let Err(_) = tx.send(i).await {
    ///                 println!("receiver dropped");
    ///                 return;
    ///             }
    ///         }
    ///     });
    ///
    ///     while let Some(i) = rx.recv().await {
    ///         println!("got = {}", i);
    ///     }
    /// }
    /// ```
    pub async fn send(&mut self, value: T) -> Result<(), SendError<T>> {
        use crate::future::poll_fn;

        if poll_fn(|cx| self.poll_ready(cx)).await.is_err() {
            return Err(SendError(value));
        }

        match self.try_send(value) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => unreachable!(),
            Err(TrySendError::Closed(value)) => Err(SendError(value)),
        }
    }

    /// Attempts to immediately send a message on this `Sender`
    ///
    /// This method differs from [`send`] by returning immediately if the channel's
    /// buffer is full or no receiver is waiting to acquire some data. Compared
    /// with [`send`], this function has two failure cases instead of one (one for
    /// disconnection, one for a full buffer).
    ///
    /// This function may be paired with [`poll_ready`] in order to wait for
    /// channel capacity before trying to send a value.
    ///
    /// # Errors
    ///
    /// If the channel capacity has been reached, i.e., the channel has `n`
    /// buffered values where `n` is the argument passed to [`channel`], then an
    /// error is returned.
    ///
    /// If the receive half of the channel is closed, either due to [`close`]
    /// being called or the [`Receiver`] handle dropping, the function returns
    /// an error. The error includes the value passed to `send`.
    ///
    /// [`send`]: Sender::send
    /// [`poll_ready`]: Sender::poll_ready
    /// [`channel`]: channel
    /// [`close`]: Receiver::close
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Create a channel with buffer size 1
    ///     let (mut tx1, mut rx) = mpsc::channel(1);
    ///     let mut tx2 = tx1.clone();
    ///
    ///     tokio::spawn(async move {
    ///         tx1.send(1).await.unwrap();
    ///         tx1.send(2).await.unwrap();
    ///         // task waits until the receiver receives a value.
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         // This will return an error and send
    ///         // no message if the buffer is full
    ///         let _ = tx2.try_send(3);
    ///     });
    ///
    ///     let mut msg;
    ///     msg = rx.recv().await.unwrap();
    ///     println!("message {} received", msg);
    ///
    ///     msg = rx.recv().await.unwrap();
    ///     println!("message {} received", msg);
    ///
    ///     // Third message may have never been sent
    ///     match rx.recv().await {
    ///         Some(msg) => println!("message {} received", msg),
    ///         None => println!("the third message was never sent"),
    ///     }
    /// }
    /// ```
    pub fn try_send(&mut self, message: T) -> Result<(), TrySendError<T>> {
        self.chan.try_send(message)?;
        Ok(())
    }

    /// Sends a value, waiting until there is capacity, but only for a limited time.
    ///
    /// Shares the same success and error conditions as [`send`], adding one more
    /// condition for an unsuccessful send, which is when the provided timeout has
    /// elapsed, and there is no capacity available.
    ///
    /// [`send`]: Sender::send
    ///
    /// # Errors
    ///
    /// If the receive half of the channel is closed, either due to [`close`]
    /// being called or the [`Receiver`] having been dropped,
    /// the function returns an error. The error includes the value passed to `send`.
    ///
    /// [`close`]: Receiver::close
    /// [`Receiver`]: Receiver
    ///
    /// # Examples
    ///
    /// In the following example, each call to `send_timeout` will block until the
    /// previously sent value was received, unless the timeout has elapsed.
    ///
    /// ```rust
    /// use tokio::sync::mpsc;
    /// use tokio::time::{delay_for, Duration};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (mut tx, mut rx) = mpsc::channel(1);
    ///
    ///     tokio::spawn(async move {
    ///         for i in 0..10 {
    ///             if let Err(e) = tx.send_timeout(i, Duration::from_millis(100)).await {
    ///                 println!("send error: #{:?}", e);
    ///                 return;
    ///             }
    ///         }
    ///     });
    ///
    ///     while let Some(i) = rx.recv().await {
    ///         println!("got = {}", i);
    ///         delay_for(Duration::from_millis(200)).await;
    ///     }
    /// }
    /// ```
    #[cfg(feature = "time")]
    #[cfg_attr(docsrs, doc(cfg(feature = "time")))]
    pub async fn send_timeout(
        &mut self,
        value: T,
        timeout: Duration,
    ) -> Result<(), SendTimeoutError<T>> {
        use crate::future::poll_fn;

        match crate::time::timeout(timeout, poll_fn(|cx| self.poll_ready(cx))).await {
            Err(_) => {
                return Err(SendTimeoutError::Timeout(value));
            }
            Ok(Err(_)) => {
                return Err(SendTimeoutError::Closed(value));
            }
            Ok(_) => {}
        }

        match self.try_send(value) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => unreachable!(),
            Err(TrySendError::Closed(value)) => Err(SendTimeoutError::Closed(value)),
        }
    }

    /// Returns `Poll::Ready(Ok(()))` when the channel is able to accept another item.
    ///
    /// If the channel is full, then `Poll::Pending` is returned and the task is notified when a
    /// slot becomes available.
    ///
    /// Once `poll_ready` returns `Poll::Ready(Ok(()))`, a call to `try_send` will succeed unless
    /// the channel has since been closed. To provide this guarantee, the channel reserves one slot
    /// in the channel for the coming send. This reserved slot is not available to other `Sender`
    /// instances, so you need to be careful to not end up with deadlocks by blocking after calling
    /// `poll_ready` but before sending an element.
    ///
    /// If, after `poll_ready` succeeds, you decide you do not wish to send an item after all, you
    /// can use [`disarm`](Sender::disarm) to release the reserved slot.
    ///
    /// Until an item is sent or [`disarm`](Sender::disarm) is called, repeated calls to
    /// `poll_ready` will return either `Poll::Ready(Ok(()))` or `Poll::Ready(Err(_))` if channel
    /// is closed.
    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), ClosedError>> {
        self.chan.poll_ready(cx).map_err(|_| ClosedError::new())
    }

    /// Undo a successful call to `poll_ready`.
    ///
    /// Once a call to `poll_ready` returns `Poll::Ready(Ok(()))`, it holds up one slot in the
    /// channel to make room for the coming send. `disarm` allows you to give up that slot if you
    /// decide you do not wish to send an item after all. After calling `disarm`, you must call
    /// `poll_ready` until it returns `Poll::Ready(Ok(()))` before attempting to send again.
    ///
    /// Returns `false` if no slot is reserved for this sender (usually because `poll_ready` was
    /// not previously called, or did not succeed).
    ///
    /// # Motivation
    ///
    /// Since `poll_ready` takes up one of the finite number of slots in a bounded channel, callers
    /// need to send an item shortly after `poll_ready` succeeds. If they do not, idle senders may
    /// take up all the slots of the channel, and prevent active senders from getting any requests
    /// through. Consider this code that forwards from one channel to another:
    ///
    /// ```rust,ignore
    /// loop {
    ///   ready!(tx.poll_ready(cx))?;
    ///   if let Some(item) = ready!(rx.poll_recv(cx)) {
    ///     tx.try_send(item)?;
    ///   } else {
    ///     break;
    ///   }
    /// }
    /// ```
    ///
    /// If many such forwarders exist, and they all forward into a single (cloned) `Sender`, then
    /// any number of forwarders may be waiting for `rx.poll_recv` at the same time. While they do,
    /// they are effectively each reducing the channel's capacity by 1. If enough of these
    /// forwarders are idle, forwarders whose `rx` _do_ have elements will be unable to find a spot
    /// for them through `poll_ready`, and the system will deadlock.
    ///
    /// `disarm` solves this problem by allowing you to give up the reserved slot if you find that
    /// you have to block. We can then fix the code above by writing:
    ///
    /// ```rust,ignore
    /// loop {
    ///   ready!(tx.poll_ready(cx))?;
    ///   let item = rx.poll_recv(cx);
    ///   if let Poll::Ready(Ok(_)) = item {
    ///     // we're going to send the item below, so don't disarm
    ///   } else {
    ///     // give up our send slot, we won't need it for a while
    ///     tx.disarm();
    ///   }
    ///   if let Some(item) = ready!(item) {
    ///     tx.try_send(item)?;
    ///   } else {
    ///     break;
    ///   }
    /// }
    /// ```
    pub fn disarm(&mut self) -> bool {
        if self.chan.is_ready() {
            self.chan.disarm();
            true
        } else {
            false
        }
    }
}
