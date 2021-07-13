use std::{
    hint::unreachable_unchecked,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

/// Given a slice of bytes, determine how much of it is complete UTF-8. If there
/// is an incomplete UTF-8 sequence at the end, exclude that from the length
/// returned. Any invalid bytes are added to the length (in essence, ignored).
/// The intent is to pass the slice of bytes to the user and let them handle any
/// invalid bytes in the way they most prefer.
fn len_of_complete_or_invalid_utf8_bytes(slice: &[u8]) -> usize {
    let mut index = 0;
    loop {
        match std::str::from_utf8(&slice[index..]) {
            Ok(_) => break slice.len(),
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                match err.error_len() {
                    // Reached an unexpected end, return where we're valid up
                    // to. We have to add `index` because `valid_up_to` only
                    // pertains to `&slice[index..]`, but we want it relative to
                    // `slice`. `index` will always exclude either valid UTF-8
                    // or invalid bytes from `slice`, but not partial UTF-8
                    // sequences at the end.
                    None => break index + valid_up_to,
                    // Invalid byte, ignore it
                    Some(len) => index += valid_up_to + len,
                }
            }
        }
    }
}

pin_project! {
    /// An asynchronous UTF-8 text reader.
    ///
    /// `Utf8Reader` wraps an [`AsyncRead`] and/or
    /// [`AsyncBufRead`](https://docs.rs/tokio/latest/tokio/io/trait.AsyncBufRead.html),
    /// withholding a partial UTF-8 sequence at the end if one is present. That
    /// is, with a multi-byte UTF-8 sequence, if the underlying reader has only
    /// supplied some of the necessary bytes, `Utf8Reader` saves that partial
    /// UTF-8 sequence for later, but yields the rest immediately after the
    /// inner reader yields the bytes.
    ///
    /// # Invalid UTF-8
    /// All `Utf8Reader` does is parse bytes yielded by the inner reader for
    /// correct UTF-8 structure, withholding any incomplete UTF-8 sequences. It
    /// does not check that all the actual Unicode code points are valid, nor
    /// does it handle invalid UTF-8 structure. In both scenarios, the bytes are
    /// ignored and dumbly forwarded along to the user. In handling invalid
    /// bytes or Unicode code points, one can use any of the `from_utf8_*`
    /// functions to determine how their program should respond to invalid
    /// bytes.
    ///
    /// # Full buffer
    /// If the buffer supplied to the [`AsyncRead`] implementation runs out of
    /// room, the above guarentees do not apply. As many bytes as possible will
    /// be put into the supplied buffer, regardless of whether they are complete
    /// UTF-8 or not.
    ///
    /// [`AsyncRead`]: https://docs.rs/tokio/latest/tokio/io/trait.AsyncRead.html
    ///
    /// # Examples
    /// ```
    /// // for `from_utf8`
    /// use std::str;
    ///
    /// use tokio::io::AsyncReadExt;
    /// use tokio_util::io::Utf8Reader;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # const SOME_SOURCE: &[u8] = "😀😬😁😂😃".as_bytes();
    /// // Some `AsyncRead` that yields the bytes that make up the string
    /// // "😀😬😁😂😃". This is helpful for demonstration purposes because each
    /// // emoji takes up 4 bytes
    /// let source = SOME_SOURCE;
    /// # struct Source<'a> {
    /// #     inner: &'a [u8],
    /// #     go_from_beginning: bool,
    /// # }
    /// # impl<'a> tokio::io::AsyncRead for Source<'a> {
    /// #     fn poll_read(
    /// #         mut self: std::pin::Pin<&mut Self>,
    /// #         cx: &mut std::task::Context<'_>,
    /// #         buf: &mut tokio::io::ReadBuf<'_>
    /// #     ) -> std::task::Poll<std::io::Result<()>> {
    /// #         buf.put_slice(
    /// #             if self.go_from_beginning {
    /// #                 self.go_from_beginning = false;
    /// #                 &self.inner[..9]
    /// #             } else {
    /// #                 &self.inner[9..]
    /// #             }
    /// #         );
    /// #         std::task::Poll::Ready(Ok(()))
    /// #     }
    /// # }
    /// let mut buffer = [0; 25];
    /// # let source = Source { inner: source, go_from_beginning: true };
    /// let mut reader = Utf8Reader::new(source);
    ///
    /// // For whatever reason, the underlying reader was only able to read the
    /// // first nine bytes, which is the first two emojis plus the first byte
    /// // of the third. Thus, we expect to only get the first two emojis in
    /// // `buffer`.
    /// let bytes_read = reader.read(&mut buffer).await?;
    /// assert_eq!(bytes_read, 8);
    /// assert_eq!(str::from_utf8(&buffer[..bytes_read])?, "😀😬");
    ///
    /// // ... later on ...
    ///
    /// // `reader` now gives us the leftover byte from last time because the
    /// // rest of the bytes that make up the third emoji have been supplied
    /// // from the inner reader, as well as the fourth and fifth emojis. We
    /// // have to slice `buffer` by `bytes_read` in order to not overwrite what
    /// // was read in the previous read
    /// let bytes_read_2 = reader.read(&mut buffer[bytes_read..]).await?;
    /// assert_eq!(bytes_read_2, 12);
    /// assert_eq!(str::from_utf8(&buffer[bytes_read..][..bytes_read_2])?, "😁😂😃");
    /// assert_eq!(str::from_utf8(&buffer[..(bytes_read + bytes_read_2)])?, "😀😬😁😂😃");
    /// # Ok(())
    /// # }
    /// ```
    #[derive(Debug, Clone)]
    pub struct Utf8Reader<R> {
        #[pin]
        inner: R,
        read_scrap: Option<([u8; 3], usize)>,
        read_buf_scrap: Option<([u8; 4], usize)>,
    }
}

impl<R> Utf8Reader<R> {
    /// Create a new `Utf8Reader` from an underlying reader.
    ///
    /// Note that the generic parameter `R` is not constrained by either
    /// [`AsyncRead`] or [`AsyncBufRead`], but in order to be able to use a
    /// `Utf8Reader` as one (in order for `Utf8Reader` to implement
    /// [`AsyncRead`] and/or [`AsyncBufRead`]), `R` must implement one or both
    /// of those traits.
    ///
    /// [`AsyncRead`]: https://docs.rs/tokio/latest/tokio/io/trait.AsyncRead.html
    /// [`AsyncBufRead`]: https://docs.rs/tokio/latest/tokio/io/trait.AsyncBufRead.html
    ///
    /// # Examples
    /// ```
    /// use tokio_util::io::Utf8Reader;
    ///
    /// # const INNER_READER: () = ();
    /// let utf8_reader = Utf8Reader::new(INNER_READER);
    /// ```
    pub const fn new(inner: R) -> Self {
        Self {
            inner,
            read_scrap: None,
            read_buf_scrap: None,
        }
    }

    /// Get the inner reader out of the `Utf8Reader`, destroying the
    /// `Utf8Reader` in the process.
    ///
    /// # Examples
    /// ```
    /// use tokio_util::io::Utf8Reader;
    ///
    /// # const INNER_READER: () = ();
    /// let utf8_reader = Utf8Reader::new(INNER_READER);
    ///
    /// // ... later on ...
    /// let inner_reader = utf8_reader.into_inner();
    /// ```
    pub fn into_inner(self) -> R {
        self.inner
    }

    fn handle_read_buf(
        buf: &mut ReadBuf<'_>,
        read_scrap: &mut Option<([u8; 3], usize)>,
    ) -> Result<(), ()> {
        match len_of_complete_or_invalid_utf8_bytes(buf.filled()) {
            0 => {
                debug_assert!(buf.filled().len() < 4, "it shouldn't be possible to not have valid UTF-8 to yield with at least 4 bytes available");
                *read_scrap = make_read_scrap(buf.filled());
                buf.set_filled(0);
                Err(())
            }
            len_of_valid_utf8 => {
                *read_scrap = make_read_scrap(&buf.filled()[len_of_valid_utf8..]);
                buf.set_filled(len_of_valid_utf8);
                Ok(())
            }
        }
    }
}

impl<R> From<R> for Utf8Reader<R> {
    fn from(inner: R) -> Self {
        Self::new(inner)
    }
}

impl<R: AsyncRead> AsyncRead for Utf8Reader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.as_mut().project();
        let filled_before = buf.filled().len();
        match me.read_scrap {
            Some((scrap, len)) => {
                let to_fill = std::cmp::min(*len, buf.remaining());
                buf.put_slice(&scrap[..to_fill]);
                if buf.remaining() == 0 {
                    *me.read_scrap = make_read_scrap(&scrap[to_fill..*len]);
                    return Poll::Ready(Ok(()));
                }
                match me.inner.poll_read(cx, buf) {
                    Poll::Pending => {
                        buf.set_filled(filled_before);
                        Poll::Pending
                    }
                    Poll::Ready(res) => {
                        if let err @ Err(_) = res {
                            buf.set_filled(filled_before);
                            return Poll::Ready(err);
                        }

                        // At this point, reading from the inner reader has
                        // completed successfully. And since
                        // `buf.remaining()` != 0, we know the entire scrap has
                        // been used, so we can safely overrwrite it.

                        match Self::handle_read_buf(buf, me.read_scrap) {
                            Ok(()) => Poll::Ready(Ok(())),
                            Err(()) => self.poll_read(cx, buf),
                        }
                    }
                }
            }
            None => {
                ready!(me.inner.poll_read(cx, buf))?;

                match Self::handle_read_buf(buf, me.read_scrap) {
                    Ok(()) => Poll::Ready(Ok(())),
                    Err(()) => self.poll_read(cx, buf),
                }
            }
        }
    }
}

impl<R: AsyncBufRead> AsyncBufRead for Utf8Reader<R> {
    fn poll_fill_buf(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let me = self.as_mut().project();
        match me.read_buf_scrap {
            Some((scrap, len)) => match len_of_complete_or_invalid_utf8_bytes(scrap) {
                0 => {
                    let slice = ready!(me.inner.poll_fill_buf(cx))?;
                    let to_fill = std::cmp::min(4 - *len, slice.len());
                    scrap[*len..to_fill].copy_from_slice(&slice[..to_fill]);
                    match len_of_complete_or_invalid_utf8_bytes(scrap) {
                        0 => {
                            self.as_mut().project().inner.consume(to_fill);
                            self.poll_fill_buf(cx)
                        }
                        len_of_valid_utf8 => {
                            let to_consume = len_of_valid_utf8 - *len;
                            *len = len_of_valid_utf8;
                            // We must re-project `self` because otherwise the borrow
                            // checker would complain about us borrowing from the projection
                            // created from a borrowed `self`. We must borrow `self` because
                            // we want to be able to use `self` for recursion.
                            let me = self.project();
                            me.inner.consume(to_consume);
                            Poll::Ready(Ok(&me
                                .read_buf_scrap
                                .as_ref()
                                // SAFETY: the above match asserts that
                                // `self.read_buf_scrap` is `Some`
                                .unwrap_or_else(|| unsafe { unreachable_unchecked() })
                                .0[..len_of_valid_utf8]))
                        }
                    }
                }
                // We must re-project `self` because otherwise the borrow
                // checker would complain about us borrowing from the projection
                // created from a borrowed `self`. We must borrow `self` because
                // we want to be able to use `self` for recursion.
                len_of_valid_utf8 => Poll::Ready(Ok(&self
                    .project()
                    .read_buf_scrap
                    .as_ref()
                    // SAFETY: the above match asserts that
                    // `self.read_buf_scrap` is `Some`
                    .unwrap_or_else(|| unsafe { unreachable_unchecked() })
                    .0[..len_of_valid_utf8])),
            },
            None => {
                let slice = ready!(me.inner.poll_fill_buf(cx))?;
                match (
                    len_of_complete_or_invalid_utf8_bytes(slice),
                    slice.is_empty(),
                ) {
                    (0, false) => {
                        *me.read_buf_scrap = make_read_buf_scrap(slice);
                        let to_consume = slice.len();
                        self.as_mut().project().inner.consume(to_consume);
                        self.poll_fill_buf(cx)
                    }
                    // if `slice.is_empty()`, `len_of_valid_utf8` will be 0
                    (len_of_valid_utf8, _) => Poll::Ready(Ok(&slice[..len_of_valid_utf8])),
                }
            }
        }
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        match me.read_buf_scrap {
            Some((scrap, len)) => *me.read_buf_scrap = make_read_buf_scrap(&scrap[amt..*len]),
            None => me.inner.consume(amt),
        }
    }
}

macro_rules! scrap_functions {
    ($($name:ident => $size:literal);+ $(;)?) => {
        $(fn $name(scrap: &[u8]) -> Option<([u8; $size], usize)> {
            if scrap.len() == 0 {
                return None;
            }

            let mut result = [0; $size];
            debug_assert!(scrap.len() <= $size, "scrap is too long");
            let len = std::cmp::min(scrap.len(), 3);
            result[..len].copy_from_slice(&scrap[..len]);

            Some((result, len))
        })+
    };
}

scrap_functions! {
    make_read_scrap => 3;
    make_read_buf_scrap => 4;
}
