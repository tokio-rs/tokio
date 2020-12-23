use std::io::{self, IoSlice};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Writes bytes asynchronously.
///
/// The trait inherits from [`std::io::Write`] and indicates that an I/O object is
/// **nonblocking**. All non-blocking I/O objects must return an error when
/// bytes cannot be written instead of blocking the current thread.
///
/// Specifically, this means that the [`poll_write`] function will return one of
/// the following:
///
/// * `Poll::Ready(Ok(n))` means that `n` bytes of data was immediately
///   written.
///
/// * `Poll::Pending` means that no data was written from the buffer
///   provided. The I/O object is not currently writable but may become writable
///   in the future. Most importantly, **the current future's task is scheduled
///   to get unparked when the object is writable**. This means that like
///   `Future::poll` you'll receive a notification when the I/O object is
///   writable again.
///
/// * `Poll::Ready(Err(e))` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the [`write`][stdwrite] method only works in
/// the context of a future's task. The object may panic if used outside of a task.
///
/// Note that this trait also represents that the  [`Write::flush`][stdflush] method
/// works very similarly to the `write` method, notably that `Ok(())` means that the
/// writer has successfully been flushed, a "would block" error means that the
/// current task is ready to receive a notification when flushing can make more
/// progress, and otherwise normal errors can happen as well.
///
/// Utilities for working with `AsyncWrite` values are provided by
/// [`AsyncWriteExt`].
///
/// [`std::io::Write`]: std::io::Write
/// [`poll_write`]: AsyncWrite::poll_write()
/// [stdwrite]: std::io::Write::write()
/// [stdflush]: std::io::Write::flush()
/// [`AsyncWriteExt`]: crate::io::AsyncWriteExt
pub use t10::io::AsyncWrite;

