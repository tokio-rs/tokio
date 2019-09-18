use crate::io::chain::{chain, Chain};
use crate::io::copy::{copy, Copy};
use crate::io::read::{read, Read};
use crate::io::read_exact::{read_exact, ReadExact};
use crate::io::read_to_end::{read_to_end, ReadToEnd};
use crate::io::read_to_string::{read_to_string, ReadToString};
use crate::io::take::{take, Take};
use crate::io::{BufReader, BufStream};
use crate::{AsyncRead, AsyncWrite};

/// An extension trait which adds utility methods to `AsyncRead` types.
pub trait AsyncReadExt: AsyncRead {
    /// Creates an adaptor which will chain this stream with another.
    ///
    /// The returned `AsyncRead` instance will first read all bytes from this object
    /// until EOF is encountered. Afterwards the output is equivalent to the
    /// output of `next`.
    fn chain<R>(self, next: R) -> Chain<Self, R>
    where
        Self: Sized,
        R: AsyncRead,
    {
        chain(self, next)
    }

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
    fn read<'a>(&'a mut self, dst: &'a mut [u8]) -> Read<'a, Self>
    where
        Self: Unpin,
    {
        read(self, dst)
    }

    /// Read exactly the amount of data needed to fill the provided buffer.
    fn read_exact<'a>(&'a mut self, dst: &'a mut [u8]) -> ReadExact<'a, Self>
    where
        Self: Unpin,
    {
        read_exact(self, dst)
    }

    /// Read all bytes until EOF in this source, placing them into `dst`.
    ///
    /// On success the total number of bytes read is returned.
    fn read_to_end<'a>(&'a mut self, dst: &'a mut Vec<u8>) -> ReadToEnd<'a, Self>
    where
        Self: Unpin,
    {
        read_to_end(self, dst)
    }

    /// Read all bytes until EOF in this source, placing them into `dst`.
    ///
    /// On success the total number of bytes read is returned.
    fn read_to_string<'a>(&'a mut self, dst: &'a mut String) -> ReadToString<'a, Self>
    where
        Self: Unpin,
    {
        read_to_string(self, dst)
    }

    /// Creates an AsyncRead adapter which will read at most `limit` bytes
    /// from the underlying reader.
    fn take(self, limit: u64) -> Take<Self>
    where
        Self: Sized,
    {
        take(self, limit)
    }

    /// Wraps the reader in a [`BufReader`] so that small reads are batched to the underlying
    /// [`AsyncRead`].
    ///
    /// See [`BufReader`] for details.
    fn buffered(self) -> BufReader<Self>
    where
        Self: Sized,
    {
        BufReader::new(self)
    }

    /// Wraps the underlying stream ([`AsyncRead`] + [`AsyncWrite`]) in a [`BufStream`] so that
    /// small reads and writes are batched to the underlying stream.
    ///
    /// See [`BufStream`] for details.
    fn buffered_duplex(self) -> BufStream<Self>
    where
        Self: Sized + AsyncWrite,
    {
        BufStream::new(self)
    }
}

impl<R: AsyncRead + ?Sized> AsyncReadExt for R {}
