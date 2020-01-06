use crate::io::AsyncRead;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    /// An async reader which yields one byte over and over and over and over and
    /// over and...
    ///
    /// This struct is generally created by calling [`repeat`][repeat]. Please
    /// see the documentation of `repeat()` for more details.
    ///
    /// This is an asynchronous version of [`std::io::Repeat`][std].
    ///
    /// [repeat]: fn.repeat.html
    /// [std]: std::io::Repeat
    #[derive(Debug)]
    pub struct Repeat {
        byte: u8,
    }

    /// Creates an instance of an async reader that infinitely repeats one byte.
    ///
    /// All reads from this reader will succeed by filling the specified buffer with
    /// the given byte.
    ///
    /// This is an asynchronous version of [`std::io::repeat`][std].
    ///
    /// [std]: std::io::repeat
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::{self, AsyncReadExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut buffer = [0; 3];
    ///     io::repeat(0b101).read_exact(&mut buffer).await.unwrap();
    ///     assert_eq!(buffer, [0b101, 0b101, 0b101]);
    /// }
    /// ```
    pub fn repeat(byte: u8) -> Repeat {
        Repeat { byte }
    }
}

impl AsyncRead for Repeat {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        for byte in &mut *buf {
            *byte = self.byte;
        }
        Poll::Ready(Ok(buf.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_unpin() {
        crate::is_unpin::<Repeat>();
    }
}
