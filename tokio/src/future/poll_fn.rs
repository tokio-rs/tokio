#![allow(dead_code)]

//! Definition of the `PollFn` adapter combinator.

use std::fmt;
use std::future::Future;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future for the [`poll_fn`] function.
pub struct PollFn<F> {
    f: F,
    _pinned: PhantomPinned,
}

/// Creates a new future wrapping around a function returning [`Poll`].
pub fn poll_fn<T, F>(f: F) -> PollFn<F>
where
    F: FnMut(&mut Context<'_>) -> Poll<T>,
{
    PollFn {
        f,
        _pinned: PhantomPinned,
    }
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
        // SAFETY: We never construct a `Pin<&mut F>` anywhere, so accessing `f`
        // mutably in an unpinned way is sound.
        //
        // This is, strictly speaking, not necessary. We could make `PollFn`
        // unconditionally `Unpin` and avoid this unsafe. However, making this
        // struct `!Unpin` mitigates the issues described here:
        // <https://internals.rust-lang.org/t/surprising-soundness-trouble-around-pollfn/17484>
        let me = unsafe { Pin::into_inner_unchecked(self) };
        (me.f)(cx)
    }
}
