use std::io as std_io;
use futures::{Async, Poll};

use AsyncWrite;

/// Seek files asynchronously.
///
/// This trait inherits from `std::io::Seek` and indicates that an I/O object is
/// **non-blocking**. All non-blocking I/O objects must return an error when
/// seeking is unavailable instead of blocking the current thread.
///
/// Specifically, this means that the `poll_seek` function will return one of
/// the following:
///

/// * `Ok(Async::Ready(n))` means that the seek was successful, and `n` is the
///   new position in the file.
///

/// * `Ok(Async::NotReady)` means that the I/O object is not currently seekable
///   but may become seekable in the future. Most importantly, **the current
///   future's task is scheduled to get unparked when the object is seekable**.
///   This means that like `Future::poll` you'll receive a notification when
///   the I/O object is seekable again.
///
/// * `Err(e)` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the `seek` method only works in the
/// context of a future's task. The object may panic if used outside of a task.
pub trait AsyncSeek: std_io::Seek {
    /// Seek to an offset, in bytes, in a stream.
    ///
    /// A seek beyond the end of a stream is allowed, but implementation
    /// defined.
    ///
    /// If the seek operation completed successfully, this method returns the
    /// new position from the start of the stream. That position can be used
    /// later with `SeekFrom::Start`.
    ///
    /// # Errors
    ///
    /// Seeking to a negative offset is considered an error.
    fn poll_seek(&mut self, pos: std_io::SeekFrom) -> Poll<u64, std_io::Error> {
        match self.seek(pos) {
            Ok(t) => Ok(Async::Ready(t)),
            Err(ref e) if e.kind() == std_io::ErrorKind::WouldBlock => {
                return Ok(Async::NotReady)
            }
            Err(e) => return Err(e.into())
        }
    }
}

impl<T: ?Sized + AsyncSeek> AsyncSeek for Box<T> { }

impl<'a, T: ?Sized + AsyncSeek> AsyncSeek for &'a mut T { }

impl<T: AsRef<[u8]>> AsyncSeek for std_io::Cursor<T> { }

impl<T: AsyncSeek> AsyncSeek for std_io::BufReader<T> { }

impl<T: AsyncSeek + AsyncWrite> AsyncSeek for std_io::BufWriter<T> { }
