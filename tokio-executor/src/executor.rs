use futures::Future;
use SpawnError;

/// A value that executes futures.
///
/// The [`spawn`] function is used to submit a future to an executor. Once
/// submitted, the executor takes ownership of the future and becomes
/// responsible for driving the future to completion.
///
/// The strategy employed by the executor to handle the future is less defined
/// and is left up to the `Executor` implementation. The `Executor` instance is
/// expected to call [`poll`] on the future once it has been notified, however
/// the "when" and "how" can vary greatly.
///
/// For example, the executor might be a thread pool, in which case a set of
/// threads have already been spawned up and the future is inserted into a
/// queue. A thread will acquire the future and poll it.
///
/// The `Executor` trait is only for futures that **are** `Send`. These are most
/// common. There currently is no trait that describes executors that operate
/// entirely on the current thread (i.e., are able to spawn futures that are not
/// `Send`). Note that single threaded executors can still implement `Executor`,
/// but only futures that are `Send` can be spawned via the trait.
///
/// This trait is primarily intended to implemented by executors and used to
/// back `tokio::spawn`. Libraries and applications **may** use this trait to
/// bound generics, but doing so will limit usage to futures that implement
/// `Send`. Instead, libraries and applications are recommended to use
/// [`TypedExecutor`] as a bound.
///
/// # Errors
///
/// The [`spawn`] function returns `Result` with an error type of `SpawnError`.
/// This error type represents the reason that the executor was unable to spawn
/// the future. The two current represented scenarios are:
///
/// * An executor being at capacity or full. As such, the executor is not able
///   to accept a new future. This error state is expected to be transient.
/// * An executor has been shutdown and can no longer accept new futures. This
///   error state is expected to be permanent.
///
/// If a caller encounters an at capacity error, the caller should try to shed
/// load. This can be as simple as dropping the future that was spawned.
///
/// If the caller encounters a shutdown error, the caller should attempt to
/// gracefully shutdown.
///
/// # Examples
///
/// ```rust
/// # extern crate futures;
/// # extern crate tokio_executor;
/// # use tokio_executor::Executor;
/// # fn docs(my_executor: &mut Executor) {
/// use futures::future::lazy;
/// my_executor.spawn(Box::new(lazy(|| {
///     println!("running on the executor");
///     Ok(())
/// }))).unwrap();
/// # }
/// # fn main() {}
/// ```
///
/// [`spawn`]: #tymethod.spawn
/// [`poll`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html#tymethod.poll
/// [`TypedExecutor`]: ../trait.TypedExecutor.html
pub trait Executor {
    /// Spawns a future object to run on this executor.
    ///
    /// `future` is passed to the executor, which will begin running it. The
    /// future may run on the current thread or another thread at the discretion
    /// of the `Executor` implementation.
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
    /// # use tokio_executor::Executor;
    /// # fn docs(my_executor: &mut Executor) {
    /// use futures::future::lazy;
    /// my_executor.spawn(Box::new(lazy(|| {
    ///     println!("running on the executor");
    ///     Ok(())
    /// }))).unwrap();
    /// # }
    /// # fn main() {}
    /// ```
    fn spawn(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError>;

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
    /// # use tokio_executor::Executor;
    /// # fn docs(my_executor: &mut Executor) {
    /// use futures::future::lazy;
    ///
    /// if my_executor.status().is_ok() {
    ///     my_executor.spawn(Box::new(lazy(|| {
    ///         println!("running on the executor");
    ///         Ok(())
    ///     }))).unwrap();
    /// } else {
    ///     println!("the executor is not in a good state");
    /// }
    /// # }
    /// # fn main() {}
    /// ```
    fn status(&self) -> Result<(), SpawnError> {
        Ok(())
    }
}

impl<E: Executor + ?Sized> Executor for Box<E> {
    fn spawn(
        &mut self,
        future: Box<dyn Future<Item = (), Error = ()> + Send>,
    ) -> Result<(), SpawnError> {
        (**self).spawn(future)
    }

    fn status(&self) -> Result<(), SpawnError> {
        (**self).status()
    }
}
