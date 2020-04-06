use crate::io::{AsyncBufRead, AsyncRead};

use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    /// An async reader which is always at EOF.
    ///
    /// This struct is generally created by calling [`empty`]. Please see
    /// the documentation of [`empty()`][`empty`] for more details.
    ///
    /// This is an asynchronous version of [`std::io::empty`][std].
    ///
    /// [`empty`]: fn@empty
    /// [std]: std::io::empty
    pub struct Empty {
        _p: (),
    }

    /// Creates a new empty async reader.
    ///
    /// All reads from the returned reader will return `Poll::Ready(Ok(0))`.
    ///
    /// This is an asynchronous version of [`std::io::empty`][std].
    ///
    /// [std]: std::io::empty
    ///
    /// # Examples
    ///
    /// A slightly sad example of not reading anything into a buffer:
    ///
    /// ```
    /// use tokio::io::{self, AsyncReadExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut buffer = String::new();
    ///     io::empty().read_to_string(&mut buffer).await.unwrap();
    ///     assert!(buffer.is_empty());
    /// }
    /// ```
    pub fn empty() -> Empty {
        Empty { _p: () }
    }
}

impl AsyncRead for Empty {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(0))
    }
}

impl AsyncBufRead for Empty {
    #[inline]
    fn poll_fill_buf(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        Poll::Ready(Ok(&[]))
    }

    #[inline]
    fn consume(self: Pin<&mut Self>, _: usize) {}
}

impl fmt::Debug for Empty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Empty { .. }")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<Empty>();
    }
}
