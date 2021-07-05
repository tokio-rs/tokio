use std::{
    io,
    num::NonZeroU8,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

/// Given the first byte in a UTF-8 sequence, determine how many bytes total,
/// including the one passed in, to expect to be in the proceeding UTF-8
/// sequence. Returns `None` if the byte is not a valid first UTF-8 byte.
#[allow(clippy::unusual_byte_groupings)]
fn num_utf8_bytes(byte: u8) -> Option<NonZeroU8> {
    match byte {
        // SAFETY: all integers passed in are hard-coded non-zero integers
        0b0_0000000..=0b0_1111111 => Some(unsafe { NonZeroU8::new_unchecked(1) }),
        0b110_00000..=0b110_11111 => Some(unsafe { NonZeroU8::new_unchecked(2) }),
        0b1110_0000..=0b1110_1111 => Some(unsafe { NonZeroU8::new_unchecked(3) }),
        0b11110_000..=0b11110_111 => Some(unsafe { NonZeroU8::new_unchecked(4) }),
        _ => None,
    }
}

/// Determine whether the given byte is a continuation of a UTF-8 sequence or
/// not. That is, whether the given byte starts with the bits `01` or not.
#[allow(clippy::unusual_byte_groupings)]
fn is_continuation_byte(byte: u8) -> bool {
    (0b10_000000..=0b10_111111).contains(&byte)
}

/// Given a slice of bytes, determine how much of it is complete UTF-8. If there
/// is an incomplete UTF-8 sequence at the end, exclude that from the length
/// returned. Any invalid bytes are added to the length (in essence, ignored).
/// The intent is to pass the slice of bytes to the user and let them handle any
/// invalid bytes in the way they most prefer.
fn len_of_complete_or_invalid_utf8_bytes(slice: &[u8]) -> usize {
    let mut iter = slice.iter();
    let mut cursor = 0;
    while cursor < slice.len() {
        let next = match iter.next() {
            Some(&byte) => byte,
            None => break,
        };
        match num_utf8_bytes(next).map(|n| n.get()) {
            // Either an invalid byte or ASCII character. In either case, we
            // increment `cursor` and move on to the next iteration.
            None | Some(1) => cursor += 1,
            // A valid first byte to a UTF-8 sequence that we expect to take
            // either 2, 3, or 4 bytes. Here, we must do additional checking to
            // verify if this is a valid UTF-8 sequence.
            Some(num_bytes @ 2) | Some(num_bytes @ 3) | Some(num_bytes @ 4) => {
                // We try to go through the next [num_bytes - 1] bytes,
                // validating each one in turn. If there are not enough bytes,
                // we return `cursor`, because we have an incomplete UTF-8
                // sequence as far as we can tell. If any byte is invalid, we
                // increment `cursor` up to the invalid byte and break out of
                // the loop, starting the next iteration of the outer loop. If
                // we have all valid bytes, we increment `cursor` to the end of
                // the valid UTF-8 sequence that we just found.
                for i in 2..=num_bytes {
                    match iter.next() {
                        Some(&next) => {
                            if is_continuation_byte(next) {
                                if i == num_bytes {
                                    cursor += num_bytes as usize;
                                }
                            } else {
                                cursor += i as usize;
                                break;
                            }
                        }
                        None => return cursor,
                    }
                }
            }
            _ => unreachable!(),
        }
    }
    cursor
}

#[derive(Debug, Clone, Copy)]
enum ScrapState {
    // This should only be used after the underlying reader has returned `Ready`
    // and before it has been polled again (in between reads).
    Scrap([u8; 3], usize),
    NoScrap,
}

pin_project! {
    /// An asynchronous UTF-8 text reader.
    ///
    /// `Utf8Reader` wraps an
    /// [`AsyncRead`](https://docs.rs/tokio/latest/tokio/io/trait.AsyncRead.html)
    /// and/or
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
    /// # let SOME_SOURCE = "😀😬😁😂😃".as_bytes();
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
        scrap_state: ScrapState,
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
    /// # let INNER_READER = ();
    /// let utf8_reader = Utf8Reader::new(INNER_READER);
    /// ```
    pub const fn new(inner: R) -> Self {
        Self {
            inner,
            scrap_state: ScrapState::NoScrap,
        }
    }

    /// Get the inner reader out of the `Utf8Reader`, destroying the
    /// `Utf8Reader` in the process.
    ///
    /// # Examples
    /// ```
    /// use tokio_util::io::Utf8Reader;
    ///
    /// # let INNER_READER = ();
    /// let utf8_reader = Utf8Reader::new(INNER_READER);
    ///
    /// // ... later on ...
    /// let inner_reader = utf8_reader.into_inner();
    /// ```
    pub fn into_inner(self) -> R {
        self.inner
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
        match me.scrap_state {
            ScrapState::Scrap(scrap, len) => {
                buf.put_slice(&scrap[..*len]);
                *me.scrap_state = ScrapState::NoScrap;
                self.poll_read(cx, buf)
            }
            ScrapState::NoScrap => {
                ready!(me.inner.poll_read(cx, buf))?;

                let filled = buf.filled();
                let len_of_complete_utf8 = len_of_complete_or_invalid_utf8_bytes(filled);
                if len_of_complete_utf8 != filled.len() {
                    // should not be greater than 3, as
                    // `len_of_complete_or_invalid_utf8_bytes` should only ever
                    // return a length that's at most 3 less than the length of
                    // the byte-string
                    let scrap_len = filled.len() - len_of_complete_utf8;
                    let mut scrap = [0; 3];
                    scrap[..scrap_len].copy_from_slice(&filled[len_of_complete_utf8..]);
                    *me.scrap_state = ScrapState::Scrap(scrap, scrap_len);
                    // shouldn't panic, because `len_of_complete_utf8` will be less than or
                    // equal to `filled.len()`, which is guarenteed to be less than the
                    // initialized portion of `buf`
                    buf.set_filled(len_of_complete_utf8);
                }

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<R: AsyncBufRead> AsyncBufRead for Utf8Reader<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let slice = ready!(self.project().inner.poll_fill_buf(cx))?;
        Poll::Ready(Ok(&slice[..len_of_complete_or_invalid_utf8_bytes(slice)]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().inner.consume(amt)
    }
}
