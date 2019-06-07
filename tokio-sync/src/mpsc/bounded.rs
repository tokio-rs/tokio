use super::chan;

use std::fmt;
use std::task::{Context, Poll};

#[cfg(feature = "async-traits")]
use std::pin::Pin;

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

/// Error returned by the `Sender`.
#[derive(Debug)]
pub struct SendError(());

/// Error returned by `Sender::try_send`.
#[derive(Debug)]
pub struct TrySendError<T> {
    kind: ErrorKind,
    value: T,
}

#[derive(Debug)]
enum ErrorKind {
    Closed,
    NoCapacity,
}

/// Error returned by `Receiver`.
#[derive(Debug)]
pub struct RecvError(());

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
/// use tokio::sync::mpsc::channel;
/// use tokio::prelude::*;
/// use futures::future::lazy;
///
/// # fn some_computation() -> impl Future<Item = (), Error = ()> + Send {
/// # futures::future::ok::<(), ()>(())
/// # }
///
/// tokio::run(lazy(|| {
///     let (tx, rx) = channel(100);
///
///     tokio::spawn({
///         some_computation()
///             .and_then(|value| {
///                 tx.send(value)
///                     .map_err(|_| ())
///             })
///             .map(|_| ())
///             .map_err(|_| ())
///     });
///
///     rx.for_each(|value| {
///         println!("got value = {:?}", value);
///         Ok(())
///     })
///     .map(|_| ())
///     .map_err(|_| ())
/// }));
/// ```
pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    assert!(buffer > 0, "mpsc bounded channel requires buffer > 0");
    let semaphore = (crate::semaphore::Semaphore::new(buffer), buffer);
    let (tx, rx) = chan::channel(semaphore);

    let tx = Sender::new(tx);
    let rx = Receiver::new(rx);

    (tx, rx)
}

/// Channel semaphore is a tuple of the semaphore implementation and a `usize`
/// representing the channel bound.
type Semaphore = (crate::semaphore::Semaphore, usize);

impl<T> Receiver<T> {
    pub(crate) fn new(chan: chan::Rx<T, Semaphore>) -> Receiver<T> {
        Receiver { chan }
    }

    /// TODO: Dox
    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
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

#[cfg(feature = "async-traits")]
impl<T> futures_core::Stream for Receiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        Receiver::poll_next(self.get_mut(), cx)
    }
}

impl<T> Sender<T> {
    pub(crate) fn new(chan: chan::Tx<T, Semaphore>) -> Sender<T> {
        Sender { chan }
    }

    /// Check if the `Sender` is ready to handle a value.
    ///
    /// Polls the channel to determine if there is guaranteed capacity to send
    /// at least one item without waiting.
    ///
    /// When `poll_ready` returns `Ready`, the channel reserves capacity for one
    /// message for this `Sender` instance. The capacity is held until a message
    /// is send or the `Sender` instance is dropped. Callers should ensure a
    /// message is sent in a timely fashion in order to not starve other
    /// `Sender` instances.
    ///
    /// # Return value
    ///
    /// This method returns:
    ///
    /// - `Poll::Ready(Ok(_))` if capacity is reserved for a single message.
    /// - `Poll::Pending` if the channel may not have capacity, in which
    ///   case the current task is queued to be notified once
    ///   capacity is available;
    /// - `Poll::Ready(Err(SendError))` if the receiver has been dropped.
    pub fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.chan.poll_ready(cx).map_err(|_| SendError(()))
    }

    /// Attempts to send a message on this `Sender`, returning the message
    /// if there was an error.
    pub fn try_send(&mut self, message: T) -> Result<(), TrySendError<T>> {
        self.chan.try_send(message)?;
        Ok(())
    }
}

#[cfg(feature = "async-traits")]
impl<T> async_sink::Sink<T> for Sender<T> {
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Sender::poll_ready(self.get_mut(), cx)
    }

    fn start_send(mut self: Pin<&mut Self>, msg: T) -> Result<(), Self::Error> {
        self.as_mut()
            .try_send(msg)
            .map_err(|err| {
                assert!(err.is_full(), "call `poll_ready` before sending");
                SendError(())
            })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

// ===== impl SendError =====

impl fmt::Display for SendError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for SendError {
    fn description(&self) -> &str {
        "channel closed"
    }
}

// ===== impl TrySendError =====

impl<T> TrySendError<T> {
    /// Get the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Did the send fail because the channel has been closed?
    pub fn is_closed(&self) -> bool {
        if let ErrorKind::Closed = self.kind {
            true
        } else {
            false
        }
    }

    /// Did the send fail because the channel was at capacity?
    pub fn is_full(&self) -> bool {
        if let ErrorKind::NoCapacity = self.kind {
            true
        } else {
            false
        }
    }
}

impl<T: fmt::Debug> fmt::Display for TrySendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl<T: fmt::Debug> ::std::error::Error for TrySendError<T> {
    fn description(&self) -> &str {
        match self.kind {
            ErrorKind::Closed => "channel closed",
            ErrorKind::NoCapacity => "no available capacity",
        }
    }
}

impl<T> From<(T, chan::TrySendError)> for TrySendError<T> {
    fn from((value, err): (T, chan::TrySendError)) -> TrySendError<T> {
        TrySendError {
            value,
            kind: match err {
                chan::TrySendError::Closed => ErrorKind::Closed,
                chan::TrySendError::NoPermits => ErrorKind::NoCapacity,
            },
        }
    }
}

// ===== impl RecvError =====

impl fmt::Display for RecvError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::error::Error;
        write!(fmt, "{}", self.description())
    }
}

impl ::std::error::Error for RecvError {
    fn description(&self) -> &str {
        "channel closed"
    }
}
