use crate::io::copy::{copy, Copy};
use crate::io::read::{read, Read};
use crate::io::read_exact::{read_exact, ReadExact};
use crate::io::read_to_end::{read_to_end, ReadToEnd};

use tokio_io::{AsyncRead, AsyncWrite};

/// An extension trait which adds utility methods to `AsyncRead` types.
pub trait AsyncReadExt: AsyncRead {
    /// Copy all data from `self` into the provided `AsyncWrite`.
    ///
    /// The returned future will copy all the bytes read from `reader` into the
    /// `writer` specified.  This future will only complete once the `reader`
    /// has hit EOF and all bytes have been written to and flushed from the
    /// `writer` provided.
    ///
    /// On success the number of bytes is returned and the `reader` and `writer`
    /// are consumed. On error the error is returned and the I/O objects are
    /// consumed as well.
    ///
    /// # Examples
    ///
    /// ```
    /// unimplemented!();
    /// ```
    fn copy<'a, W>(&'a mut self, dst: &'a mut W) -> Copy<'a, Self, W>
    where
        Self: Unpin,
        W: AsyncWrite + Unpin + ?Sized,
    {
        copy(self, dst)
    }

    /// Read data into the provided buffer.
    ///
    /// The returned future will resolve to the number of bytes read once the
    /// read operation is completed.
    ///
    /// # Examples
    ///
    /// ```
    /// unimplemented!();
    /// ```
    fn read<'a>(&'a mut self, dst: &'a mut [u8]) -> Read<'a, Self>
    where
        Self: Unpin,
    {
        read(self, dst)
    }

    /// Read exactly the amount of data needed to fill the provided buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// unimplemented!();
    /// ```
    fn read_exact<'a>(&'a mut self, dst: &'a mut [u8]) -> ReadExact<'a, Self>
    where
        Self: Unpin,
    {
        read_exact(self, dst)
    }

    /// Read all bytes until EOF in this source, placing them into `dst`.
    fn read_to_end<'a>(&'a mut self, dst: &'a mut Vec<u8>) -> ReadToEnd<'a, Self>
    where
        Self: Unpin,
    {
        read_to_end(self, dst)
    }
}

impl<R: AsyncRead + ?Sized> AsyncReadExt for R {}
