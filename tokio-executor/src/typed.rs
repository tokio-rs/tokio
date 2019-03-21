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
///
pub trait TypedExecutor<T> {
    /// TODO: DOX
    fn spawn(&mut self, future: T) -> Result<(), SpawnError>;

    /// TODO: DOX
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
