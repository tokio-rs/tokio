use crate::AsyncRead;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

// An async reader which is always at EOF.
///
/// This struct is generally created by calling [`empty`]. Please see
/// the documentation of [`empty()`][`empty`] for more details.
///
/// This is an asynchronous version of `std::io::empty`.
///
/// [`empty`]: fn.empty.html
pub struct Empty {
    _p: (),
}

/// Creates a new empty async reader.
///
/// All reads from the returned reader will return `Poll::Ready(Ok(0))`.
///
/// This is an asynchronous version of `std::io::empty`.
///
/// # Examples
///
/// A slightly sad example of not reading anything into a buffer:
///
/// ```rust
/// # use tokio_io::{self as io, AsyncRead};
/// # async fn dox() {
/// let mut buffer = String::new();
/// io::empty().read_to_string(&mut buffer).await.unwrap();
/// assert!(buffer.is_empty());
/// # }
/// ```
pub fn empty() -> Empty {
    Empty { _p: () }
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
