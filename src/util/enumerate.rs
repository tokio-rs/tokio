use futures::{Async, Poll, Stream};

/// A stream combinator which combines the yields the current item
/// plus its count starting from 0.
///
/// This structure is produced by the `Stream::enumerate` method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Enumerate<T> {
    stream: T,
    count: usize,
}

impl<T> Enumerate<T> {
    pub fn new(stream: T) -> Self {
        Self { stream, count: 0 }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &T {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.stream
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> T {
        self.stream
    }
}

impl<T> Stream for Enumerate<T>
where
    T: Stream,
{
    type Item = (usize, T::Item);
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, T::Error> {
        match try_ready!(self.stream.poll()) {
            Some(item) => {
                let ret = Some((self.count, item));
                self.count += 1;
                Ok(Async::Ready(ret))
            }
            None => return Ok(Async::Ready(None)),
        }
    }
}
