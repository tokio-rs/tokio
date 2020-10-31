use super::write_vectored::{write_vectored, WriteVectored};
use crate::io::vec::AsyncVectoredWrite;

use std::io::IoSlice;

/// Vectored output with an async method.
///
/// Implemented as an extention trait, adding the `write_vectored`
/// utility method to all [`AsyncVectoredWrite`] types. Callers will
/// tend to import this trait instead of [`AsyncVectoredWrite`].
pub trait AsyncVectoredWriteExt: AsyncVectoredWrite {
    /// Like [`AsyncWriteExt::write`], except that it writes from
    /// a slice of buffers.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn write_vectored(&mut self, slices: &[IoSlice<'_>]) -> io::Result<usize>;
    /// ```
    ///
    /// [`AsyncWriteExt::write`]: crate::io::AsyncWriteExt::write
    fn write_vectored<'a>(&'a mut self, slices: &'a [IoSlice<'a>]) -> WriteVectored<'a, Self>
    where
        Self: Unpin,
    {
        write_vectored(self, slices)
    }
}

impl<W: AsyncVectoredWrite + ?Sized> AsyncVectoredWriteExt for W {}
