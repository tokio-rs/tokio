use crate::io::write::{write, Write};
use crate::io::write_all::{write_all, WriteAll};

use tokio_io::AsyncWrite;

/// An extension trait which adds utility methods to `AsyncWrite` types.
pub trait AsyncWriteExt: AsyncWrite {
    /// Write a buffer into this writter, returning how many bytes were written.
    ///
    /// # Examples
    ///
    /// ```
    /// unimplemented!();
    /// ````
    fn write<'a>(&'a mut self, src: &'a [u8]) -> Write<'a, Self>
    where
        Self: Unpin,
    {
        write(self, src)
    }

    /// Attempt to write an entire buffer into this writter.
    ///
    /// # Examples
    ///
    /// ```
    /// unimplemented!();
    /// ```
    fn write_all<'a>(&'a mut self, src: &'a [u8]) -> WriteAll<'a, Self>
    where
        Self: Unpin,
    {
        write_all(self, src)
    }
}

impl<W: AsyncWrite + ?Sized> AsyncWriteExt for W {}
