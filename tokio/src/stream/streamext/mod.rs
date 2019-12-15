use core::future::Future;
use crate::stream::Stream;

mod collect;
mod filter;
mod filter_map;
mod map;
mod next;

#[allow(unreachable_pub)]
pub use {
    collect::Collect,
    filter::Filter,
    filter_map::FilterMap,
    map::Map,
    next::Next,
};

/// An extension trait for `Stream`s that provides a variety of convenient
/// combinator functions.
pub trait StreamExt: Stream {
    /// Creates a future that resolves to the next item in the stream.
    ///
    /// Note that because `next` doesn't take ownership over the stream,
    /// the [`Stream`] type must be [`Unpin`]. If you want to use `next` with a
    /// [`!Unpin`](Unpin) stream, you'll first have to pin the stream. This can
    /// be done by boxing the stream using [`Box::pin`] or
    /// pinning it to the stack using the `pin_mut!` macro from the `pin_utils`
    /// crate.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let mut stream = stream::iter(1..=3);
    ///
    /// assert_eq!(stream.next().await, Some(1));
    /// assert_eq!(stream.next().await, Some(2));
    /// assert_eq!(stream.next().await, Some(3));
    /// assert_eq!(stream.next().await, None);
    /// # }
    /// ```
    fn next(&mut self) -> Next<'_, Self>
    where
        Self: Unpin,
    {
        Next::new(self)
    }

    /// Maps this stream's items to a different type, returning a new stream of
    /// the resulting type.
    ///
    /// The provided closure is executed over all elements of this stream as
    /// they are made available. It is executed inline with calls to
    /// [`poll_next`](Stream::poll_next).
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to the existing `map` methods in the
    /// standard library.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let stream = stream::iter(1..=3);
    /// let stream = stream.map(|x| x + 3);
    ///
    /// assert_eq!(vec![4, 5, 6], stream.collect::<Vec<_>>().await);
    /// # } 
    /// ```
    fn map<T, F>(self, f: F) -> Map<Self, F>
    where
        F: FnMut(Self::Item) -> T,
        Self: Sized,
    {
        Map::new(self, f)
    }

    /// Filters the values produced by this stream according to the provided
    /// asynchronous predicate.
    ///
    /// As values of this stream are made available, the provided predicate `f`
    /// will be run against them. If the predicate returns a `Future` which
    /// resolves to `true`, then the stream will yield the value, but if the
    /// predicate returns a `Future` which resolves to `false`, then the value
    /// will be discarded and the next value will be produced.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to the existing `filter` methods in the
    /// standard library.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use futures::future;
    /// use futures::stream::{self, StreamExt};
    ///
    /// let stream = stream::iter(1..=10);
    /// let evens = stream.filter(|x| future::ready(x % 2 == 0));
    ///
    /// assert_eq!(vec![2, 4, 6, 8, 10], evens.collect::<Vec<_>>().await);
    /// # }
    /// ```
    fn filter<Fut, F>(self, f: F) -> Filter<Self, Fut, F>
    where
        F: FnMut(&Self::Item) -> Fut,
        Fut: Future<Output = bool>,
        Self: Sized,
    {
        Filter::new(self, f)
    }

    /// Filters the values produced by this stream while simultaneously mapping
    /// them to a different type according to the provided asynchronous closure.
    ///
    /// As values of this stream are made available, the provided function will
    /// be run on them. If the future returned by the predicate `f` resolves to
    /// [`Some(item)`](Some) then the stream will yield the value `item`, but if
    /// it resolves to [`None`] then the next value will be produced.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to the existing `filter_map` methods in
    /// the standard library.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let stream = stream::iter(1..=10);
    /// let evens = stream.filter_map(|x| async move {
    ///     if x % 2 == 0 { Some(x + 1) } else { None }
    /// });
    ///
    /// assert_eq!(vec![3, 5, 7, 9, 11], evens.collect::<Vec<_>>().await);
    /// # }
    /// ```
    fn filter_map<Fut, T, F>(self, f: F) -> FilterMap<Self, Fut, F>
    where
        F: FnMut(Self::Item) -> Fut,
        Fut: Future<Output = Option<T>>,
        Self: Sized,
    {
        FilterMap::new(self, f)
    }

    /// Transforms a stream into a collection, returning a
    /// future representing the result of that computation.
    ///
    /// The returned future will be resolved when the stream terminates.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::sync::mpsc;
    /// use tokio::stream::StreamExt;
    /// use std::thread;
    ///
    /// let (tx, rx) = mpsc::unbounded_channel();
    ///
    /// thread::spawn(move || {
    ///     for i in 1..=5 {
    ///         tx.send(i).unwrap();
    ///     }
    /// });
    ///
    /// let output = rx.collect::<Vec<i32>>().await;
    /// assert_eq!(output, vec![1, 2, 3, 4, 5]);
    /// # }
    /// ```
    fn collect<C: Default + Extend<Self::Item>>(self) -> Collect<Self, C>
    where
        Self: Sized,
    {
        Collect::new(self)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
