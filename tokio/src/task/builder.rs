#![allow(unreachable_pub)]
use crate::{runtime::context, task::JoinHandle};
use std::future::Future;

/// Factory which is used to configure the properties of a new task.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// Methods can be chained in order to configure it.
///
/// Currently, there is only one configuration option:
///
/// - [`name`], which specifies an associated name for
///   the task
///
/// There are three types of task that can be spawned from a Builder:
/// - [`spawn_local`] for executing futures on the current thread
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
///             });
///     }
/// }
/// ```
/// [unstable]: crate#unstable-features
/// [`name`]: Builder::name
/// [`spawn_local`]: Builder::spawn_local
/// [`spawn`]: Builder::spawn
/// [`spawn_blocking`]: Builder::spawn_blocking
#[derive(Default, Debug)]
#[cfg_attr(docsrs, doc(cfg(all(tokio_unstable, feature = "tracing"))))]
pub struct Builder<'a> {
    name: Option<&'a str>,
}

impl<'a> Builder<'a> {
    /// Creates a new task builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Assigns a name to the task which will be spawned.
    pub fn name(&self, name: &'a str) -> Self {
        Self { name: Some(name) }
    }

    /// Spawns a task on the executor.
    ///
    /// See [`task::spawn`](crate::task::spawn) for
    /// more details.
    #[track_caller]
    pub fn spawn<Fut>(self, future: Fut) -> JoinHandle<Fut::Output>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        super::spawn::spawn_inner(future, self.name)
    }

    /// Spawns a task on the current thread.
    ///
    /// See [`task::spawn_local`](crate::task::spawn_local)
    /// for more details.
    #[track_caller]
    pub fn spawn_local<Fut>(self, future: Fut) -> JoinHandle<Fut::Output>
    where
        Fut: Future + 'static,
        Fut::Output: 'static,
    {
        super::local::spawn_local_inner(future, self.name)
    }

    /// Spawns blocking code on the blocking threadpool.
    ///
    /// See [`task::spawn_blocking`](crate::task::spawn_blocking)
    /// for more details.
    #[track_caller]
    pub fn spawn_blocking<Function, Output>(self, function: Function) -> JoinHandle<Output>
    where
        Function: FnOnce() -> Output + Send + 'static,
        Output: Send + 'static,
    {
        use crate::runtime::Mandatory;
        let handle = context::current();
        let (join_handle, _was_spawned) = handle.as_inner().spawn_blocking_inner(
            function,
            Mandatory::NonMandatory,
            self.name,
            &handle,
        );
        join_handle
    }
}
