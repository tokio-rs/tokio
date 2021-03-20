use crate::Stream;

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Future for the [`any`](super::StreamExt::any) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct AnyFuture<'a, St: ?Sized, F> {
        stream: &'a mut St,
        f: F,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<'a, St: ?Sized, F> AnyFuture<'a, St, F> {
    pub(super) fn new(stream: &'a mut St, f: F) -> Self {
        Self {
            stream,
            f,
            _pin: PhantomPinned,
        }
    }
}

impl<St, F> Future for AnyFuture<'_, St, F>
where
    St: ?Sized + Stream + Unpin,
    F: FnMut(St::Item) -> bool,
{
    type Output = bool;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = self.project();
        let mut stream = Pin::new(me.stream);

        loop {
            match futures_core::ready!(stream.as_mut().poll_next(cx)) {
                Some(v) => {
                    if (me.f)(v) {
                        break Poll::Ready(true);
                    }
                }
                None => break Poll::Ready(false),
            }
        }
    }
}
