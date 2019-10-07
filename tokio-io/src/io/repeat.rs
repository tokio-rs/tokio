use crate::AsyncRead;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// An async reader which yields one byte over and over and over and over and
/// over and...
///
/// This struct is generally created by calling [`repeat`][repeat]. Please
/// see the documentation of `repeat()` for more details.
///
/// This is an asynchronous version of [`std::io::Repeat`][std].
///
/// [repeat]: fn.repeat.html
/// [std]: https://doc.rust-lang.org/std/io/struct.Repeat.html
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
/// # Examples
///
/// ```
/// # use tokio_io::{self as io, AsyncReadExt};
/// # async fn dox() {
/// let mut buffer = [0; 3];
/// io::repeat(0b101).read_exact(&mut buffer).await.unwrap();
/// assert_eq!(buffer, [0b101, 0b101, 0b101]);
/// # }
/// ```
///
/// [std]: https://doc.rust-lang.org/std/io/fn.repeat.html
pub fn repeat(byte: u8) -> Repeat {
    Repeat { byte }
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
