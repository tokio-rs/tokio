use std::{
    io,
    num::NonZeroU8,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::ready;
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, ReadBuf};

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

#[allow(clippy::unusual_byte_groupings)]
fn is_continuation_byte(byte: u8) -> bool {
    (0b10_000000..=0b10_111111).contains(&byte)
}

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
            // increment `cursor` and move on to the next iteration
            None | Some(1) => cursor += 1,
            // A valid first byte to a UTF-8 sequence that we expect to take
            // either 2, 3, or 4 bytes. Here, we must do additional checking to
            // verify if this is a valid UTF-8 sequence
            Some(num_bytes @ 2 | num_bytes @ 3 | num_bytes @ 4) => {
                // We try to go through the next [num_bytes - 1] bytes,
                // validating each one in turn. If there are not enough bytes,
                // we return `cursor`, because we have an incomplete UTF-8
                // sequence as far as we can tell. If any byte is invalid, we
                // increment `cursor` up to the invalid byte and break out of
                // the loop, starting the next iteration of the outer loop. If
                // we have all valid bytes, we increment `cursor` to the end of
                // the valid UTF-8 sequence that we just found
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
    ScrapBetweenReads([u8; 3], usize),
    NoScrap,
}

pin_project! {
    /// UTF-8 reader
    #[derive(Debug, Clone)]
    pub struct Utf8Reader<R> {
        #[pin]
        inner: R,
        scrap_state: ScrapState,
    }
}

impl<R> Utf8Reader<R> {
    /// Create new
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            scrap_state: ScrapState::NoScrap,
        }
    }

    /// Into inner
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
            ScrapState::ScrapBetweenReads(scrap, len) => {
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
                    *me.scrap_state = ScrapState::ScrapBetweenReads(scrap, scrap_len);
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
