//! Use streams with `async` / `await`.

mod next;

pub use self::next::Next;

use futures::Stream;

/// An extension trait which adds utility methods to `Stream` types.
pub trait StreamExt: Stream {
    /// Creates a future that resolves to the next item in the stream.
    ///
    /// # Examples
    ///
    /// ```edition2018
    /// #![feature(await_macro, async_await)]
    /// tokio::run_async(async {
    /// // The extension trait can also be imported with
    /// // `use tokio::prelude::*`.
    /// use tokio::prelude::{stream, StreamAsyncExt};
    ///
    /// let mut stream = stream::iter_ok::<_, ()>(1..3);
    ///
    /// assert_eq!(await!(stream.next()), Some(Ok(1)));
    /// assert_eq!(await!(stream.next()), Some(Ok(2)));
    /// assert_eq!(await!(stream.next()), Some(Ok(3)));
    /// assert_eq!(await!(stream.next()), None);
    /// });
    /// ```
    fn next(&mut self) -> Next<Self>
    where
        Self: Sized + Unpin,
    {
        Next::new(self)
    }
}

impl<T: Stream> StreamExt for T {}
