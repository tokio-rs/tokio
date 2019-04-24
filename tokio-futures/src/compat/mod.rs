//! Compatibility layer between futures 0.1 and `std`.

pub mod backward;
pub mod forward;

/// Convert a `std::future::Future` yielding `Result` into an 0.1 `Future`.
pub fn into_01<T, Item, Error>(future: T) -> backward::Compat<T>
where
    T: std::future::Future<Output = Result<Item, Error>>,
{
    backward::Compat::new(future)
}

/// Convert a `std::future::Future` into an 0.1 `Future` with unit error.
pub fn infallible_into_01<T>(future: T) -> impl futures::Future<Item = T::Output, Error = ()>
where
    T: std::future::Future,
{
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub struct Map<T>(T);

    impl<T> Map<T> {
        fn future<'a>(self: Pin<&'a mut Self>) -> Pin<&'a mut T> {
            unsafe { Pin::map_unchecked_mut(self, |x| &mut x.0) }
        }
    }

    impl<T: std::future::Future> std::future::Future for Map<T> {
        type Output = Result<T::Output, ()>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            match self.future().poll(cx) {
                Poll::Ready(v) => Poll::Ready(Ok(v)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    into_01(Map(future))
}
