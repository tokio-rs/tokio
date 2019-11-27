use crate::task::JoinHandle;

cfg_rt_threaded! {
    /// Run the provided blocking function without blocking the executor.
    ///
    /// In general, issuing a blocking call or performing a lot of compute in a
    /// future without yielding is not okay, as it may prevent the executor from
    /// driving other futures forward. If you run a closure through this method,
    /// the current executor thread will relegate all its executor duties to another
    /// (possibly new) thread, and only then poll the task. Note that this requires
    /// additional synchronization.
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
    pub fn block_in_place<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        use crate::runtime::{enter, thread_pool};

        enter::exit(|| thread_pool::block_in_place(f))
    }
}

cfg_blocking! {
    /// Run the provided closure on a thread where blocking is acceptable.
    ///
    /// In general, issuing a blocking call or performing a lot of compute in a future without
    /// yielding is not okay, as it may prevent the executor from driving other futures forward.
    /// A closure that is run through this method will instead be run on a dedicated thread pool for
    /// such blocking tasks without holding up the main futures executor.
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
