use crate::BufStream;

use bytes::Buf;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Limits the stream to a maximum amount of data.
#[derive(Debug)]
pub struct Limit<T> {
    stream: T,
    remaining: u64,
}

/// Errors returned from `Limit`.
#[derive(Debug)]
pub struct LimitError<T> {
    /// When `None`, limit was reached
    inner: Option<T>,
}

impl<T> Limit<T> {
    pub(crate) fn new(stream: T, amount: u64) -> Limit<T> {
        Limit {
            stream,
            remaining: amount,
        }
    }
}

impl<T> BufStream for Limit<T>
where
    T: BufStream,
{
    type Item = T::Item;
    type Error = LimitError<T::Error>;

    fn poll_buf(&mut self) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        if self.stream.size_hint().lower() > self.remaining {
            return Err(LimitError { inner: None });
        }

        let res = self
            .stream
            .poll_buf(cx)
            .map_err(|err| LimitError { inner: Some(err) });

        match res {
            Poll::Ready(Ok(Some(ref buf))) => {
                if buf.remaining() as u64 > self.remaining {
                    self.remaining = 0;
                    return Poll::Ready(Err(LimitError { inner: None }));
                }

                self.remaining -= buf.remaining() as u64;
            }
            _ => {}
        }

        res
    }
}

// ===== impl LimitError =====

impl<T> LimitError<T> {
    /// Returns `true` if the error was caused by polling the stream.
    pub fn is_stream_err(&self) -> bool {
        self.inner.is_some()
    }

    /// Returns `true` if the stream reached its limit.
    pub fn is_limit_err(&self) -> bool {
        self.inner.is_none()
    }
}
