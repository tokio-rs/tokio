use futures_core::ready;
use futures_sink::Sink;
use tokio::sync::mpsc::OwnedPermit;
use std::{fmt, mem};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::Sender;

use super::ReusableBoxFuture;

/// Error returned by the `PollSender` when the channel is closed.
#[derive(Debug)]
pub struct PollSendError<T>(Option<T>);

impl<T> PollSendError<T> {
    /// Consumes the stored value, if any.
    /// 
    /// If this error was encountered when calling `start_send`, this will be the item that the
    /// caller was attempting to send.  Otherwise, it will be `None`.
    pub fn into_inner(self) -> Option<T> {
        self.0
    }
}

impl<T> fmt::Display for PollSendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "channel closed")
    }
}

impl<T: fmt::Debug> std::error::Error for PollSendError<T> {}

#[derive(Debug)]
enum State<T> {
    Idle(Sender<T>),
    Acquiring,
    ReadyToSend(OwnedPermit<T>),
    Closed,
}

/// A wrapper around [`mpsc::Sender`] that can be polled.
///
/// [`mpsc::Sender`]: tokio::sync::mpsc::Sender
#[derive(Debug)]
pub struct PollSender<T> {
    sender: Option<Sender<T>>,
    state: State<T>,
    acquire: ReusableBoxFuture<Result<OwnedPermit<T>, PollSendError<T>>>,
}

// Creates a future for acquiring a permit from the underlying channel.  This is used to ensure
// there's capacity for a send to complete.
//
// By reusing the same async fn for both `Some` and `None`, we make sure every future passed to
// ReusableBoxFuture has the same underlying type, and hence the same size and alignment.
async fn make_acquire_future<T>(data: Option<Sender<T>>) -> Result<OwnedPermit<T>, PollSendError<T>> {
    match data {
        Some(sender) => sender.reserve_owned().await.map_err(|_| PollSendError(None)),
        None => unreachable!("this future should not be pollable in this state"),
    }
}

impl<T: Send + 'static> PollSender<T> {
    /// Create a new `PollSender`.
    pub fn new(sender: Sender<T>) -> Self {
        Self {
            sender: Some(sender.clone()),
            state: State::Idle(sender),
            acquire: ReusableBoxFuture::new(make_acquire_future(None)),
        }
    }

    fn take_state(&mut self) -> State<T> {
        mem::replace(&mut self.state, State::Closed)
    }

    /// Attempts to prepare the sender to receive a value.
    ///
    /// This method must be called and return `Poll::Ready(Ok(()))` prior to each call to
    /// `start_send`.
    ///
    /// This method returns `Poll::Ready` once the underlying channel is ready to receive a value,
    /// by reserving a slot in the channel for the item to be sent. If this method returns
    /// `Poll::Pending`, the current task is registered to be notified (via
    /// `cx.waker().wake_by_ref()`) when `poll_ready` should be called again.
    ///
    /// # Errors
    /// 
    /// If the channel is closed, an error will be returned.  This is a permanent state.
    /// 
    /// # Panics
    /// 
    /// If a previous call to `poll_reserve` returned `Poll::Ready(Ok(()))`, and `poll_reserve` is
    /// called again immediately after without a call to `start_send` first, the second call to
    /// `poll_reserve` will panic.
    pub fn poll_reserve(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), PollSendError<T>>> {
        let (result, next_state) = match self.take_state() {
            State::Idle(sender) => {
                self.acquire.set(make_acquire_future(Some(sender)));
                match self.acquire.poll(cx) {
                    // Channel has capacity.
                    Poll::Ready(Ok(permit)) => (Poll::Ready(Ok(())), State::ReadyToSend(permit)),
                    // Channel is closed.
                    Poll::Ready(Err(e)) => (Poll::Ready(Err(e)), State::Closed),
                    // Channel doesn't have capacity yet, so we need to wait.
                    Poll::Pending => (Poll::Pending, State::Acquiring),
                }
            },
            State::Acquiring => match ready!(self.acquire.poll(cx)) {
                // Channel has capacity.
                Ok(permit) => (Poll::Ready(Ok(())), State::ReadyToSend(permit)),
                // Channel is closed.
                Err(e) => (Poll::Ready(Err(e)), State::Closed), 
            },
            // We're closed, either by choice or because the underlying sender was closed.
            State::Closed => return Poll::Ready(Err(PollSendError(None))),
            State::ReadyToSend(_) => panic!("called `poll_ready` when `start_send` was expected"),
        };

        self.state = next_state;
        result
    }

    /// Sends an item to the channel.
    /// 
    /// Before calling `start_send`, `poll_reserve` must be called with a successful return
    /// value of `Poll::Ready(Ok(()))`.
    /// 
    /// # Errors
    /// 
    /// If the channel is closed, an error will be returned.  This is a permanent state.
    /// 
    /// # Panics
    /// 
    /// If `poll_reserve` was not successfully called prior to calling `start_send`, then this method
    /// will panic.
    pub fn start_send(&mut self, value: T) -> Result<(), PollSendError<T>> {
        let (result, next_state) = match self.take_state() {
            State::Idle(_) | State::Acquiring => panic!("`start_send` called without first calling `poll_ready`"),
            // We have a permit to send our item, so go ahead, which gets us our sender back.
            State::ReadyToSend(permit) => (Ok(()), State::Idle(permit.send(value))),
            // We're closed, either by choice or because the underlying sender was closed.
            State::Closed => (Err(PollSendError(Some(value))), State::Closed),
        };

        // Handle deferred closing if `close_this_sender` was called between `poll_reserve` and `start_send`.
        self.state = if self.sender.is_some() { 
            next_state
        } else {
            State::Closed
        };
        result
    }

    /// Check whether the channel has been closed.
    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }

    /// Clones the underlying `Sender`.
    ///
    /// If the channel is closed, `None` is returned.
    pub fn clone_inner(&self) -> Option<Sender<T>> {
        self.sender.clone()
    }

    /// Gets a reference to the underlying `Sender`.
    /// 
    /// If the channel is closed, `None` is returned.
    pub fn inner_ref(&self) -> Option<&Sender<T>> {
        self.sender.as_ref()
    }

    /// Close this sender. No more messages can be sent from this sender.
    ///
    /// Note that this only closes the channel from the view-point of this sender. The channel
    /// remains open until all senders have gone away, or until the [`Receiver`] closes the channel.
    ///
    /// If a slot was previously reserved by calling `poll_reserve`, then a final call can be made
    /// to `start_send` in order to consume the reserved slot.  After that, no further sends will be
    /// possible.  If you do not intend to send another item, you can release the reserved slot back
    /// to the underlying sender by calling [`abort_send`].
    ///
    /// [`Receiver`]: tokio::sync::mpsc::Receiver
    pub fn close_this_sender(&mut self) {
        // Mark ourselves officially closed by dropping our main sender.
        self.sender = None;

        // We don't want to abort an in-progress send since the caller might still want to complete
        // it.  If that's the case, we'll defer updating our state until `start_send` is called, but
        // otherwise, we can speed up the process if we're already idle.
        if let State::Idle(_) = self.state {
            self.state = State::Closed;
        }
    }

    /// Aborts the current in-progress send, if any.
    ///
    /// Returns `true` if a send was aborted.  If the sender was closed prior to calling
    /// `abort_send`, then the sender will remain in the closed state, otherwise the sender will be
    /// ready to attempt another send.
    pub fn abort_send(&mut self) -> bool {
        // We may have been closed in the meantime, after a call to `poll_reserve` already
        // succeeded.  We'll check if `self.sender` is `None` to see if we should transition to the
        // closed state when we actually abort a send, rather than resetting ourselves back to idle.

        let (result, next_state) = match self.take_state() {
            // We're currently trying to reserve a slot to send into.
            State::Acquiring => {
                // Replacing the future drops the in-flight one.
                self.acquire.set(make_acquire_future(None));

                // If we haven't closed yet, we have to clone our stored sender since we have no way
                // to get it back from the acquire future we just dropped.
                let state = match self.sender.clone() {
                    Some(sender) => State::Idle(sender),
                    None => State::Closed,
                };
                (true, state)
            },
            // We got the permit.  If we haven't closed yet, get the sender back.
            State::ReadyToSend(permit) => {
                let state = if self.sender.is_some() {
                    State::Idle(permit.release())
                } else {
                    State::Closed
                };
                (true, state)
            },
            s => (false, s),
        };

        self.state = next_state;
        result
    }
}

impl<T> Clone for PollSender<T> {
    /// Clones this `PollSender`.
    /// 
    /// The resulting `PollSender` will have an initial state identical to calling `PollSender::new`.
    fn clone(&self) -> PollSender<T> {
        let (sender, state) = match self.sender.clone() {
            Some(sender) => (Some(sender.clone()), State::Idle(sender)),
            None => (None, State::Closed),
        };
        
        Self {
            sender,
            state,
            acquire: ReusableBoxFuture::new(async { unreachable!() }),
        }
    }
}

impl<T: Send + 'static> Sink<T> for PollSender<T> {
    type Error = PollSendError<T>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_reserve(cx)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        Pin::into_inner(self).start_send(item)
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).close_this_sender();
        Poll::Ready(Ok(()))
    }
}
