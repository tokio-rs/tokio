#![allow(unreachable_pub)]
use crate::{
    runtime::{Handle, BOX_FUTURE_THRESHOLD},
    task::{JoinHandle, LocalSet},
    util::trace::SpawnMeta,
};
use std::{any::Any, fmt, future::Future, io, mem};

/// Factory which is used to configure the properties of a new task.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// Methods can be chained in order to configure it.
///
/// Configuration options include:
///
/// - [`name`], which specifies an associated name for
///   the task
/// - [`data`], which stores user data for runtime task hooks
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
/// [`data`]: Builder::data
/// [`spawn_local`]: Builder::spawn_local
/// [`spawn`]: Builder::spawn
/// [`spawn_blocking`]: Builder::spawn_blocking
#[derive(Default)]
#[cfg_attr(docsrs, doc(cfg(all(tokio_unstable, feature = "tracing"))))]
pub struct Builder<'a> {
    name: Option<&'a str>,
    data: Option<crate::runtime::TaskData>,
}

impl<'a> Builder<'a> {
    /// Creates a new task builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Assigns a name to the task which will be spawned.
    pub fn name(self, name: &'a str) -> Self {
        Self {
            name: Some(name),
            ..self
        }
    }

    /// Sets task data visible to runtime task hooks.
    pub fn data<T>(self, data: T) -> Self
    where
        T: Any + Send + Sync + 'static,
    {
        Self {
            data: Some(Box::new(data)),
            ..self
        }
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
        let Builder { name, data } = self;
        let fut_size = mem::size_of::<Fut>();
        Ok(if fut_size > BOX_FUTURE_THRESHOLD {
            super::spawn::spawn_inner(Box::pin(future), SpawnMeta::new(name, fut_size), data)
        } else {
            super::spawn::spawn_inner(future, SpawnMeta::new(name, fut_size), data)
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
        let Builder { name, data } = self;
        let fut_size = mem::size_of::<Fut>();
        Ok(if fut_size > BOX_FUTURE_THRESHOLD {
            handle.spawn_named_with_data(Box::pin(future), SpawnMeta::new(name, fut_size), data)
        } else {
            handle.spawn_named_with_data(future, SpawnMeta::new(name, fut_size), data)
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
        let Builder { name, data } = self;
        let fut_size = mem::size_of::<Fut>();
        Ok(if fut_size > BOX_FUTURE_THRESHOLD {
            super::local::spawn_local_inner(Box::pin(future), SpawnMeta::new(name, fut_size), data)
        } else {
            super::local::spawn_local_inner(future, SpawnMeta::new(name, fut_size), data)
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
        let Builder { name, data } = self;
        let fut_size = mem::size_of::<Fut>();
        Ok(if fut_size > BOX_FUTURE_THRESHOLD {
            local_set.spawn_named_with_data(Box::pin(future), SpawnMeta::new(name, fut_size), data)
        } else {
            local_set.spawn_named_with_data(future, SpawnMeta::new(name, fut_size), data)
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
        let Builder { name, data } = self;
        let fn_size = mem::size_of::<Function>();
        let (join_handle, spawn_result) = if fn_size > BOX_FUTURE_THRESHOLD {
            handle.inner.blocking_spawner().spawn_blocking_inner(
                Box::new(function),
                Mandatory::NonMandatory,
                SpawnMeta::new(name, fn_size),
                handle,
                data,
            )
        } else {
            handle.inner.blocking_spawner().spawn_blocking_inner(
                function,
                Mandatory::NonMandatory,
                SpawnMeta::new(name, fn_size),
                handle,
                data,
            )
        };

        spawn_result?;
        Ok(join_handle)
    }
}

impl fmt::Debug for Builder<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Builder")
            .field("name", &self.name)
            .field("data", &self.data.as_ref().map(|_| "<task data>"))
            .finish()
    }
}
