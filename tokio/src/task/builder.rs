#![allow(unreachable_pub)]
use crate::{
    runtime::{AutoBox, Handle},
    task::{JoinHandle, LocalSet},
    util::trace::SpawnMeta,
};
#[cfg(feature = "rt-multi-thread")]
use crate::runtime::{LlcTaskOptions, LlcTaskPlacement};
use std::{future::Future, io, mem};

/// Factory which is used to configure the properties of a new task.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// Methods can be chained in order to configure it.
///
/// The following configuration options are available:
///
/// - [`name`], which specifies an associated name for
///   the task
/// - [`llc_partition`], which selects a last-level-cache partition
/// - [`global_queue`], which bypasses LLC-aware placement
///
/// There are three types of task that can be spawned from a Builder:
/// - [`spawn_local`] for executing not [`Send`] futures
/// - [`spawn`] for executing [`Send`] futures on the runtime
/// - [`spawn_blocking`] for executing blocking code in the
///   blocking thread pool.
///
/// ## Example
///
/// ```no_run
/// use tokio::net::{TcpListener, TcpStream};
///
/// use std::io;
///
/// async fn process(socket: TcpStream) {
///     // ...
/// # drop(socket);
/// }
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let listener = TcpListener::bind("127.0.0.1:8080").await?;
///
///     loop {
///         let (socket, _) = listener.accept().await?;
///
///         tokio::task::Builder::new()
///             .name("tcp connection handler")
///             .spawn(async move {
///                 // Process each socket concurrently.
///                 process(socket).await
///             })?;
///     }
/// }
/// ```
/// [unstable]: crate#unstable-features
/// [`name`]: Builder::name
/// [`llc_partition`]: Builder::llc_partition
/// [`global_queue`]: Builder::global_queue
/// [`spawn_local`]: Builder::spawn_local
/// [`spawn`]: Builder::spawn
/// [`spawn_blocking`]: Builder::spawn_blocking
#[derive(Default, Debug)]
#[cfg_attr(
    docsrs,
    doc(cfg(all(
        tokio_unstable,
        any(feature = "tracing", feature = "rt-multi-thread")
    )))
)]
pub struct Builder<'a> {
    name: Option<&'a str>,
    #[cfg(feature = "rt-multi-thread")]
    llc: LlcTaskOptions,
}

impl<'a> Builder<'a> {
    /// Creates a new task builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Assigns a name to the task which will be spawned.
    pub fn name(&self, name: &'a str) -> Self {
        Self {
            name: Some(name),
            #[cfg(feature = "rt-multi-thread")]
            llc: self.llc,
        }
    }

    /// Assigns the task to an LLC partition when it enters a shared scheduler
    /// queue.
    ///
    /// Tasks spawned from a runtime worker continue to use that worker's
    /// private queue when possible. The partition hint takes effect whenever
    /// the task must be scheduled through a shared queue. An out-of-range
    /// partition falls back to the global queue. The hint has no effect when
    /// LLC-aware scheduling is disabled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> std::io::Result<()> {
    /// let task = tokio::task::Builder::new()
    ///     .llc_partition(0)
    ///     .spawn(async {})?;
    /// task.await.unwrap();
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rt-multi-thread")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt-multi-thread")))]
    pub fn llc_partition(mut self, partition: usize) -> Self {
        self.llc.placement = Some(LlcTaskPlacement::Partition(partition));
        self
    }

    /// Routes the task through the global queue instead of an LLC queue.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> std::io::Result<()> {
    /// let task = tokio::task::Builder::new()
    ///     .global_queue()
    ///     .spawn(async {})?;
    /// task.await.unwrap();
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "rt-multi-thread")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rt-multi-thread")))]
    pub fn global_queue(mut self) -> Self {
        self.llc.placement = Some(LlcTaskPlacement::Global);
        self
    }

    #[track_caller]
    fn spawn_meta(&self, original_size: usize) -> SpawnMeta<'a> {
        let meta = SpawnMeta::new(self.name, original_size);
        #[cfg(feature = "rt-multi-thread")]
        return meta.with_llc_options(self.llc);
        #[cfg(not(feature = "rt-multi-thread"))]
        meta
    }

    /// Spawns a task with this builder's settings on the current runtime.
    ///
    /// # Panics
    ///
    /// This method panics if called outside of a Tokio runtime.
    ///
    /// See [`task::spawn`](crate::task::spawn()) for
    /// more details.
    #[track_caller]
    pub fn spawn<Fut>(self, future: Fut) -> io::Result<JoinHandle<Fut::Output>>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let fut_size = mem::size_of::<Fut>();
        let meta = self.spawn_meta(fut_size);
        Ok(if AutoBox::<Fut>::SHOULD_BOX {
            super::spawn::spawn_inner(Box::pin(future), meta)
        } else {
            super::spawn::spawn_inner(future, meta)
        })
    }

    /// Spawn a task with this builder's settings on the provided [runtime
    /// handle].
    ///
    /// See [`Handle::spawn`] for more details.
    ///
    /// [runtime handle]: crate::runtime::Handle
    /// [`Handle::spawn`]: crate::runtime::Handle::spawn
    #[track_caller]
    pub fn spawn_on<Fut>(self, future: Fut, handle: &Handle) -> io::Result<JoinHandle<Fut::Output>>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let fut_size = mem::size_of::<Fut>();
        let meta = self.spawn_meta(fut_size);
        Ok(if AutoBox::<Fut>::SHOULD_BOX {
            handle.spawn_named(Box::pin(future), meta)
        } else {
            handle.spawn_named(future, meta)
        })
    }

    /// Spawns a `!Send` task on the current [`LocalSet`] or [`LocalRuntime`] with
    /// this builder's settings.
    ///
    /// The spawned future will be run on the same thread that called `spawn_local`.
    /// This may only be called from the context of a [local task set][`LocalSet`]
    /// or a [`LocalRuntime`].
    ///
    /// # Panics
    ///
    /// This function panics if called outside of a [local task set][`LocalSet`]
    /// or a [`LocalRuntime`].
    ///
    /// See [`task::spawn_local`] for more details.
    ///
    /// [`task::spawn_local`]: crate::task::spawn_local
    /// [`LocalSet`]: crate::task::LocalSet
    /// [`LocalRuntime`]: crate::runtime::LocalRuntime
    #[track_caller]
    pub fn spawn_local<Fut>(self, future: Fut) -> io::Result<JoinHandle<Fut::Output>>
    where
        Fut: Future + 'static,
        Fut::Output: 'static,
    {
        let fut_size = mem::size_of::<Fut>();
        let meta = self.spawn_meta(fut_size);
        Ok(if AutoBox::<Fut>::SHOULD_BOX {
            super::local::spawn_local_inner(Box::pin(future), meta)
        } else {
            super::local::spawn_local_inner(future, meta)
        })
    }

    /// Spawns `!Send` a task on the provided [`LocalSet`] with this builder's
    /// settings.
    ///
    /// See [`LocalSet::spawn_local`] for more details.
    ///
    /// [`LocalSet::spawn_local`]: crate::task::LocalSet::spawn_local
    /// [`LocalSet`]: crate::task::LocalSet
    #[track_caller]
    pub fn spawn_local_on<Fut>(
        self,
        future: Fut,
        local_set: &LocalSet,
    ) -> io::Result<JoinHandle<Fut::Output>>
    where
        Fut: Future + 'static,
        Fut::Output: 'static,
    {
        let fut_size = mem::size_of::<Fut>();
        let meta = self.spawn_meta(fut_size);
        Ok(if AutoBox::<Fut>::SHOULD_BOX {
            local_set.spawn_named(Box::pin(future), meta)
        } else {
            local_set.spawn_named(future, meta)
        })
    }

    /// Spawns blocking code on the blocking threadpool.
    ///
    /// # Panics
    ///
    /// This method panics if called outside of a Tokio runtime.
    ///
    /// See [`task::spawn_blocking`](crate::task::spawn_blocking)
    /// for more details.
    #[track_caller]
    pub fn spawn_blocking<Function, Output>(
        self,
        function: Function,
    ) -> io::Result<JoinHandle<Output>>
    where
        Function: FnOnce() -> Output + Send + 'static,
        Output: Send + 'static,
    {
        let handle = Handle::current();
        self.spawn_blocking_on(function, &handle)
    }

    /// Spawns blocking code on the provided [runtime handle]'s blocking threadpool.
    ///
    /// See [`Handle::spawn_blocking`] for more details.
    ///
    /// [runtime handle]: crate::runtime::Handle
    /// [`Handle::spawn_blocking`]: crate::runtime::Handle::spawn_blocking
    #[track_caller]
    pub fn spawn_blocking_on<Function, Output>(
        self,
        function: Function,
        handle: &Handle,
    ) -> io::Result<JoinHandle<Output>>
    where
        Function: FnOnce() -> Output + Send + 'static,
        Output: Send + 'static,
    {
        use crate::runtime::Mandatory;
        let fn_size = mem::size_of::<Function>();
        let meta = self.spawn_meta(fn_size);
        let (join_handle, spawn_result) = if AutoBox::<Function>::SHOULD_BOX {
            handle.inner.blocking_spawner().spawn_blocking_inner(
                Box::new(function),
                Mandatory::NonMandatory,
                meta,
                handle,
            )
        } else {
            handle.inner.blocking_spawner().spawn_blocking_inner(
                function,
                Mandatory::NonMandatory,
                meta,
                handle,
            )
        };

        spawn_result?;
        Ok(join_handle)
    }
}
