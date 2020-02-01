//! Stream utilities for Tokio.
//!
//! A `Stream` is an asynchronous sequence of values. It can be thought of as an asynchronous version of the standard library's `Iterator` trait.
//!
//! This module provides helpers to work with them.

mod all;
use all::AllFuture;

mod any;
use any::AnyFuture;

mod chain;
use chain::Chain;

mod collect;
use collect::Collect;
pub use collect::FromStream;

mod empty;
pub use empty::{empty, Empty};

mod filter;
use filter::Filter;

mod filter_map;
use filter_map::FilterMap;

mod fold;
use fold::FoldFuture;

mod fuse;
use fuse::Fuse;

mod iter;
pub use iter::{iter, Iter};

mod map;
use map::Map;

mod merge;
use merge::Merge;

mod next;
use next::Next;

mod once;
pub use once::{once, Once};

mod pending;
pub use pending::{pending, Pending};

mod stream_map;
pub use stream_map::StreamMap;

mod skip;
use skip::Skip;

mod skip_while;
use skip_while::SkipWhile;

mod try_next;
use try_next::TryNext;

mod take;
use take::Take;

mod take_while;
use take_while::TakeWhile;

cfg_time! {
    mod timeout;
    use timeout::Timeout;
    use std::time::Duration;
}

pub use futures_core::Stream;

/// An extension trait for `Stream`s that provides a variety of convenient
/// combinator functions.
pub trait StreamExt: Stream {
    /// Consumes and returns the next value in the stream or `None` if the
    /// stream is finished.
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

    /// Consumes and returns the next item in the stream. If an error is
    /// encountered before the next item, the error is returned instead.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn try_next(&mut self) -> Result<Option<T>, E>;
    /// ```
    ///
    /// This is similar to the [`next`](StreamExt::next) combinator,
    /// but returns a [`Result<Option<T>, E>`](Result) rather than
    /// an [`Option<Result<T, E>>`](Option), making for easy use
    /// with the [`?`](std::ops::Try) operator.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let mut stream = stream::iter(vec![Ok(1), Ok(2), Err("nope")]);
    ///
    /// assert_eq!(stream.try_next().await, Ok(Some(1)));
    /// assert_eq!(stream.try_next().await, Ok(Some(2)));
    /// assert_eq!(stream.try_next().await, Err("nope"));
    /// # }
    /// ```
    fn try_next<T, E>(&mut self) -> TryNext<'_, Self>
    where
        Self: Stream<Item = Result<T, E>> + Unpin,
    {
        TryNext::new(self)
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

    /// Combine two streams into one by interleaving the output of both as it
    /// is produced.
    ///
    /// Values are produced from the merged stream in the order they arrive from
    /// the two source streams. If both source streams provide values
    /// simultaneously, the merge stream alternates between them. This provides
    /// some level of fairness.
    ///
    /// The merged stream completes once **both** source streams complete. When
    /// one source stream completes before the other, the merge stream
    /// exclusively polls the remaining stream.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::StreamExt;
    /// use tokio::sync::mpsc;
    /// use tokio::time;
    ///
    /// use std::time::Duration;
    ///
    /// # /*
    /// #[tokio::main]
    /// # */
    /// # #[tokio::main(basic_scheduler)]
    /// async fn main() {
    /// # time::pause();
    ///     let (mut tx1, rx1) = mpsc::channel(10);
    ///     let (mut tx2, rx2) = mpsc::channel(10);
    ///
    ///     let mut rx = rx1.merge(rx2);
    ///
    ///     tokio::spawn(async move {
    ///         // Send some values immediately
    ///         tx1.send(1).await.unwrap();
    ///         tx1.send(2).await.unwrap();
    ///
    ///         // Let the other task send values
    ///         time::delay_for(Duration::from_millis(20)).await;
    ///
    ///         tx1.send(4).await.unwrap();
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         // Wait for the first task to send values
    ///         time::delay_for(Duration::from_millis(5)).await;
    ///
    ///         tx2.send(3).await.unwrap();
    ///
    ///         time::delay_for(Duration::from_millis(25)).await;
    ///
    ///         // Send the final value
    ///         tx2.send(5).await.unwrap();
    ///     });
    ///
    ///    assert_eq!(1, rx.next().await.unwrap());
    ///    assert_eq!(2, rx.next().await.unwrap());
    ///    assert_eq!(3, rx.next().await.unwrap());
    ///    assert_eq!(4, rx.next().await.unwrap());
    ///    assert_eq!(5, rx.next().await.unwrap());
    ///
    ///    // The merged stream is consumed
    ///    assert!(rx.next().await.is_none());
    /// }
    /// ```
    fn merge<U>(self, other: U) -> Merge<Self, U>
    where
        U: Stream<Item = Self::Item>,
        Self: Sized,
    {
        Merge::new(self, other)
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

    /// Creates a stream which ends after the first `None`.
    ///
    /// After a stream returns `None`, behavior is undefined. Future calls to
    /// `poll_next` may or may not return `Some(T)` again or they may panic.
    /// `fuse()` adapts a stream, ensuring that after `None` is given, it will
    /// return `None` forever.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{Stream, StreamExt};
    ///
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    ///
    /// // a stream which alternates between Some and None
    /// struct Alternate {
    ///     state: i32,
    /// }
    ///
    /// impl Stream for Alternate {
    ///     type Item = i32;
    ///
    ///     fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<i32>> {
    ///         let val = self.state;
    ///         self.state = self.state + 1;
    ///
    ///         // if it's even, Some(i32), else None
    ///         if val % 2 == 0 {
    ///             Poll::Ready(Some(val))
    ///         } else {
    ///             Poll::Ready(None)
    ///         }
    ///     }
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut stream = Alternate { state: 0 };
    ///
    ///     // the stream goes back and forth
    ///     assert_eq!(stream.next().await, Some(0));
    ///     assert_eq!(stream.next().await, None);
    ///     assert_eq!(stream.next().await, Some(2));
    ///     assert_eq!(stream.next().await, None);
    ///
    ///     // however, once it is fused
    ///     let mut stream = stream.fuse();
    ///
    ///     assert_eq!(stream.next().await, Some(4));
    ///     assert_eq!(stream.next().await, None);
    ///
    ///     // it will always return `None` after the first time.
    ///     assert_eq!(stream.next().await, None);
    ///     assert_eq!(stream.next().await, None);
    ///     assert_eq!(stream.next().await, None);
    /// }
    /// ```
    fn fuse(self) -> Fuse<Self>
    where
        Self: Sized,
    {
        Fuse::new(self)
    }

    /// Creates a new stream of at most `n` items of the underlying stream.
    ///
    /// Once `n` items have been yielded from this stream then it will always
    /// return that the stream is done.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let mut stream = stream::iter(1..=10).take(3);
    ///
    /// assert_eq!(Some(1), stream.next().await);
    /// assert_eq!(Some(2), stream.next().await);
    /// assert_eq!(Some(3), stream.next().await);
    /// assert_eq!(None, stream.next().await);
    /// # }
    /// ```
    fn take(self, n: usize) -> Take<Self>
    where
        Self: Sized,
    {
        Take::new(self, n)
    }

    /// Take elements from this stream while the provided predicate
    /// resolves to `true`.
    ///
    /// This function, like `Iterator::take_while`, will take elements from the
    /// stream until the predicate `f` resolves to `false`. Once one element
    /// returns false it will always return that the stream is done.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let mut stream = stream::iter(1..=10).take_while(|x| *x <= 3);
    ///
    /// assert_eq!(Some(1), stream.next().await);
    /// assert_eq!(Some(2), stream.next().await);
    /// assert_eq!(Some(3), stream.next().await);
    /// assert_eq!(None, stream.next().await);
    /// # }
    /// ```
    fn take_while<F>(self, f: F) -> TakeWhile<Self, F>
    where
        F: FnMut(&Self::Item) -> bool,
        Self: Sized,
    {
        TakeWhile::new(self, f)
    }

    /// Creates a new stream that will skip the `n` first items of the
    /// underlying stream.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let mut stream = stream::iter(1..=10).skip(7);
    ///
    /// assert_eq!(Some(8), stream.next().await);
    /// assert_eq!(Some(9), stream.next().await);
    /// assert_eq!(Some(10), stream.next().await);
    /// assert_eq!(None, stream.next().await);
    /// # }
    /// ```
    fn skip(self, n: usize) -> Skip<Self>
    where
        Self: Sized,
    {
        Skip::new(self, n)
    }

    /// Skip elements from the underlying stream while the provided predicate
    /// resolves to `true`.
    ///
    /// This function, like [`Iterator::skip_while`], will ignore elemets from the
    /// stream until the predicate `f` resolves to `false`. Once one element
    /// returns false, the rest of the elements will be yielded.
    ///
    /// [`Iterator::skip_while`]: std::iter::Iterator::skip_while()
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    /// let mut stream = stream::iter(vec![1,2,3,4,1]).skip_while(|x| *x < 3);
    ///
    /// assert_eq!(Some(3), stream.next().await);
    /// assert_eq!(Some(4), stream.next().await);
    /// assert_eq!(Some(1), stream.next().await);
    /// assert_eq!(None, stream.next().await);
    /// # }
    /// ```
    fn skip_while<F>(self, f: F) -> SkipWhile<Self, F>
    where
        F: FnMut(&Self::Item) -> bool,
        Self: Sized,
    {
        SkipWhile::new(self, f)
    }

    /// Tests if every element of the stream matches a predicate.
    ///
    /// `all()` takes a closure that returns `true` or `false`. It applies
    /// this closure to each element of the stream, and if they all return
    /// `true`, then so does `all`. If any of them return `false`, it
    /// returns `false`. An empty stream returns `true`.
    ///
    /// `all()` is short-circuiting; in other words, it will stop processing
    /// as soon as it finds a `false`, given that no matter what else happens,
    /// the result will also be `false`.
    ///
    /// An empty stream returns `true`.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let a = [1, 2, 3];
    ///
    /// assert!(stream::iter(&a).all(|&x| x > 0).await);
    ///
    /// assert!(!stream::iter(&a).all(|&x| x > 2).await);
    /// # }
    /// ```
    ///
    /// Stopping at the first `false`:
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let a = [1, 2, 3];
    ///
    /// let mut iter = stream::iter(&a);
    ///
    /// assert!(!iter.all(|&x| x != 2).await);
    ///
    /// // we can still use `iter`, as there are more elements.
    /// assert_eq!(iter.next().await, Some(&3));
    /// # }
    /// ```
    fn all<F>(&mut self, f: F) -> AllFuture<'_, Self, F>
    where
        Self: Unpin,
        F: FnMut(Self::Item) -> bool,
    {
        AllFuture::new(self, f)
    }

    /// Tests if any element of the stream matches a predicate.
    ///
    /// `any()` takes a closure that returns `true` or `false`. It applies
    /// this closure to each element of the stream, and if any of them return
    /// `true`, then so does `any()`. If they all return `false`, it
    /// returns `false`.
    ///
    /// `any()` is short-circuiting; in other words, it will stop processing
    /// as soon as it finds a `true`, given that no matter what else happens,
    /// the result will also be `true`.
    ///
    /// An empty stream returns `false`.
    ///
    /// Basic usage:
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let a = [1, 2, 3];
    ///
    /// assert!(stream::iter(&a).any(|&x| x > 0).await);
    ///
    /// assert!(!stream::iter(&a).any(|&x| x > 5).await);
    /// # }
    /// ```
    ///
    /// Stopping at the first `true`:
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    ///
    /// let a = [1, 2, 3];
    ///
    /// let mut iter = stream::iter(&a);
    ///
    /// assert!(iter.any(|&x| x != 2).await);
    ///
    /// // we can still use `iter`, as there are more elements.
    /// assert_eq!(iter.next().await, Some(&2));
    /// # }
    /// ```
    fn any<F>(&mut self, f: F) -> AnyFuture<'_, Self, F>
    where
        Self: Unpin,
        F: FnMut(Self::Item) -> bool,
    {
        AnyFuture::new(self, f)
    }

    /// Combine two streams into one by first returning all values from the
    /// first stream then all values from the second stream.
    ///
    /// As long as `self` still has values to emit, no values from `other` are
    /// emitted, even if some are ready.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{self, StreamExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let one = stream::iter(vec![1, 2, 3]);
    ///     let two = stream::iter(vec![4, 5, 6]);
    ///
    ///     let mut stream = one.chain(two);
    ///
    ///     assert_eq!(stream.next().await, Some(1));
    ///     assert_eq!(stream.next().await, Some(2));
    ///     assert_eq!(stream.next().await, Some(3));
    ///     assert_eq!(stream.next().await, Some(4));
    ///     assert_eq!(stream.next().await, Some(5));
    ///     assert_eq!(stream.next().await, Some(6));
    ///     assert_eq!(stream.next().await, None);
    /// }
    /// ```
    fn chain<U>(self, other: U) -> Chain<Self, U>
    where
        U: Stream<Item = Self::Item>,
        Self: Sized,
    {
        Chain::new(self, other)
    }

    /// A combinator that applies a function to every element in a stream
    /// producing a single, final value.
    ///
    /// # Examples
    /// Basic usage:
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, *};
    ///
    /// let s = stream::iter(vec![1u8, 2, 3]);
    /// let sum = s.fold(0, |acc, x| acc + x).await;
    ///
    /// assert_eq!(sum, 6);
    /// # }
    /// ```
    fn fold<B, F>(self, init: B, f: F) -> FoldFuture<Self, B, F>
    where
        Self: Sized,
        F: FnMut(B, Self::Item) -> B,
    {
        FoldFuture::new(self, init, f)
    }

    /// Drain stream pushing all emitted values into a collection.
    ///
    /// `collect` streams all values, awaiting as needed. Values are pushed into
    /// a collection. A number of different target collection types are
    /// supported, including [`Vec`](std::vec::Vec),
    /// [`String`](std::string::String), and [`Bytes`](bytes::Bytes).
    ///
    /// # `Result`
    ///
    /// `collect()` can also be used with streams of type `Result<T, E>` where
    /// `T: FromStream<_>`. In this case, `collect()` will stream as long as
    /// values yielded from the stream are `Ok(_)`. If `Err(_)` is encountered,
    /// streaming is terminated and `collect()` returns the `Err`.
    ///
    /// # Notes
    ///
    /// `FromStream` is currently a sealed trait. Stabilization is pending
    /// enhancements to the Rust langague.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```
    /// use tokio::stream::{self, StreamExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let doubled: Vec<i32> =
    ///         stream::iter(vec![1, 2, 3])
    ///             .map(|x| x * 2)
    ///             .collect()
    ///             .await;
    ///
    ///     assert_eq!(vec![2, 4, 6], doubled);
    /// }
    /// ```
    ///
    /// Collecting a stream of `Result` values
    ///
    /// ```
    /// use tokio::stream::{self, StreamExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // A stream containing only `Ok` values will be collected
    ///     let values: Result<Vec<i32>, &str> =
    ///         stream::iter(vec![Ok(1), Ok(2), Ok(3)])
    ///             .collect()
    ///             .await;
    ///
    ///     assert_eq!(Ok(vec![1, 2, 3]), values);
    ///
    ///     // A stream containing `Err` values will return the first error.
    ///     let results = vec![Ok(1), Err("no"), Ok(2), Ok(3), Err("nein")];
    ///
    ///     let values: Result<Vec<i32>, &str> =
    ///         stream::iter(results)
    ///             .collect()
    ///             .await;
    ///
    ///     assert_eq!(Err("no"), values);
    /// }
    /// ```
    fn collect<T>(self) -> Collect<Self, T>
    where
        T: FromStream<Self::Item>,
        Self: Sized,
    {
        Collect::new(self)
    }

    /// Applies a per-item timeout to the passed stream.
    ///
    /// `timeout()` takes a `Duration` that represents the maximum amount of
    /// time each element of the stream has to complete before timing out.
    ///
    /// If the wrapped stream yields a value before the deadline is reached, the
    /// value is returned. Otherwise, an error is returned. The caller may decide
    /// to continue consuming the stream and will eventually get the next source
    /// stream value once it becomes available.
    ///
    /// # Notes
    ///
    /// This function consumes the stream passed into it and returns a
    /// wrapped version of it.
    ///
    /// Polling the returned stream will continue to poll the inner stream even
    /// if one or more items time out.
    ///
    /// # Examples
    ///
    /// Suppose we have a stream `int_stream` that yields 3 numbers (1, 2, 3):
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::stream::{self, StreamExt};
    /// use std::time::Duration;
    /// # let int_stream = stream::iter(1..=3);
    ///
    /// let mut int_stream = int_stream.timeout(Duration::from_secs(1));
    ///
    /// // When no items time out, we get the 3 elements in succession:
    /// assert_eq!(int_stream.try_next().await, Ok(Some(1)));
    /// assert_eq!(int_stream.try_next().await, Ok(Some(2)));
    /// assert_eq!(int_stream.try_next().await, Ok(Some(3)));
    /// assert_eq!(int_stream.try_next().await, Ok(None));
    ///
    /// // If the second item times out, we get an error and continue polling the stream:
    /// # let mut int_stream = stream::iter(vec![Ok(1), Err(()), Ok(2), Ok(3)]);
    /// assert_eq!(int_stream.try_next().await, Ok(Some(1)));
    /// assert!(int_stream.try_next().await.is_err());
    /// assert_eq!(int_stream.try_next().await, Ok(Some(2)));
    /// assert_eq!(int_stream.try_next().await, Ok(Some(3)));
    /// assert_eq!(int_stream.try_next().await, Ok(None));
    ///
    /// // If we want to stop consuming the source stream the first time an
    /// // element times out, we can use the `take_while` operator:
    /// # let int_stream = stream::iter(vec![Ok(1), Err(()), Ok(2), Ok(3)]);
    /// let mut int_stream = int_stream.take_while(Result::is_ok);
    ///
    /// assert_eq!(int_stream.try_next().await, Ok(Some(1)));
    /// assert_eq!(int_stream.try_next().await, Ok(None));
    /// # }
    /// ```
    #[cfg(all(feature = "time"))]
    #[cfg_attr(docsrs, doc(cfg(feature = "time")))]
    fn timeout(self, duration: Duration) -> Timeout<Self>
    where
        Self: Sized,
    {
        Timeout::new(self, duration)
    }
}

impl<St: ?Sized> StreamExt for St where St: Stream {}
