#![allow(dead_code)]

//! Definition of the `PollFn` adapter combinator.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// This struct is intentionally `!Unpin` when `F` is `!Unpin`. This is to
// mitigate the issue where rust puts noalias on mutable references to the
// `PollFn` type if it is `Unpin`. If the closure has ownership of a future,
// then this "leaks" and the future is affected by noalias too, which we don't
// want.
//
// See this thread for more information:
// <https://internals.rust-lang.org/t/surprising-soundness-trouble-around-pollfn/17484>
//
// The fact that `PollFn` is not `Unpin` when it shouldn't be is tested in
// `tests/async_send_sync.rs`.

/// Future for the [`poll_fn`] function.
pub struct PollFn<F> {
    f: F,
}

/// Creates a new future wrapping around a function returning [`Poll`].
pub fn poll_fn<T, F>(f: F) -> PollFn<F>
where
    F: FnMut(&mut Context<'_>) -> Poll<T>,
{
    PollFn { f }
}

impl<F> fmt::Debug for PollFn<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PollFn").finish()
    }
}

impl<T, F> Future for PollFn<F>
where
    F: FnMut(&mut Context<'_>) -> Poll<T>,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        // Safety: We never construct a `Pin<&mut F>` anywhere, so accessing `f`
        // mutably in an unpinned way is sound.
        //
        // This use of unsafe cannot be replaced with the pin-project macro
        // because:
        //  * If we put `#[pin]` on the field, then it gives us a `Pin<&mut F>`,
        //    which we can't use to call the closure.
        //  * If we don't put `#[pin]` on the field, then it makes `PollFn` be
        //    unconditionally `Unpin`, which we also don't want.
        let me = unsafe { Pin::into_inner_unchecked(self) };
        (me.f)(cx)
    }
}
