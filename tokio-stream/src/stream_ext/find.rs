use crate::Stream;

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::task::{ready, Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Future for the [`find`](super::StreamExt::find) method.
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct FindFuture<'a, St: ?Sized, F> {
        stream: &'a mut St,
        f: F,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<'a, St: ?Sized, F> FindFuture<'a, St, F> {
    pub(super) fn new(stream: &'a mut St, f: F) -> Self {
        Self {
            stream,
            f,
            _pin: PhantomPinned,
        }
    }
}

impl<St, F> Future for FindFuture<'_, St, F>
where
    St: ?Sized + Stream + Unpin,
    F: FnMut(St::Item) -> Option<St::Item>,
{
    type Output = Option<St::Item>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<St::Item>> {
        let me = self.project();
        let mut stream = Pin::new(me.stream);

        loop {
            match ready!(stream.as_mut().poll_next(cx)) {
                Some(v) => {
                    if let Some(v) = (me.f)(v) {
                        return Poll::Ready(Some(v));
                    }
                }
                None => return Poll::Ready(None),
            }
        }
    }
}
