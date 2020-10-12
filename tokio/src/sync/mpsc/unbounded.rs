use crate::loom::sync::atomic::AtomicUsize;
use crate::sync::mpsc::chan;
use crate::sync::mpsc::error::{SendError, TryRecvError};

use std::fmt;
use std::task::{Context, Poll};

/// Send values to the associated `UnboundedReceiver`.
///
/// Instances are created by the
/// [`unbounded_channel`](unbounded_channel) function.
pub struct UnboundedSender<T> {
    chan: chan::Tx<T, Semaphore>,
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        UnboundedSender {
            chan: self.chan.clone(),
        }
    }
}

impl<T> fmt::Debug for UnboundedSender<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnboundedSender")
            .field("chan", &self.chan)
            .finish()
    }
}

/// Receive values from the associated `UnboundedSender`.
///
/// Instances are created by the
/// [`unbounded_channel`](unbounded_channel) function.
pub struct UnboundedReceiver<T> {
    /// The channel receiver
    chan: chan::Rx<T, Semaphore>,
}

impl<T> fmt::Debug for UnboundedReceiver<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UnboundedReceiver")
            .field("chan", &self.chan)
            .finish()
    }
}

/// Creates an unbounded mpsc channel for communicating between asynchronous
/// tasks without backpressure.
///
/// A `send` on this channel will always succeed as long as the receive half has
/// not been closed. If the receiver falls behind, messages will be arbitrarily
/// buffered.
///
/// **Note** that the amount of available system memory is an implicit bound to
/// the channel. Using an `unbounded` channel has the ability of causing the
/// process to run out of memory. In this case, the process will be aborted.
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = chan::channel(AtomicUsize::new(0));

    let tx = UnboundedSender::new(tx);
    let rx = UnboundedReceiver::new(rx);

    (tx, rx)
}

/// No capacity
type Semaphore = AtomicUsize;

impl<T> UnboundedReceiver<T> {
    pub(crate) fn new(chan: chan::Rx<T, Semaphore>) -> UnboundedReceiver<T> {
        UnboundedReceiver { chan }
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.chan.recv(cx)
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
    ///     let (tx, mut rx) = mpsc::unbounded_channel();
    ///
    ///     tokio::spawn(async move {
    ///         tx.send("hello").unwrap();
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
    ///     let (tx, mut rx) = mpsc::unbounded_channel();
    ///
    ///     tx.send("hello").unwrap();
    ///     tx.send("world").unwrap();
    ///
    ///     assert_eq!(Some("hello"), rx.recv().await);
    ///     assert_eq!(Some("world"), rx.recv().await);
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<T> {
        use crate::future::poll_fn;

        poll_fn(|cx| self.poll_recv(cx)).await
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

#[cfg(feature = "stream")]
impl<T> crate::stream::Stream for UnboundedReceiver<T> {
    type Item = T;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.poll_recv(cx)
    }
}

impl<T> UnboundedSender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> UnboundedSender<T> {
        UnboundedSender { chan }
    }

    /// Attempts to send a message on this `UnboundedSender` without blocking.
    ///
    /// This method is not marked async because sending a message to an unbounded channel
    /// never requires any form of waiting. Because of this, the `send` method can be
    /// used in both synchronous and asynchronous code without problems.
    ///
    /// If the receive half of the channel is closed, either due to [`close`]
    /// being called or the [`UnboundedReceiver`] having been dropped, this
    /// function returns an error. The error includes the value passed to `send`.
    ///
    /// [`close`]: UnboundedReceiver::close
    /// [`UnboundedReceiver`]: UnboundedReceiver
    pub fn send(&self, message: T) -> Result<(), SendError<T>> {
        if !self.inc_num_messages() {
            return Err(SendError(message));
        }

        self.chan.send(message);
        Ok(())
    }

    fn inc_num_messages(&self) -> bool {
        use std::process;
        use std::sync::atomic::Ordering::{AcqRel, Acquire};

        let mut curr = self.chan.semaphore().load(Acquire);

        loop {
            if curr & 1 == 1 {
                return false;
            }

            if curr == usize::MAX ^ 1 {
                // Overflowed the ref count. There is no safe way to recover, so
                // abort the process. In practice, this should never happen.
                process::abort()
            }

            match self
                .chan
                .semaphore()
                .compare_exchange(curr, curr + 2, AcqRel, Acquire)
            {
                Ok(_) => return true,
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    /// Completes when the receiver has dropped.
    ///
    /// This allows the producers to get notified when interest in the produced
    /// values is canceled and immediately stop doing work.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::mpsc;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (tx1, rx) = mpsc::unbounded_channel::<()>();
    ///     let tx2 = tx1.clone();
    ///     let tx3 = tx1.clone();
    ///     let tx4 = tx1.clone();
    ///     let tx5 = tx1.clone();
    ///     tokio::spawn(async move {
    ///         drop(rx);
    ///     });
    ///
    ///     futures::join!(
    ///         tx1.closed(),
    ///         tx2.closed(),
    ///         tx3.closed(),
    ///         tx4.closed(),
    ///         tx5.closed()
    ///     );
    ////     println!("Receiver dropped");
    /// }
    /// ```
    pub async fn closed(&self) {
        self.chan.closed().await
    }
    /// Checks if the channel has been closed. This happens when the
    /// [`UnboundedReceiver`] is dropped, or when the
    /// [`UnboundedReceiver::close`] method is called.
    ///
    /// [`UnboundedReceiver`]: crate::sync::mpsc::UnboundedReceiver
    /// [`UnboundedReceiver::close`]: crate::sync::mpsc::UnboundedReceiver::close
    ///
    /// ```
    /// let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    /// assert!(!tx.is_closed());
    ///
    /// let tx2 = tx.clone();
    /// assert!(!tx2.is_closed());
    ///
    /// drop(rx);
    /// assert!(tx.is_closed());
    /// assert!(tx2.is_closed());
    /// ```
    pub fn is_closed(&self) -> bool {
        self.chan.is_closed()
    }
}
