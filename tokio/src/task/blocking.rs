use crate::task::JoinHandle;

cfg_rt_multi_thread! {
    /// Runs the provided blocking function on the current thread without
    /// blocking the executor.
    ///
    /// In general, issuing a blocking call or performing a lot of compute in a
    /// future without yielding is problematic, as it may prevent the executor
    /// from driving other tasks forward. Calling this function informs the
    /// executor that the currently executing task is about to block the thread,
    /// so the executor is able to hand off any other tasks it has to a new
    /// worker thread before that happens. See the [CPU-bound tasks and blocking
    /// code][blocking] section for more information.
    ///
    /// Be aware that although this function avoids starving other independently
    /// spawned tasks, any other code running concurrently in the same task will
    /// be suspended during the call to `block_in_place`. This can happen e.g.
    /// when using the [`join!`] macro. To avoid this issue, use
    /// [`spawn_blocking`] instead of `block_in_place`.
    ///
    /// Note that this function cannot be used within a [`current_thread`] runtime
    /// because in this case there are no other worker threads to hand off tasks
    /// to. On the other hand, calling the function outside a runtime is
    /// allowed. In this case, `block_in_place` just calls the provided closure
    /// normally.
    ///
    /// Code running behind `block_in_place` cannot be cancelled. When you shut
    /// down the executor, it will wait indefinitely for all blocking operations
    /// to finish. You can use [`shutdown_timeout`] to stop waiting for them
    /// after a certain timeout. Be aware that this will still not cancel the
    /// tasks — they are simply allowed to keep running after the method
    /// returns.
    ///
    /// [blocking]: ../index.html#cpu-bound-tasks-and-blocking-code
    /// [`spawn_blocking`]: fn@crate::task::spawn_blocking
    /// [`join!`]: macro@join
    /// [`thread::spawn`]: fn@std::thread::spawn
    /// [`shutdown_timeout`]: fn@crate::runtime::Runtime::shutdown_timeout
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::task;
    ///
    /// # async fn docs() {
    /// task::block_in_place(move || {
    ///     // do some compute-heavy work or call synchronous code
    /// });
    /// # }
    /// ```
    ///
    /// Code running inside `block_in_place` may use `block_on` to reenter the
    /// async context.
    ///
    /// ```
    /// use tokio::task;
    /// use tokio::runtime::Handle;
    ///
    /// # async fn docs() {
    /// task::block_in_place(move || {
    ///     Handle::current().block_on(async move {
    ///         // do something async
    ///     });
    /// });
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if called from a [`current_thread`] runtime.
    ///
    /// [`current_thread`]: fn@crate::runtime::Builder::new_current_thread
    pub fn block_in_place<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        crate::runtime::thread_pool::block_in_place(f)
    }
}

cfg_rt! {
    /// Runs the provided closure on a thread where blocking is acceptable.
    ///
    /// In general, issuing a blocking call or performing a lot of compute in a
    /// future without yielding is problematic, as it may prevent the executor from
    /// driving other futures forward. This function runs the provided closure on a
    /// thread dedicated to blocking operations. See the [CPU-bound tasks and
    /// blocking code][blocking] section for more information.
    ///
    /// Tokio will spawn more blocking threads when they are requested through this
    /// function until the upper limit configured on the [`Builder`] is reached.
    /// This limit is very large by default, because `spawn_blocking` is often used
    /// for various kinds of IO operations that cannot be performed asynchronously.
    /// When you run CPU-bound code using `spawn_blocking`, you should keep this
    /// large upper limit in mind. When running many CPU-bound computations, a
    /// semaphore or some other synchronization primitive should be used to limit
    /// the number of computation executed in parallel. Specialized CPU-bound
    /// executors, such as [rayon], may also be a good fit.
    ///
    /// This function is intended for non-async operations that eventually finish on
    /// their own. If you want to spawn an ordinary thread, you should use
    /// [`thread::spawn`] instead.
    ///
    /// Closures spawned using `spawn_blocking` cannot be cancelled. When you shut
    /// down the executor, it will wait indefinitely for all blocking operations to
    /// finish. You can use [`shutdown_timeout`] to stop waiting for them after a
    /// certain timeout. Be aware that this will still not cancel the tasks — they
    /// are simply allowed to keep running after the method returns.
    ///
    /// Note that if you are using the single threaded runtime, this function will
    /// still spawn additional threads for blocking operations. The basic
    /// scheduler's single thread is only used for asynchronous code.
    ///
    /// [`Builder`]: struct@crate::runtime::Builder
    /// [blocking]: ../index.html#cpu-bound-tasks-and-blocking-code
    /// [rayon]: https://docs.rs/rayon
    /// [`thread::spawn`]: fn@std::thread::spawn
    /// [`shutdown_timeout`]: fn@crate::runtime::Runtime::shutdown_timeout
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::task;
    ///
    /// # async fn docs() -> Result<(), Box<dyn std::error::Error>>{
    /// let res = task::spawn_blocking(move || {
    ///     // do some compute-heavy work or call synchronous code
    ///     "done computing"
    /// }).await?;
    ///
    /// assert_eq!(res, "done computing");
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        crate::runtime::spawn_blocking(f)
    }
}
