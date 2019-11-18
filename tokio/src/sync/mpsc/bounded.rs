use crate::sync::mpsc::chan;
use crate::sync::mpsc::error::{ClosedError, SendError, TrySendError};
use crate::sync::semaphore;

use std::fmt;
use std::task::{Context, Poll};

/// Send values to the associated `Receiver`.
///
/// Instances are created by the [`channel`](fn.channel.html) function.
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
/// Instances are created by the [`channel`](fn.channel.html) function.
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

/// Create a bounded mpsc channel for communicating between asynchronous tasks,
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

    /// Receive the next value for this receiver.
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
    impl<T> futures_core::Stream for Receiver<T> {
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

    #[doc(hidden)] // TODO: document
    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), ClosedError>> {
        self.chan.poll_ready(cx).map_err(|_| ClosedError::new())
    }

    /// Attempts to send a message on this `Sender`, returning the message
    /// if there was an error.
    pub fn try_send(&mut self, message: T) -> Result<(), TrySendError<T>> {
        self.chan.try_send(message)?;
        Ok(())
    }

    /// Send a value, waiting until there is capacity.
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
}
