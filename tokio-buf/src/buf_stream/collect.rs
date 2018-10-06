use super::{BufStream, FromBufStream};

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
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.stream.poll_buf()) {
                Some(mut buf) => {
                    let builder = self.builder.as_mut().expect("cannot poll after done");

                    U::extend(builder, &mut buf);
                }
                None => {
                    let builder = self.builder.take().expect("cannot poll after done");
                    let value = U::build(builder);
                    return Ok(value.into());
                }
            }
        }
    }
}
