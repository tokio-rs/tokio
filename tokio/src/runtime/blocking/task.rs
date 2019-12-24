use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Converts a function to a future that completes on poll
pub(super) struct BlockingTask<T> {
    func: Option<T>,
}

impl<T> BlockingTask<T> {
    /// Initializes a new blocking task from the given function
    pub(super) fn new(func: T) -> BlockingTask<T> {
        BlockingTask { func: Some(func) }
    }
}

impl<T, R> Future for BlockingTask<T>
where
    T: FnOnce() -> R,
{
    type Output = R;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<R> {
        let me = unsafe { self.get_unchecked_mut() };
        let func = me
            .func
            .take()
            .expect("[internal exception] blocking task ran twice.");

        Poll::Ready(func())
    }
}
