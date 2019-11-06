use bytes::Buf;

use crate::io::io::flush::{flush, Flush};
use crate::io::io::shutdown::{shutdown, Shutdown};
use crate::io::io::write::{write, Write};
use crate::io::io::write_all::{write_all, WriteAll};
use crate::io::AsyncWrite;

/// An extension trait which adds utility methods to `AsyncWrite` types.
pub trait AsyncWriteExt: AsyncWrite {
    /// Write a buffer into this writter, returning how many bytes were written.
    fn write<'a, B>(&'a mut self, buf: B) -> Write<'a, Self, B>
    where
        Self: Unpin,
        B: Buf + Unpin,
    {
        write(self, buf)
    }

    /// Attempt to write an entire buffer into this writter.
    fn write_all<'a, B>(&'a mut self, buf: B) -> WriteAll<'a, Self, B>
    where
        Self: Unpin,
        B: Buf + Unpin,
    {
        write_all(self, buf)
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
}

impl<W: AsyncWrite + ?Sized> AsyncWriteExt for W {}
