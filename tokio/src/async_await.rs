use std::future::Future as StdFuture;
use std::pin::Pin;
use std::task::{Poll, Waker};

fn map_ok<T: StdFuture>(future: T) -> impl StdFuture<Output = Result<(), ()>> {
    MapOk(future)
}

struct MapOk<T>(T);

impl<T> MapOk<T> {
    fn future<'a>(self: Pin<&'a mut Self>) -> Pin<&'a mut T> {
        unsafe { Pin::map_unchecked_mut(self, |x| &mut x.0) }
    }
}

impl<T: StdFuture> StdFuture for MapOk<T> {
    type Output = Result<(), ()>;

    fn poll(self: Pin<&mut Self>, waker: &Waker) -> Poll<Self::Output> {
        match self.future().poll(waker) {
            Poll::Ready(_) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Like `tokio::run`, but takes an `async` block
pub fn run_async<F>(future: F)
where
    F: StdFuture<Output = ()> + Send + 'static,
{
    use tokio_async_await::compat::backward;
    let future = backward::Compat::new(map_ok(future));

    ::run(future);
}

/// Like `tokio::spawn`, but takes an `async` block
pub fn spawn_async<F>(future: F)
where
    F: StdFuture<Output = ()> + Send + 'static,
{
    use tokio_async_await::compat::backward;
    let future = backward::Compat::new(map_ok(future));

    ::spawn(future);
}
