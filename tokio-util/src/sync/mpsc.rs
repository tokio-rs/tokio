use futures_core::ready;
use futures_sink::Sink;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{error::SendError, Sender};

use super::ReusableBoxFuture;

// This implementation was chosen over something based on permits because to get a
// `tokio::sync::mpsc::Permit` out of the `inner` future, you must transmute the
// lifetime on the permit to `'static`.

/// A wrapper around [`mpsc::Sender`] that can be polled.
///
/// [`mpsc::Sender`]: tokio::sync::mpsc::Sender
#[derive(Debug)]
pub struct PollSender<T> {
    /// is none if closed
    sender: Option<Arc<Sender<T>>>,
    is_sending: bool,
    inner: ReusableBoxFuture<Result<(), SendError<T>>>,
}

// By reusing the same async fn for both Some and None, we make sure every
// future passed to ReusableBoxFuture has the same underlying type, and hence
// the same size and alignment.
async fn make_future<T>(data: Option<(Arc<Sender<T>>, T)>) -> Result<(), SendError<T>> {
    match data {
        Some((sender, value)) => sender.send(value).await,
        None => unreachable!(
            "This future should not be pollable, as is_sending should be set to false."
        ),
    }
}

impl<T: Send + 'static> PollSender<T> {
    /// Create a new `PollSender`.
    pub fn new(sender: Sender<T>) -> Self {
        Self {
            sender: Some(Arc::new(sender)),
            is_sending: false,
            inner: ReusableBoxFuture::new(make_future(None)),
        }
    }

    /// Start sending a new item.
    ///
    /// This method panics if a send is currently in progress. To ensure that no
    /// send is in progress, call `poll_send_done` first until it returns
    /// `Poll::Ready`.
    ///
    /// If this method returns an error, that indicates that the channel is
    /// closed. Note that this method is not guaranteed to return an error if
    /// the channel is closed, but in that case the error would be reported by
    /// the first call to `poll_send_done`.
    pub fn start_send(&mut self, value: T) -> Result<(), SendError<T>> {
        if self.is_sending {
            panic!("start_send called while not ready.");
        }
        match self.sender.clone() {
            Some(sender) => {
                self.inner.set(make_future(Some((sender, value))));
                self.is_sending = true;
                Ok(())
            }
            None => Err(SendError(value)),
        }
    }

    /// If a send is in progress, poll for its completion. If no send is in progress,
    /// this method returns `Poll::Ready(Ok(()))`.
    ///
    /// This method can return the following values:
    ///
    ///  - `Poll::Ready(Ok(()))` if the in-progress send has been completed, or there is
    ///    no send in progress (even if the channel is closed).
    ///  - `Poll::Ready(Err(err))` if the in-progress send failed because the channel has
    ///    been closed.
    ///  - `Poll::Pending` if a send is in progress, but it could not complete now.
    ///
    /// When this method returns `Poll::Pending`, the current task is scheduled
    /// to receive a wakeup when the message is sent, or when the entire channel
    /// is closed (but not if just this sender is closed by
    /// `close_this_sender`). Note that on multiple calls to `poll_send_done`,
    /// only the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup.
    ///
    /// If this method returns `Poll::Ready`, then `start_send` is guaranteed to
    /// not panic.
    pub fn poll_send_done(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), SendError<T>>> {
        if !self.is_sending {
            return Poll::Ready(Ok(()));
        }

        let result = self.inner.poll(cx);
        if result.is_ready() {
            self.is_sending = false;
        }
        if let Poll::Ready(Err(_)) = &result {
            self.sender = None;
        }
        result
    }

    /// Check whether the channel is ready to send more messages now.
    ///
    /// If this method returns `true`, then `start_send` is guaranteed to not
    /// panic.
    ///
    /// If the channel is closed, this method returns `true`.
    pub fn is_ready(&self) -> bool {
        !self.is_sending
    }

    /// Check whether the channel has been closed.
    pub fn is_closed(&self) -> bool {
        match &self.sender {
            Some(sender) => sender.is_closed(),
            None => true,
        }
    }

    /// Clone the underlying `Sender`.
    ///
    /// If this method returns `None`, then the channel is closed. (But it is
    /// not guaranteed to return `None` if the channel is closed.)
    pub fn clone_inner(&self) -> Option<Sender<T>> {
        self.sender.as_ref().map(|sender| (&**sender).clone())
    }

    /// Access the underlying `Sender`.
    ///
    /// If this method returns `None`, then the channel is closed. (But it is
    /// not guaranteed to return `None` if the channel is closed.)
    pub fn inner_ref(&self) -> Option<&Sender<T>> {
        self.sender.as_deref()
    }

    // This operation is supported because it is required by the Sink trait.
    /// Close this sender. No more messages can be sent from this sender.
    ///
    /// Note that this only closes the channel from the view-point of this
    /// sender. The channel remains open until all senders have gone away, or
    /// until the [`Receiver`] closes the channel.
    ///
    /// If there is a send in progress when this method is called, that send is
    /// unaffected by this operation, and `poll_send_done` can still be called
    /// to complete that send.
    ///
    /// [`Receiver`]: tokio::sync::mpsc::Receiver
    pub fn close_this_sender(&mut self) {
        self.sender = None;
    }

    /// Abort the current in-progress send, if any.
    ///
    /// Returns `true` if a send was aborted.
    pub fn abort_send(&mut self) -> bool {
        if self.is_sending {
            self.inner.set(make_future(None));
            self.is_sending = false;
            true
        } else {
            false
        }
    }
}

impl<T> Clone for PollSender<T> {
    /// Clones this `PollSender`. The resulting clone will not have any
    /// in-progress send operations, even if the current `PollSender` does.
    fn clone(&self) -> PollSender<T> {
        Self {
            sender: self.sender.clone(),
            is_sending: false,
            inner: ReusableBoxFuture::new(async { unreachable!() }),
        }
    }
}

impl<T: Send + 'static> Sink<T> for PollSender<T> {
    type Error = SendError<T>;

    /// This is equivalent to calling [`poll_send_done`].
    ///
    /// [`poll_send_done`]: PollSender::poll_send_done
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_send_done(cx)
    }

    /// This is equivalent to calling [`poll_send_done`].
    ///
    /// [`poll_send_done`]: PollSender::poll_send_done
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_send_done(cx)
    }

    /// This is equivalent to calling [`start_send`].
    ///
    /// [`start_send`]: PollSender::start_send
    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        Pin::into_inner(self).start_send(item)
    }

    /// This method will first flush the `PollSender`, and then close it by
    /// calling [`close_this_sender`].
    ///
    /// If a send fails while flushing because the [`Receiver`] has gone away,
    /// then this function returns an error. The channel is still successfully
    /// closed in this situation.
    ///
    /// [`close_this_sender`]: PollSender::close_this_sender
    /// [`Receiver`]: tokio::sync::mpsc::Receiver
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;

        Pin::into_inner(self).close_this_sender();
        Poll::Ready(Ok(()))
    }
}
