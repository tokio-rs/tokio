use super::FromBufStream;
use BufStream;

use futures::{Future, Poll};

/// Consumes a buf stream, collecting the data into a single byte container.
///
/// `Collect` values are produced by `BufStream::collect`.
#[derive(Debug)]
pub struct Collect<T, U>
where
    T: BufStream,
    U: FromBufStream<T::Item>,
{
    stream: T,
    builder: Option<U::Builder>,
}

/// Errors returned from `Collect` future.
#[derive(Debug)]
pub struct CollectError<T, U> {
    inner: Error<T, U>,
}

#[derive(Debug)]
enum Error<T, U> {
    Stream(T),
    Collect(U),
}

impl<T, U> Collect<T, U>
where
    T: BufStream,
    U: FromBufStream<T::Item>,
{
    pub(crate) fn new(stream: T) -> Collect<T, U> {
        let builder = U::builder(&stream.size_hint());

        Collect {
            stream,
            builder: Some(builder),
        }
    }
}

impl<T, U> Future for Collect<T, U>
where
    T: BufStream,
    U: FromBufStream<T::Item>,
{
    type Item = U;
    type Error = CollectError<T::Error, U::Error>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let res = self.stream.poll_buf().map_err(|err| {
                let inner = Error::Stream(err);
                CollectError { inner }
            });

            match try_ready!(res) {
                Some(mut buf) => {
                    let builder = self.builder.as_mut().expect("cannot poll after done");

                    U::extend(builder, &mut buf, &self.stream.size_hint()).map_err(|err| {
                        let inner = Error::Collect(err);
                        CollectError { inner }
                    })?;
                }
                None => {
                    let builder = self.builder.take().expect("cannot poll after done");
                    let value = U::build(builder).map_err(|err| {
                        let inner = Error::Collect(err);
                        CollectError { inner }
                    })?;
                    return Ok(value.into());
                }
            }
        }
    }
}

// ===== impl CollectError =====

impl<T, U> CollectError<T, U> {
    /// Returns `true` if the error was caused by polling the stream.
    pub fn is_stream_err(&self) -> bool {
        match self.inner {
            Error::Stream(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if the error happened while collecting the data.
    pub fn is_collect_err(&self) -> bool {
        match self.inner {
            Error::Collect(_) => true,
            _ => false,
        }
    }
}
