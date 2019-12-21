//! Stream utilities for Tokio.
//!
//! A `Stream` is an asynchronous sequence of values. It can be thought of as an asynchronous version of the standard library's `Iterator` trait.
//!
//! This module provides helpers to work with them.

mod filter;
use filter::Filter;

mod filter_map;
use filter_map::FilterMap;

mod iter;
pub use iter::{iter, Iter};

mod map;
use map::Map;

mod next;
use next::Next;

pub use futures_core::Stream;

/// An extension trait for `Stream`s that provides a variety of convenient
/// combinator functions.
pub trait StreamExt: Stream {
    /// Creates a future that resolves to the next item in the stream.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn next(&mut self) -> Option<Self::Item>;
    /// ```
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
    /// let mut stream = stream.map(|x| x + 3);
    ///
    /// assert_eq!(stream.next().await, Some(4));
    /// assert_eq!(stream.next().await, Some(5));
    /// assert_eq!(stream.next().await, Some(6));
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
    /// predicate.
    ///
    /// As values of this stream are made available, the provided predicate `f`
    /// will be run against them. If the predicate
    /// resolves to `true`, then the stream will yield the value, but if the
    /// predicate resolves to `false`, then the value
    /// will be discarded and the next value will be produced.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to [`Iterator::filter`] method in the
    /// standard library.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let stream = stream::iter(1..=8);
    /// let mut evens = stream.filter(|x| x % 2 == 0);
    ///
    /// assert_eq!(Some(2), evens.next().await);
    /// assert_eq!(Some(4), evens.next().await);
    /// assert_eq!(Some(6), evens.next().await);
    /// assert_eq!(Some(8), evens.next().await);
    /// assert_eq!(None, evens.next().await);
    /// # }
    /// ```
    fn filter<F>(self, f: F) -> Filter<Self, F>
    where
        F: FnMut(&Self::Item) -> bool,
        Self: Sized,
    {
        Filter::new(self, f)
    }

    /// Filters the values produced by this stream while simultaneously mapping
    /// them to a different type according to the provided closure.
    ///
    /// As values of this stream are made available, the provided function will
    /// be run on them. If the predicate `f` resolves to
    /// [`Some(item)`](Some) then the stream will yield the value `item`, but if
    /// it resolves to [`None`] then the next value will be produced.
    ///
    /// Note that this function consumes the stream passed into it and returns a
    /// wrapped version of it, similar to [`Iterator::filter_map`] method in the
    /// standard library.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let stream = stream::iter(1..=8);
    /// let mut evens = stream.filter_map(|x| {
    ///     if x % 2 == 0 { Some(x + 1) } else { None }
    /// });
    ///
    /// assert_eq!(Some(3), evens.next().await);
    /// assert_eq!(Some(5), evens.next().await);
    /// assert_eq!(Some(7), evens.next().await);
    /// assert_eq!(Some(9), evens.next().await);
    /// assert_eq!(None, evens.next().await);
    /// # }
    /// ```
    fn filter_map<T, F>(self, f: F) -> FilterMap<Self, F>
    where
        F: FnMut(Self::Item) -> Option<T>,
        Self: Sized,
    {
        FilterMap::new(self, f)
    }
}

impl<T: ?Sized> StreamExt for T where T: Stream {}
