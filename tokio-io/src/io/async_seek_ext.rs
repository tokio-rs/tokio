use crate::io::seek::{seek, Seek};
use crate::AsyncSeek;
use std::io::SeekFrom;

/// An extension trait which adds utility methods to `AsyncSeek` types.
pub trait AsyncSeekExt: AsyncSeek {
    /// Creates a future which will seek an IO object, and then yield the
    /// new position in the object and the object itself.
    ///
    /// In the case of an error the buffer and the object will be discarded, with
    /// the error yielded.
    fn seek(&mut self, pos: SeekFrom) -> Seek<'_, Self>
    where
        Self: Unpin,
    {
        seek(self, pos)
    }
}

impl<S: AsyncSeek + ?Sized> AsyncSeekExt for S {}
