use crate::util::rand::{RngSeed, RngSeedGenerator};

/// How the runtime should respond to unhandled panics.
///
/// Instances of `UnhandledPanic` are passed to `Builder::unhandled_panic`
/// to configure the runtime behavior when a spawned task panics.
///
/// See [`Builder::unhandled_panic`] for more details.
///
/// [`Builder::unhandled_panic`]: super::Builder::unhandled_panic
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum UnhandledPanic {
    /// The runtime should ignore panics on spawned tasks.
    ///
    /// The panic is forwarded to the task's [`JoinHandle`] and all spawned
    /// tasks continue running normally.
    ///
    /// This is the default behavior.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::{self, UnhandledPanic};
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_current_thread()
    ///     .unhandled_panic(UnhandledPanic::Ignore)
    ///     .build()
    ///     .unwrap();
    ///
    /// let task1 = rt.spawn(async { panic!("boom"); });
    /// let task2 = rt.spawn(async {
    ///     // This task completes normally
    ///     "done"
    /// });
    ///
    /// rt.block_on(async {
    ///     // The panic on the first task is forwarded to the `JoinHandle`
    ///     assert!(task1.await.is_err());
    ///
    ///     // The second task completes normally
    ///     assert!(task2.await.is_ok());
    /// })
    /// # }
    /// ```
    ///
    /// [`JoinHandle`]: struct@crate::task::JoinHandle
    Ignore,

    /// The runtime should immediately shutdown if a spawned task panics.
    ///
    /// The runtime will immediately shutdown even if the panicked task's
    /// [`JoinHandle`] is still available. All further spawned tasks will be
    /// immediately dropped and call to [`Runtime::block_on`] will panic.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use tokio::runtime::{self, UnhandledPanic};
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_current_thread()
    ///     .unhandled_panic(UnhandledPanic::ShutdownRuntime)
    ///     .build()
    ///     .unwrap();
    ///
    /// rt.spawn(async { panic!("boom"); });
    /// rt.spawn(async {
    ///     // This task never completes.
    /// });
    ///
    /// rt.block_on(async {
    ///     // Do some work
    /// # loop { tokio::task::yield_now().await; }
    /// })
    /// # }
    /// ```
    ///
    /// [`JoinHandle`]: struct@crate::task::JoinHandle
    ShutdownRuntime,
}

impl super::Builder {
    /// Configure how the runtime responds to an unhandled panic on a
    /// spawned task.
    ///
    /// By default, an unhandled panic (i.e. a panic not caught by
    /// [`std::panic::catch_unwind`]) has no impact on the runtime's
    /// execution. The panic is error value is forwarded to the task's
    /// [`JoinHandle`] and all other spawned tasks continue running.
    ///
    /// The `unhandled_panic` option enables configuring this behavior.
    ///
    /// * `UnhandledPanic::Ignore` is the default behavior. Panics on
    ///   spawned tasks have no impact on the runtime's execution.
    /// * `UnhandledPanic::ShutdownRuntime` will force the runtime to
    ///   shutdown immediately when a spawned task panics even if that
    ///   task's `JoinHandle` has not been dropped. All other spawned tasks
    ///   will immediately terminate and further calls to
    ///   [`Runtime::block_on`] will panic.
    ///
    /// # Unstable
    ///
    /// This option is currently unstable and its implementation is
    /// incomplete. The API may change or be removed in the future. See
    /// tokio-rs/tokio#4516 for more details.
    ///
    /// # Examples
    ///
    /// The following demonstrates a runtime configured to shutdown on
    /// panic. The first spawned task panics and results in the runtime
    /// shutting down. The second spawned task never has a chance to
    /// execute. The call to `block_on` will panic due to the runtime being
    /// forcibly shutdown.
    ///
    /// ```should_panic
    /// use tokio::runtime::{self, UnhandledPanic};
    ///
    /// # pub fn main() {
    /// let rt = runtime::Builder::new_current_thread()
    ///     .unhandled_panic(UnhandledPanic::ShutdownRuntime)
    ///     .build()
    ///     .unwrap();
    ///
    /// rt.spawn(async { panic!("boom"); });
    /// rt.spawn(async {
    ///     // This task never completes.
    /// });
    ///
    /// rt.block_on(async {
    ///     // Do some work
    /// # loop { tokio::task::yield_now().await; }
    /// })
    /// # }
    /// ```
    ///
    /// [`JoinHandle`]: struct@crate::task::JoinHandle
    pub fn unhandled_panic(&mut self, behavior: UnhandledPanic) -> &mut Self {
        self.unhandled_panic = behavior;
        self
    }

    /// Disables the LIFO task scheduler heuristic.
    ///
    /// The multi-threaded scheduler includes a heuristic for optimizing
    /// message-passing patterns. This heuristic results in the **last**
    /// scheduled task being polled first.
    ///
    /// To implement this heuristic, each worker thread has a slot which
    /// holds the task that should be polled next. However, this slot cannot
    /// be stolen by other worker threads, which can result in lower total
    /// throughput when tasks tend to have longer poll times.
    ///
    /// This configuration option will disable this heuristic resulting in
    /// all scheduled tasks being pushed into the worker-local queue, which
    /// is stealable.
    ///
    /// Consider trying this option when the task "scheduled" time is high
    /// but the runtime is underutilized. Use tokio-rs/tokio-metrics to
    /// collect this data.
    ///
    /// # Unstable
    ///
    /// This configuration option is considered a workaround for the LIFO
    /// slot not being stealable. When the slot becomes stealable, we will
    /// revisit whether or not this option is necessary. See
    /// tokio-rs/tokio#4941.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime;
    ///
    /// let rt = runtime::Builder::new_multi_thread()
    ///     .disable_lifo_slot()
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn disable_lifo_slot(&mut self) -> &mut Self {
        self.disable_lifo_slot = true;
        self
    }

    /// Specifies the random number generation seed to use within all
    /// threads associated with the runtime being built.
    ///
    /// This option is intended to make certain parts of the runtime
    /// deterministic (e.g. the [`tokio::select!`] macro). In the case of
    /// [`tokio::select!`] it will ensure that the order that branches are
    /// polled is deterministic.
    ///
    /// In addition to the code specifying `rng_seed` and interacting with
    /// the runtime, the internals of Tokio and the Rust compiler may affect
    /// the sequences of random numbers. In order to ensure repeatable
    /// results, the version of Tokio, the versions of all other
    /// dependencies that interact with Tokio, and the Rust compiler version
    /// should also all remain constant.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime::{self, RngSeed};
    /// # pub fn main() {
    /// let seed = RngSeed::from_bytes(b"place your seed here");
    /// let rt = runtime::Builder::new_current_thread()
    ///     .rng_seed(seed)
    ///     .build();
    /// # }
    /// ```
    ///
    /// [`tokio::select!`]: crate::select
    pub fn rng_seed(&mut self, seed: RngSeed) -> &mut Self {
        self.seed_generator = RngSeedGenerator::new(seed);
        self
    }
}
