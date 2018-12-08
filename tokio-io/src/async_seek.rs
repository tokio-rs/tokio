use std::io::{self as std_io, Seek};
use futures::{Async, Poll};

/// Seek files asynchronously.
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
pub trait AsyncSeek {
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
    fn poll_seek(&mut self, pos: std_io::SeekFrom) -> Poll<u64, std_io::Error>;
}

impl<T: ?Sized + AsyncSeek> AsyncSeek for Box<T> {
    fn poll_seek(&mut self, pos: std_io::SeekFrom) -> Poll<u64, std_io::Error> {
        (**self).poll_seek(pos)
    }
}

impl<'a, T: ?Sized + AsyncSeek> AsyncSeek for &'a mut T {
    fn poll_seek(&mut self, pos: std_io::SeekFrom) -> Poll<u64, std_io::Error> {
        (**self).poll_seek(pos)
    }
}

impl<T: AsRef<[u8]>> AsyncSeek for std_io::Cursor<T> {
    fn poll_seek(&mut self, pos: std_io::SeekFrom) -> Poll<u64, std_io::Error> {
        self.seek(pos).map(|n| Async::Ready(n))
    }
}
