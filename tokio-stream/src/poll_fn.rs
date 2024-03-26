use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) struct PollFn<F> {
    f: F,
}

pub(crate) fn poll_fn<T, F>(f: F) -> PollFn<F>
where
    F: FnMut(&mut Context<'_>) -> Poll<T>,
{
    PollFn { f }
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
