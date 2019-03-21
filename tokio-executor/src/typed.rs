use SpawnError;

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
/// ```rust
/// #[macro_use]
/// extern crate futures;
/// extern crate tokio;
///
/// use futures::{Future, Stream, Poll};
/// use tokio::executor::TypedExecutor;
/// use tokio::sync::oneshot;
///
/// pub fn drain<T, E>(stream: T, executor: &mut E)
///     -> impl Future<Item = (), Error = ()>
/// where
///     T: Stream,
///     E: TypedExecutor<Drain<T>>
/// {
///     let (tx, rx) = oneshot::channel();
///
///     executor.spawn(Drain {
///         stream,
///         tx: Some(tx),
///     }).unwrap();
///
///     rx.map_err(|_| ())
/// }
///
/// // The background task
/// pub struct Drain<T: Stream> {
///     stream: T,
///     tx: Option<oneshot::Sender<()>>,
/// }
///
/// impl<T: Stream> Future for Drain<T> {
///     type Item = ();
///     type Error = ();
///
///     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
///         loop {
///             let item = try_ready!(
///                 self.stream.poll()
///                     .map_err(|_| ())
///             );
///
///             if item.is_none() { break; }
///         }
///
///         self.tx.take().unwrap().send(()).map_err(|_| ());
///         Ok(().into())
///     }
/// }
/// # pub fn main() {}
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
    /// # extern crate futures;
    /// # extern crate tokio_executor;
    /// # use tokio_executor::TypedExecutor;
    /// # use futures::{Future, Poll};
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
    ///     type Item = ();
    ///     type Error = ();
    ///
    ///     fn poll(&mut self) -> Poll<(), ()> {
    ///         println!("running on the executor");
    ///         Ok(().into())
    ///     }
    /// }
    /// # fn main() {}
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
    /// # extern crate futures;
    /// # extern crate tokio_executor;
    /// # use tokio_executor::TypedExecutor;
    /// # use futures::{Future, Poll};
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
    ///     type Item = ();
    ///     type Error = ();
    ///
    ///     fn poll(&mut self) -> Poll<(), ()> {
    ///         println!("running on the executor");
    ///         Ok(().into())
    ///     }
    /// }
    /// # fn main() {}
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
