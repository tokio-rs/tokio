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
    /// ...
    #[track_caller]
    pub fn block_in_place<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        crate::runtime::scheduler::block_in_place(f)
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
    /// After reaching the upper limit, the tasks are put in a queue.
    /// The thread limit is very large by default, because `spawn_blocking` is often
    /// used for various kinds of IO operations that cannot be performed
    /// asynchronously.  When you run CPU-bound code using `spawn_blocking`, you
    /// should keep this large upper limit in mind. When running many CPU-bound
    /// computations, a semaphore or some other synchronization primitive should be
    /// used to limit the number of computations executed in parallel. Specialized
    /// CPU-bound executors, such as [rayon], may also be a good fit.
    ///
    /// This function is intended for non-async operations that eventually finish on
    /// their own. If you want to spawn an ordinary thread, you should use
    /// [`thread::spawn`] instead.
    ///
    /// # When to use `spawn_blocking` vs dedicated threads
    ///
    /// `spawn_blocking` is intended for *bounded* blocking work that eventually
    /// finishes. Each call occupies a thread from the runtime's blocking thread
    /// pool for the duration of the task.
    ///
    /// Although the default upper limit for blocking threads is large, long-lived
    /// tasks reduce the pool's effective capacity. Once the limit is reached,
    /// additional blocking tasks will be queued, potentially delaying other
    /// operations that rely on the blocking pool.
    ///
    /// For workloads that run indefinitely or for extended periods (for example,
    /// background workers or persistent processing loops), it is generally better
    /// to use a dedicated thread created with [`thread::spawn`].
    ///
    /// As a rule of thumb:
    /// - Use `spawn_blocking` for short-lived, bounded blocking operations
    /// - Use dedicated threads for long-lived or persistent blocking workloads
    ///
    /// Be aware that tasks spawned using `spawn_blocking` cannot be aborted
    /// because they are not async. If you call [`abort`] on a `spawn_blocking`
    /// task, then this *will not have any effect*, and the task will continue
    /// running normally. The exception is if the task has not started running
    /// yet; in that case, calling `abort` may prevent the task from starting.
    ///
    /// When you shut down the executor, it will attempt to `abort` all tasks
    /// including `spawn_blocking` tasks. However, `spawn_blocking` tasks
    /// cannot be aborted once they start running, which means that runtime
    /// shutdown will wait indefinitely for all started `spawn_blocking` to
    /// finish running. You can use [`shutdown_timeout`] to stop waiting for
    /// them after a certain timeout. Be aware that this will still not cancel
    /// the tasks — they are simply allowed to keep running after the method
    /// returns. It is possible for a blocking task to be cancelled if it has
    /// not yet started running, but this is not guaranteed.
    ///
    /// Note that if you are using the single threaded runtime, this function will
    /// still spawn additional threads for blocking operations. The current-thread
    /// scheduler's single thread is only used for asynchronous code.
    ///
    /// # Related APIs and patterns for bridging asynchronous and blocking code
    /// ...
    #[track_caller]
    pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        crate::runtime::spawn_blocking(f)
    }
}
