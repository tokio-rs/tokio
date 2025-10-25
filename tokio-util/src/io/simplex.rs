//! Unidirectional byte-oriented channel.

use bytes::Buf;
use bytes::BytesMut;
use futures_core::ready;
use std::io::Error as IoError;
use std::io::ErrorKind as IoErrorKind;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::task::coop::poll_proceed;

type IoResult<T> = Result<T, IoError>;

#[derive(Debug)]
struct Inner {
    /// `poll_*` will return [`Poll::Pending`] if the backpressure boundary is reached
    backpressure_boundary: usize,

    /// either [`Sender`] or [`Receiver`] is closed
    is_closed: bool,

    /// Waker used to wake the [`Receiver`]
    receiver_waker: Option<Waker>,

    /// Waker used to wake the [`Sender`]
    sender_waker: Option<Waker>,

    /// Buffer used to read and write data
    buf: BytesMut,
}

impl Inner {
    fn with_capacity(backpressure_boundary: usize) -> Self {
        Self {
            backpressure_boundary,
            is_closed: false,
            receiver_waker: None,
            sender_waker: None,
            buf: BytesMut::new(),
        }
    }

    fn register_receiver_waker(&mut self, waker: &Waker) {
        match self.receiver_waker.as_mut() {
            Some(old) if old.will_wake(waker) => {}
            Some(old) => old.clone_from(waker),
            None => self.receiver_waker = Some(waker.clone()),
        }
    }

    fn register_sender_waker(&mut self, waker: &Waker) {
        match self.sender_waker.as_mut() {
            Some(old) if old.will_wake(waker) => {}
            Some(old) => old.clone_from(waker),
            None => self.sender_waker = Some(waker.clone()),
        }
    }

    fn wake_receiver(&mut self) {
        if let Some(waker) = self.receiver_waker.take() {
            waker.wake();
        }
    }

    fn wake_sender(&mut self) {
        if let Some(waker) = self.sender_waker.take() {
            waker.wake();
        }
    }

    fn is_closed(&self) -> bool {
        self.is_closed
    }

    fn close_receiver(&mut self) {
        self.is_closed = true;
        self.wake_sender();
    }

    fn close_sender(&mut self) {
        self.is_closed = true;
        self.wake_receiver();
    }
}

/// Receiver of the simplex channel.
///
/// You can still read the remaining data from the buffer
/// even if the write half has been dropped.
/// See [`Sender::poll_shutdown`] and [`Sender::drop`] for more details.
#[derive(Debug)]
pub struct Receiver {
    inner: Arc<Mutex<Inner>>,
}

impl Drop for Receiver {
    /// This also wakes up the [`Sender`].
    fn drop(&mut self) {
        self.inner.lock().unwrap().close_receiver();
    }
}

impl AsyncRead for Receiver {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        let mut inner = self.inner.lock().unwrap();

        let to_read = buf.remaining().min(inner.buf.remaining());
        if to_read == 0 {
            return if inner.is_closed() {
                Poll::Ready(Ok(()))
            } else {
                inner.register_receiver_waker(cx.waker());
                inner.wake_sender();
                Poll::Pending
            };
        }

        ready!(poll_proceed(cx)).made_progress();

        buf.put_slice(&inner.buf[..to_read]);
        inner.buf.advance(to_read);
        inner.wake_sender();
        Poll::Ready(Ok(()))
    }
}

/// Sender of the simplex channel.
///
/// ## Shutdown
///
/// See [`Sender::poll_shutdown`].
#[derive(Debug)]
pub struct Sender {
    inner: Arc<Mutex<Inner>>,
}

impl Drop for Sender {
    /// This also wakes up the [`Receiver`].
    fn drop(&mut self) {
        self.inner.lock().unwrap().close_sender();
    }
}

impl AsyncWrite for Sender {
    /// # Error
    ///
    /// This method will return [`IoErrorKind::BrokenPipe`]
    /// if the channel has been closed.
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        let mut inner = self.inner.lock().unwrap();

        if inner.is_closed() {
            return Poll::Ready(Err(IoError::new(
                IoErrorKind::BrokenPipe,
                "simplex has been closed",
            )));
        }

        let free = inner
            .backpressure_boundary
            .checked_sub(inner.buf.len())
            .expect("backpressure boundary overflow");
        let to_write = buf.len().min(free);
        if to_write == 0 {
            inner.register_sender_waker(cx.waker());
            inner.wake_receiver();
            return Poll::Pending;
        }

        // this is to avoid starving other tasks
        ready!(poll_proceed(cx)).made_progress();

        inner.buf.extend_from_slice(&buf[..to_write]);
        inner.wake_receiver();
        Poll::Ready(Ok(to_write))
    }

    /// # Error
    ///
    /// This method will return [`IoErrorKind::BrokenPipe`]
    /// if the channel has been closed.
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let inner = self.inner.lock().unwrap();
        if inner.is_closed() {
            Poll::Ready(Err(IoError::new(
                IoErrorKind::BrokenPipe,
                "simplex has been shut down",
            )))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    /// After returns [`Poll::Ready`], all the following call to
    /// [`Sender::poll_write`] and [`Sender::poll_flush`]
    /// will return error.
    ///
    /// The [`Receiver`] can still be used to read remaining data
    /// until all bytes have been consumed.
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let mut inner = self.inner.lock().unwrap();

        if inner.is_closed() {
            Poll::Ready(Err(IoError::new(
                IoErrorKind::BrokenPipe,
                "simplex has already been shut down, cannot be shut down again",
            )))
        } else {
            inner.close_sender();
            Poll::Ready(Ok(()))
        }
    }
}

/// Create a simplex channel.
///
/// The `capacity` parameter specifies the maximum number of bytes that can be
/// stored in the channel without making the [`Sender::poll_write`]
/// return [`Poll::Pending`].
pub fn new(capacity: usize) -> (Sender, Receiver) {
    let inner = Arc::new(Mutex::new(Inner::with_capacity(capacity)));
    let tx = Sender {
        inner: Arc::clone(&inner),
    };
    let rx = Receiver { inner };
    (tx, rx)
}
