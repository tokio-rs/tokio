use crate::io::flush::{flush, Flush};
use crate::io::shutdown::{shutdown, Shutdown};
use crate::io::write::{write, Write};
use crate::io::write_all::{write_all, WriteAll};
use crate::io::{BufStream, BufWriter};
use crate::{AsyncRead, AsyncWrite};

/// An extension trait which adds utility methods to `AsyncWrite` types.
pub trait AsyncWriteExt: AsyncWrite {
    /// Write a buffer into this writter, returning how many bytes were written.
    fn write<'a>(&'a mut self, src: &'a [u8]) -> Write<'a, Self>
    where
        Self: Unpin,
    {
        write(self, src)
    }

    /// Attempt to write an entire buffer into this writter.
    fn write_all<'a>(&'a mut self, src: &'a [u8]) -> WriteAll<'a, Self>
    where
        Self: Unpin,
    {
        write_all(self, src)
    }

    /// Flush the contents of this writer.
    fn flush(&mut self) -> Flush<'_, Self>
    where
        Self: Unpin,
    {
        flush(self)
    }

    /// Shutdown this writer.
    fn shutdown(&mut self) -> Shutdown<'_, Self>
    where
        Self: Unpin,
    {
        shutdown(self)
    }

    /// Wraps the writer in a [`BufWriter`] so that small writes are batched to the underlying
    /// [`AsyncWrite`].
    ///
    /// See [`BufWriter`] for details.
    fn buffered(self) -> BufWriter<Self>
    where
        Self: Sized,
    {
        BufWriter::new(self)
    }

    /// Wraps the underlying stream ([`AsyncRead`] + [`AsyncWrite`]) in a [`BufStream`] so that
    /// small reads and writes are batched to the underlying stream.
    ///
    /// See [`BufStream`] for details.
    fn buffered_duplex(self) -> BufStream<Self>
    where
        Self: Sized + AsyncRead,
    {
        BufStream::new(self)
    }
}

impl<W: AsyncWrite + ?Sized> AsyncWriteExt for W {}
