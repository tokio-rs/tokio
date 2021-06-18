#![allow(unreachable_pub)]
use crate::task::JoinHandle;
use std::future::Future;

/// Factory which is used to configure the properties of a new task.
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
#[derive(Default, Debug)]
pub struct Builder<'a> {
    name: Option<&'a str>,
}

impl Builder<'_> {
    /// Creates a new task builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Assigns a name to the task which will be spawned
    pub fn name<'b>(&self, name: &'b str) -> Builder<'b> {
        Builder { name: Some(name) }
    }

    /// Spawns a task on the executor.
    ///
    /// See
    /// [`task::spawn`][crate::task::spawn] for more details
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn spawn<T>(self, future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        super::spawn::spawn_inner(future, self.name)
    }

    /// Spawns a task on the current thread.
    ///
    /// See [`task::spawn_local`][crate::task::spawn_local]
    /// for more details
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn spawn_local<F>(self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        super::local::spawn_local_inner(future, self.name)
    }

    /// Spawns blocking code on the blocking threadpool.
    ///
    /// See [`task::spawn_blocking`][crate::task::spawn_blocking]
    /// for more details
    #[cfg_attr(tokio_track_caller, track_caller)]
    pub fn spawn_blocking<Function, Output>(self, function: Function) -> JoinHandle<Output>
    where
        Function: FnOnce() -> Output + Send + 'static,
        Output: Send + 'static,
    {
        #[cfg(tokio_track_caller)]
        let location = std::panic::Location::caller();
        #[cfg(tokio_track_caller)]
        let span = tracing::trace_span!(
            target: "tokio::task",
            "task",
            kind = "blocking",
            spawn.location = %format_args!("{}:{}:{}", location.file(), location.line(), location.column()),
            task.name = %self.name.unwrap_or_default()
        );

        #[cfg(not(tokio_track_caller))]
        let span = tracing::trace_span!(
            target: "tokio::task",
            "task",
            kind = "blocking",
            task.name = %self.name.unwrap_or_default()
        );

        crate::runtime::spawn_blocking(move || {
            let _span = span.entered();
            function()
        })
    }
}
