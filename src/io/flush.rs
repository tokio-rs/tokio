use std::io::{self, Write};

use futures::{Poll, Future, Async};

/// A future used to fully flush an I/O object.
///
/// Resolves to the underlying I/O object once the flush operation is complete.
///
/// Created by the [`flush`] function.
///
/// [`flush`]: fn.flush.html
pub struct Flush<A> {
    a: Option<A>,
}

/// Creates a future which will entirely flush an I/O object and then yield the
/// object itself.
///
/// This function will consume the object provided if an error happens, and
/// otherwise it will repeatedly call `flush` until it sees `Ok(())`, scheduling
/// a retry if `WouldBlock` is seen along the way.
pub fn flush<A>(a: A) -> Flush<A>
    where A: Write,
{
    Flush {
        a: Some(a),
    }
}

impl<A> Future for Flush<A>
    where A: Write,
{
    type Item = A;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<A, io::Error> {
        try_nb!(self.a.as_mut().unwrap().flush());
        Ok(Async::Ready(self.a.take().unwrap()))
    }
}

