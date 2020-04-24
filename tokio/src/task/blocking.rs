use crate::task::JoinHandle;

cfg_rt_threaded! {
    /// Runs the provided blocking function on the current thread without
    /// blocking the executor.
    ///
    /// In general, issuing a blocking call or performing a lot of compute in a
    /// future without yielding is not okay, as it may prevent the executor from
    /// driving other futures forward.  This function runs the closure on the
    /// current thread by having the thread temporarily cease from being a core
    /// thread, and turns it into a blocking thread. See the [CPU-bound tasks
    /// and blocking code][blocking] section for more information.
    ///
    /// Although this function avoids starving other independently spawned
    /// tasks, any other code running concurrently in the same task will be
    /// suspended during the call to `block_in_place`. This can happen e.g. when
    /// using the [`join!`] macro. To avoid this issue, use [`spawn_blocking`]
    /// instead.
    ///
    /// Note that this function can only be used on the [threaded scheduler].
    ///
    /// Code running behind `block_in_place` cannot be cancelled. When you shut
    /// down the executor, it will wait indefinitely for all blocking operations
    /// to finish. You can use [`shutdown_timeout`] to stop waiting for them
    /// after a certain timeout. Be aware that this will still not cancel the
    /// tasks — they are simply allowed to keep running after the method
    /// returns.
    ///
    /// [blocking]: ../index.html#cpu-bound-tasks-and-blocking-code
    /// [threaded scheduler]: fn@crate::runtime::Builder::threaded_scheduler
    /// [`spawn_blocking`]: fn@crate::task::spawn_blocking
    /// [`join!`]: ../macro.join.html
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
    #[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]
    pub fn block_in_place<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        crate::runtime::thread_pool::block_in_place(f)
    }
}

cfg_blocking! {
    /// Runs the provided closure on a thread where blocking is acceptable.
    ///
    /// In general, issuing a blocking call or performing a lot of compute in a
    /// future without yielding is not okay, as it may prevent the executor from
    /// driving other futures forward. This function runs the provided closure
    /// on a thread dedicated to blocking operations. See the [CPU-bound tasks
    /// and blocking code][blocking] section for more information.
    ///
    /// Tokio will spawn more blocking threads when they are requested through
    /// this function until the upper limit configured on the [`Builder`] is
    /// reached.  This limit is very large by default, because `spawn_blocking` is
    /// often used for various kinds of IO operations that cannot be performed
    /// asynchronously. When you run CPU-bound code using `spawn_blocking`, you
    /// should keep this large upper limit in mind; to run your CPU-bound
    /// computations on only a few threads, you should use a separate thread
    /// pool such as [rayon] rather than configuring the number of blocking
    /// threads.
    ///
    /// This function is intended for non-async operations that eventually
    /// finish on their own. If you want to spawn an ordinary thread, you should
    /// use [`thread::spawn`] instead.
    ///
    /// Closures spawned using `spawn_blocking` cannot be cancelled. When you
    /// shut down the executor, it will wait indefinitely for all blocking
    /// operations to finish. You can use [`shutdown_timeout`] to stop waiting
    /// for them after a certain timeout. Be aware that this will still not
    /// cancel the tasks — they are simply allowed to keep running after the
    /// method returns.
    ///
    /// Note that if you are using the [basic scheduler], this function will
    /// still spawn additional threads for blocking operations. The basic
    /// scheduler's single thread is only used for asynchronous code.
    ///
    /// [`Builder`]: struct@crate::runtime::Builder
    /// [blocking]: ../index.html#cpu-bound-tasks-and-blocking-code
    /// [rayon]: https://docs.rs/rayon
    /// [basic scheduler]: fn@crate::runtime::Builder::basic_scheduler
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
    pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        crate::runtime::spawn_blocking(f)
    }
}
