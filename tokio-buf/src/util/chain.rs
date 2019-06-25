use crate::BufStream;

use either::Either;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_futures::ready;

/// A buf stream that sequences two buf streams together.
///
/// `Chain` values are produced by the `chain` function on `BufStream`.
#[derive(Debug)]
pub struct Chain<T, U> {
    left: Option<T>,
    right: U,
}

impl<T, U> Chain<T, U> {
    pub(crate) fn new(left: T, right: U) -> Chain<T, U> {
        Chain {
            left: Some(left),
            right,
        }
    }
}

impl<T, U> BufStream for Chain<T, U>
where
    T: BufStream,
    U: BufStream<Error = T::Error>,
{
    type Item = Either<T::Item, U::Item>;
    type Error = T::Error;

    fn poll_buf(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        if let Some(ref mut stream) = self.left {
            let res = ready!(stream.poll_buf(cx))?;

            if res.is_some() {
                return Ok(res.map(Either::Left).into());
            }
        }

        self.left = None;

        let res = ready!(self.right.poll_buf())?;
        Ok(res.map(Either::Right).into())
    }
}
