use std::io::{self, SeekFrom};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

use super::seek::{seek, Seek};

/// Seek bytes asynchronously.
///
/// This trait is analogous to the [`std::io::Seek`] trait, but integrates
/// with the asynchronous task system. In particular, the `start_seek`
/// method, unlike [`Seek::seek`], will not block the calling thread.
///
/// [`std::io::Seek`]: std::io::Seek
/// [`Seek::seek`]: std::io::Seek::seek()
pub trait AsyncSeek {
    /// Attempts to seek to an offset, in bytes, in a stream.
    ///
    /// A seek beyond the end of a stream is allowed, but behavior is defined
    /// by the implementation.
    ///
    /// If this function returns successfully, then the job has been submitted.
    /// To find out when it completes, call `poll_complete`.
    ///
    /// # Errors
    ///
    /// This function can return [`io::ErrorKind::Other`] in case there is
    /// another seek in progress. To avoid this, it is advisable that any call
    /// to `start_seek` is preceded by a call to `poll_complete` to ensure all
    /// pending seeks have completed.
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()>;

    /// Waits for a seek operation to complete.
    ///
    /// If the seek operation completed successfully,
    /// this method returns the new position from the start of the stream.
    /// That position can be used later with [`SeekFrom::Start`]. Repeatedly
    /// calling this function without calling `start_seek` might return the
    /// same result.
    ///
    /// # Errors
    ///
    /// Seeking to a negative offset is considered an error.
    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>>;

    /// Creates a future which will seek an IO object, and then yield the
    /// new position in the object and the object itself.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
    /// ```
    ///
    /// In the case of an error the buffer and the object will be discarded, with
    /// the error yielded.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::fs::File;
    /// use tokio::prelude::*;
    ///
    /// use std::io::SeekFrom;
    ///
    /// # async fn dox() -> std::io::Result<()> {
    /// let mut file = File::open("foo.txt").await?;
    /// file.seek(SeekFrom::Start(6)).await?;
    ///
    /// let mut contents = vec![0u8; 10];
    /// file.read_exact(&mut contents).await?;
    /// # Ok(())
    /// # }
    /// ```
    fn seek(&mut self, pos: SeekFrom) -> Seek<'_, Self>
    where
        Self: Sized + Unpin,
    {
        seek(self, pos)
    }
}

macro_rules! deref_async_seek {
    () => {
        fn start_seek(mut self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
            Pin::new(&mut **self).start_seek(pos)
        }

        fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Pin::new(&mut **self).poll_complete(cx)
        }
    };
}

impl<T: ?Sized + AsyncSeek + Unpin> AsyncSeek for Box<T> {
    deref_async_seek!();
}

impl<T: ?Sized + AsyncSeek + Unpin> AsyncSeek for &mut T {
    deref_async_seek!();
}

impl<P> AsyncSeek for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncSeek,
{
    fn start_seek(self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
        self.get_mut().as_mut().start_seek(pos)
    }

    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.get_mut().as_mut().poll_complete(cx)
    }
}

impl<T: AsRef<[u8]> + Unpin> AsyncSeek for io::Cursor<T> {
    fn start_seek(mut self: Pin<&mut Self>, pos: SeekFrom) -> io::Result<()> {
        io::Seek::seek(&mut *self, pos).map(drop)
    }
    fn poll_complete(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Poll::Ready(Ok(self.get_mut().position()))
    }
}
