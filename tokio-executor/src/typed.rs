use crate::SpawnError;

/// A value that spawns futures of a specific type.
///
/// The trait is generic over `T`: the type of future that can be spawened. This
/// is useful for implementing an executor that is only able to spawn a specific
/// type of future.
///
/// The [`spawn`] function is used to submit the future to the executor. Once
/// submitted, the executor takes ownership of the future and becomes
/// responsible for driving the future to completion.
///
/// This trait is useful as a bound for applications and libraries in order to
/// be generic over futures that are `Send` vs. `!Send`.
///
/// # Examples
///
/// Consider a function that provides an API for draining a `Stream` in the
/// background. To do this, a task must be spawned to perform the draining. As
/// such, the function takes a stream and an executor on which the background
/// task is spawned.
///
/// [`spawn`]: TypedExecutor::spawn
/// ```
/// #![feature(async_await)]
///
/// use tokio::executor::TypedExecutor;
/// use tokio::sync::oneshot;
///
/// use futures_core::{ready, Stream};
/// use std::future::Future;
/// use std::pin::Pin;
/// use std::task::{Context, Poll};
///
/// async fn drain<T, E>(stream: T, executor: &mut E)
/// where
///     T: Stream + Unpin,
///     E: TypedExecutor<Drain<T>>
/// {
///     let (tx, rx) = oneshot::channel();
///
///     executor.spawn(Drain {
///         stream,
///         tx: Some(tx),
///     }).unwrap();
///
///     rx.await.unwrap()
/// }
///
/// // The background task
/// pub struct Drain<T> {
///     stream: T,
///     tx: Option<oneshot::Sender<()>>,
/// }
///
/// impl<T: Stream + Unpin> Future for Drain<T> {
///     type Output = ();
///
///     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
///         loop {
///             let item = ready!(
///                 Pin::new(&mut self.stream).poll_next(cx)
///             );
///
///             if item.is_none() { break; }
///         }
///
///         self.tx.take().unwrap().send(()).map_err(|_| ());
///         Poll::Ready(())
///     }
/// }
/// ```
///
/// By doing this, the `drain` fn can accept a stream that is `!Send` as long as
/// the supplied executor is able to spawn `!Send` types.
pub trait TypedExecutor<T> {
    /// Spawns a future to run on this executor.
    ///
    /// `future` is passed to the executor, which will begin running it. The
    /// executor takes ownership of the future and becomes responsible for
    /// driving the future to completion.
    ///
    /// # Panics
    ///
    /// Implementations are encouraged to avoid panics. However, panics are
    /// permitted and the caller should check the implementation specific
    /// documentation for more details on possible panics.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio_executor::TypedExecutor;
    ///
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    ///
    /// fn example<T>(my_executor: &mut T)
    /// where
    ///     T: TypedExecutor<MyFuture>,
    /// {
    ///     my_executor.spawn(MyFuture).unwrap();
    /// }
    ///
    /// struct MyFuture;
    ///
    /// impl Future for MyFuture {
    ///     type Output = ();
    ///
    ///     fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
    ///         println!("running on the executor");
    ///         Poll::Ready(())
    ///     }
    /// }
    /// ```
    fn spawn(&mut self, future: T) -> Result<(), SpawnError>;

    /// Provides a best effort **hint** to whether or not `spawn` will succeed.
    ///
    /// This function may return both false positives **and** false negatives.
    /// If `status` returns `Ok`, then a call to `spawn` will *probably*
    /// succeed, but may fail. If `status` returns `Err`, a call to `spawn` will
    /// *probably* fail, but may succeed.
    ///
    /// This allows a caller to avoid creating the task if the call to `spawn`
    /// has a high likelihood of failing.
    ///
    /// # Panics
    ///
    /// This function must not panic. Implementers must ensure that panics do
    /// not happen.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio_executor::TypedExecutor;
    ///
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    ///
    /// fn example<T>(my_executor: &mut T)
    /// where
    ///     T: TypedExecutor<MyFuture>,
    /// {
    ///     if my_executor.status().is_ok() {
    ///         my_executor.spawn(MyFuture).unwrap();
    ///     } else {
    ///         println!("the executor is not in a good state");
    ///     }
    /// }
    ///
    /// struct MyFuture;
    ///
    /// impl Future for MyFuture {
    ///     type Output = ();
    ///
    ///     fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
    ///         println!("running on the executor");
    ///         Poll::Ready(())
    ///     }
    /// }
    /// ```
    fn status(&self) -> Result<(), SpawnError> {
        Ok(())
    }
}

impl<E, T> TypedExecutor<T> for Box<E>
where
    E: TypedExecutor<T>,
{
    fn spawn(&mut self, future: T) -> Result<(), SpawnError> {
        (**self).spawn(future)
    }

    fn status(&self) -> Result<(), SpawnError> {
        (**self).status()
    }
}
