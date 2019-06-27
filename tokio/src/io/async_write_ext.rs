use crate::io::write::{write, Write};

use tokio_io::AsyncWrite;

/// An extension trait which adds utility methods to `AsyncWrite` types.
pub trait AsyncWriteExt: AsyncWrite {
    /// Write the provided data into `self`.
    ///
    /// The returned future will resolve to the number of bytes written once the
    /// write operation is completed.
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
}

impl<W: AsyncWrite + ?Sized> AsyncWriteExt for W {}
