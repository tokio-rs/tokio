//! Asynchronous value iteration.

mod into_std;
pub use into_std::IntoStd;

use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A stream of values produced asynchronously.
///
/// This trait is used to convert Tokio's "stream-like" types into
/// `futures::Stream`.  This trait is not intended to be implemented by third
/// parties (use `futures::Stream` instead) and only exists to satisfy Rust's
/// coherence requirements. When `Stream` is stabilized in `std`, this trait
/// will be removed and `tokio` will directly implement the `std` trait.
#[must_use = "streams do nothing unless polled"]
pub trait Stream {
    /// Values yielded by the stream.
    type Item;

    /// Attempt to pull out the next value of this stream, registering the
    /// current task for wakeup if the value is not yet available, and returning
    /// `None` if the stream is exhausted.
    ///
    /// # Return value
    ///
    /// There are several possible return values, each indicating a distinct
    /// stream state:
    ///
    /// - `Poll::Pending` means that this stream's next value is not ready
    /// yet. Implementations will ensure that the current task will be notified
    /// when the next value may be ready.
    ///
    /// - `Poll::Ready(Some(val))` means that the stream has successfully
    /// produced a value, `val`, and may produce further values on subsequent
    /// `poll_next` calls.
    ///
    /// - `Poll::Ready(None)` means that the stream has terminated, and
    /// `poll_next` should not be invoked again.
    ///
    /// # Panics
    ///
    /// Once a stream is finished, i.e. `Ready(None)` has been returned, further
    /// calls to `poll_next` may result in a panic or other "bad behavior".  If
    /// this is difficult to guard against then the `fuse` adapter can be used
    /// to ensure that `poll_next` always returns `Ready(None)` in subsequent
    /// calls.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;

    /// Returns the bounds on the remaining length of the stream.
    ///
    /// Specifically, `size_hint()` returns a tuple where the first element
    /// is the lower bound, and the second element is the upper bound.
    ///
    /// The second half of the tuple that is returned is an [`Option`]`<`[`usize`]`>`.
    /// A [`None`] here means that either there is no known upper bound, or the
    /// upper bound is larger than [`usize`].
    ///
    /// # Implementation notes
    ///
    /// It is not enforced that a stream implementation yields the declared
    /// number of elements. A buggy stream may yield less than the lower bound
    /// or more than the upper bound of elements.
    ///
    /// `size_hint()` is primarily intended to be used for optimizations such as
    /// reserving space for the elements of the stream, but must not be
    /// trusted to e.g., omit bounds checks in unsafe code. An incorrect
    /// implementation of `size_hint()` should not lead to memory safety
    /// violations.
    ///
    /// That said, the implementation should provide a correct estimation,
    /// because otherwise it would be a violation of the trait's protocol.
    ///
    /// The default implementation returns `(0, `[`None`]`)` which is correct for any
    /// stream.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }

    /// Convert the stream into a `futures::Stream` type.
    fn into_std(self) -> IntoStd<Self>
    where
        Self: Sized,
    {
        IntoStd { stream: self }
    }
}

impl<S: ?Sized + Stream + Unpin> Stream for &mut S {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        S::poll_next(Pin::new(&mut **self), cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (**self).size_hint()
    }
}

impl<P> Stream for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: Stream,
{
    type Item = <P::Target as Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().as_mut().poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (**self).size_hint()
    }
}
