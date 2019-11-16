//! Channel error types

use std::error::Error;
use std::fmt;

/// Error returned by the `Sender`.
#[derive(Debug)]
pub struct SendError<T>(pub T);

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl<T: fmt::Debug> ::std::error::Error for SendError<T> {}

// ===== TrySendError =====

/// This enumeration is the list of the possible error outcomes for the
/// [try_send](super::Sender::try_send) method.
#[derive(Debug)]
pub enum TrySendError<T> {
    /// The data could not be sent on the channel because the channel is
    /// currently full and sending would require blocking.
    Full(T),

    /// The receive half of the channel was explicitly closed or has been
    /// dropped.
    Closed(T),
}

impl<T: fmt::Debug> Error for TrySendError<T> {}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            fmt,
            "{}",
            match self {
                TrySendError::Full(..) => "no available capacity",
                TrySendError::Closed(..) => "channel closed",
            }
        )
    }
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(src: SendError<T>) -> TrySendError<T> {
        TrySendError::Closed(src.0)
    }
}

// ===== RecvError =====

/// Error returned by `Receiver`.
#[derive(Debug)]
pub struct RecvError(());

impl fmt::Display for RecvError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl Error for RecvError {}

// ===== ClosedError =====

/// Erorr returned by [`Sender::poll_ready`](super::Sender::poll_ready)].
#[derive(Debug)]
pub struct ClosedError(());

impl ClosedError {
    pub(crate) fn new() -> ClosedError {
        ClosedError(())
    }
}

impl fmt::Display for ClosedError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl Error for ClosedError {}
