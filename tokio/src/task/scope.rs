//! Tools for structuring concurrent tasks
//!
//! Tokio tasks can run completely independent of each other. However it is
//! often useful to group tasks which try to fulfill a common goal.
//! These groups of tasks should share the same lifetime. If the task group is
//! no longer needed all tasks should stop. If one task errors, the other tasks
//! might no longer be needed and should also be cancelled.
//!
//! The utilities inside this module allow to group tasks by following the
//! concept of structured concurrency.

use crate::{
    sync::{CancellationToken, SharedWaitGroup},
    task::{JoinError, JoinHandle},
};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// Creates and enters a task scope
///
/// The `scope` allows to spawn child tasks so that the lifetime of child tasks
/// is constrained within the scope.
///
/// If the provided `Future` had been polled to completion, all child tasks which
/// have been spawned via [`scope::spawn`] and [`scope::spawn_cancellable`] are
/// guaranteed to have run to completion.
///
/// `enter` returns a [`Future`] which must be awaited. The `await` will only
/// complete once all child tasks that have been spawned via the provided
/// [`ScopeHandle`] have joined. Thereby the `scope` does not allow child tasks
/// to outlive their parent task, as long as the future returned from
/// `scope::enter` is awaited.
///
/// The `Future` returned from `enter` will evaluate the value which is returned
/// from the async function inside the scope.
///
/// Since scopes need to run to completion they should not be be started on
/// tasks which have been spawned using the [`spawn_cancellable`] function, since
/// this function will force-cancel running tasks on it.
///
/// Instead new scopes should always be created from gracefully cancellable tasks
/// which have been stared using the [`scope::spawn`] method.
///
/// # Examples
///
/// ```no_run
/// use tokio::task::scope;
///
/// #[tokio::main]
/// async fn scope_with_graceful_cancellation() {
///     let result = scope::enter(async move {
///         // This is the main task which will finish after 20ms
///         let handle = scope::spawn(async {
///             tokio::time::delay_for(std::time::Duration::from_millis(20)).await;
///             println!("Cancelling");
///             scope::current_cancellation_token().cancel();
///             123u32
///         });
///
///         // Spawn a long running task which is not intended to run to completion
///         let _ = scope::spawn(async {
///             let ct = scope::current_cancellation_token();
///             tokio::select! {
///                 _ = ct.cancelled() => {
///                     // This branch will be taken once the scope is left
///                     println!("task was cancelled");
///                 },
///                 _ = tokio::time::delay_for(std::time::Duration::from_secs(3600)) => {
///                     panic!("This task should not run to completion");
///                 },
///             }
///         }).await;
///
///         // Wait for the main task. After this finishes the scope will end.
///         // Thereby the remaining task will get cancelled, and awaited before
///         // `scope::enter` returns.
///         handle.await
///     })
///     .await;
///
///     assert_eq!(123, result.unwrap());
/// }
/// ```
pub async fn enter<Fut, R>(scope_fut: Fut) -> R
where
    Fut: Future<Output = R> + Send,
{
    let child_scope = match CURRENT_SCOPE.try_with(|scope_handle| scope_handle.clone()) {
        Ok(scope) => scope.child(),
        Err(_) => Scope::detached(),
    };

    child_scope.enter(scope_fut).await
}

/// Spawns a task on the current scope which will run to completion.
///
/// If the parent scope is cancelled the task will be informed via through a
/// [`CancellationToken`] whose cancellation state can be queried using
/// [`scope::current_cancellation_token`]. If cancellation is requested, the
/// task should return as early as possible.
pub fn spawn<T, R>(task: T) -> ScopedJoinHandle<R>
where
    T: Future<Output = R> + Send + 'static,
    R: Send + 'static,
    T: 'static,
{
    let current_scope_handle = CURRENT_SCOPE
        .try_with(|scope_handle| scope_handle.clone())
        .unwrap();

    current_scope_handle.spawn(task)
}

/// Spawns a task on the current scope which will automatically get force-cancelled
/// if the parent if the `scope` gets cancelled. That spawned task therefore is
/// not guaranteed to run to completion.
///
/// Spawning a task using [`scope::spawn_cancellable`] is equivalent to spawning
/// it with [`scope::spawn`] and aborting execution when the tasks
/// [`CancellationToken`] was signalled:
///
/// ```no_run
/// # use std::future::Future;
/// use tokio::task::scope;
///
/// fn spawn_cancellable<T, R>(task: T) -> scope::ScopedJoinHandle<Result<R, scope::CancellationError>>
/// where
///     T: Future<Output = R> + Send + 'static,
///     R: Send + 'static,
///     T: 'static,
/// {
///     scope::spawn(async {
///         let ct = scope::current_cancellation_token();
///         tokio::select! {
///             _ = ct.cancelled() => {
///                 Err(scope::CancellationError{})
///             },
///             result = task => {
///                 Ok(result)
///             },
///         }
///     })
/// }
/// ```
///
/// On tasks spawned via `spawn_cancellable` no new task scopes should be created
/// via `scope::enter`, since they are not guaranteed to run to completion. If the
/// a task gets force cancelled while a scope is active inside the task a
/// runtime panic will be emitted.
pub fn spawn_cancellable<T, R>(task: T) -> ScopedJoinHandle<Result<R, CancellationError>>
where
    T: Future<Output = R> + Send + 'static,
    R: Send + 'static,
    T: 'static,
{
    let current_scope_handle = CURRENT_SCOPE
        .try_with(|scope_handle| scope_handle.clone())
        .unwrap();

    current_scope_handle.spawn_cancellable(task)
}

/// Returns the [`CancellationToken`] which is associated with the currently
/// running task and `scope`.
/// If the current `scope` gets cancelled the `CancellationToken` will be signalled
pub fn current_cancellation_token() -> CancellationToken {
    // TODO: We could also return an Option<CancellationToken> here, but that is
    // somewhat inconvenient to use.
    // Or we wrap Option<CancellationToken> also in a struct which can product
    // a `.cancelled()` `Future`, which would never resolve in case the token
    // is `None`.
    CURRENT_SCOPE
        .try_with(|scope_handle| scope_handle.cancellation_token().clone())
        .unwrap_or_else(|_| CancellationToken::new())
}

/// Holds the current task-local [`ScopeHandle`]
static CURRENT_SCOPE: crate::task::LocalKey<ScopeHandle> = {
    std::thread_local! {
        static __KEY: std::cell::RefCell<Option<ScopeHandle>> = std::cell::RefCell::new(None);
    }

    crate::task::LocalKey { inner: __KEY }
};

/// Error type which is returned when a force-cancellable task was cancelled and
/// did not run to completion.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct CancellationError {}

pin_project! {
    /// Allows to wait for a child task to join
    pub struct ScopedJoinHandle<T> {
        #[pin]
        handle: JoinHandle<T>,
    }
}

impl<T> Future for ScopedJoinHandle<T> {
    // TODO: Assuming the runtime semantics are adapted to suite structured
    /// concurrency better, the `JoinError` might not be necessary here.
    /// - For gracefully cancelled tasks the runtime would need to wait until the
    ///   tasks finished. In this case the task would never be aborted and
    ///   JoinError is not necessary.
    /// - For forcefully cancelled tasks which have been spawned using
    ///   `spawn_cancellable` the join error in form of JoinError/CancellationError
    ///   is still necessary - but not a nested version.
    type Output = Result<T, JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().handle.poll(cx)
    }
}

#[derive(Clone)]
struct ScopeState {
    wait_group: SharedWaitGroup,
    config: ScopeConfig,
}

impl ScopeState {
    fn new(config: ScopeConfig) -> Self {
        Self {
            config,
            wait_group: SharedWaitGroup::new(),
        }
    }
}

struct CancelTasksGuard<'a> {
    scope: &'a CancellationToken,
}

impl<'a> Drop for CancelTasksGuard<'a> {
    fn drop(&mut self) {
        self.scope.cancel();
    }
}

struct WaitForTasksToJoinGuard<'a> {
    _wait_group: &'a SharedWaitGroup,
    drop_behavior: ScopeDropBehavior,
    enabled: bool,
}

impl<'a> WaitForTasksToJoinGuard<'a> {
    fn disarm(&mut self) {
        self.enabled = false;
    }
}

impl<'a> Drop for WaitForTasksToJoinGuard<'a> {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }

        match self.drop_behavior {
            ScopeDropBehavior::Panic => {
                panic!("Scope was dropped before child tasks run to completion");
            }
        }
    }
}

/// A handle to the scope, which allows to spawn child tasks
#[derive(Clone)]
struct ScopeHandle {
    scope: ScopeState,
}

impl core::fmt::Debug for ScopeHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ScopeHandle").finish()
    }
}

impl ScopeHandle {
    /// Returns a reference to the `CancellationToken` which signals whether the
    /// scope had been cancelled.
    fn cancellation_token(&self) -> &CancellationToken {
        &self.scope.config.cancellation_token
    }

    /// Creates a child scope.
    /// The child scope inherit all properties of this scope
    fn child(&self) -> Scope {
        Scope::with_parent(self.clone())
    }

    /// Spawns a task on the scope
    fn spawn<T, R>(&self, task: T) -> ScopedJoinHandle<R>
    where
        T: Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        let spawn_handle =
            crate::runtime::context::spawn_handle().expect("Spawn handle must be available");

        // Add a wait handle
        // This must happen BEFORE we spawn the child task - otherwise this is
        // would be racy.
        let releaser = self.scope.wait_group.add();

        let self_clone = self.clone();
        let child_task: JoinHandle<R> = spawn_handle.spawn(async move {
            // Drop this at the end of the task to signal we are done and unblock
            // the WaitGroup
            let _wait_group_releaser = releaser;

            // Set the thread local scope handle so that the child task inherits
            // the properties from the parent and execute it.
            // TODO: In case the properties would already be required for spawning
            // (e.g. in order to bind a task to a certain runtime thread) this
            // place would already be too late. This properties would rather
            // need to be passed to `runtime::context::spawn_handle()`.
            CURRENT_SCOPE.scope(self_clone, task).await
        });

        ScopedJoinHandle { handle: child_task }
    }

    /// Spawns a task on the scope, which will get automatically cancelled if
    /// the `CancellationToken` which is associated with the current `ScopeHandle`
    /// gets cancelled.
    fn spawn_cancellable<'inner, T, R>(
        &'inner self,
        task: T,
    ) -> ScopedJoinHandle<Result<R, CancellationError>>
    where
        T: Future<Output = R> + Send + 'static,
        R: Send + 'static,
        T: 'inner,
    {
        let cancel_token = self.cancellation_token().clone();

        self.spawn(async move {
            crate::pin!(task);
            use futures::FutureExt;

            // TODO: This should use `tokio::select!`. But using this macro from
            // inside the tokio crate just produces wonderful error messages.
            futures::select! {
                _ = cancel_token.cancelled().fuse() => {
                    // The child task was cancelled
                    Err(CancellationError{})
                },
                res = task.fuse() => {
                    Ok(res)
                }
            }
        })
    }
}

/// Defines how a scope will behave if the `Future` it returns get dropped
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ScopeDropBehavior {
    /// When a scope is dropped while tasks are outstanding, the current thread
    /// will panic. Since this will not wait for child tasks to complete, the
    /// child tasks can outlive the parent in this case.
    Panic,
}

/// Advanced configuration options for `scope`
#[derive(Debug, Clone)]
struct ScopeConfig {
    drop_behavior: ScopeDropBehavior,
    cancellation_token: CancellationToken,
}

/// Allows to configure a new `Scope`
#[derive(Debug)]
struct ScopeConfigBuilder {
    /// The parent scope if available
    parent: Option<ScopeHandle>,
    /// Drop behavior overwrite
    drop_behavior: Option<ScopeDropBehavior>,
}

impl ScopeConfigBuilder {
    /// Creates a new scope which is treated as a child scope of the given
    /// [`ScopeHandle`]. The new scope will inherit all properties of the parent
    /// scope. In addition tasks inside the new scope will get cancelled when
    /// the parent scope gets cancelled.
    fn with_parent(parent: ScopeHandle) -> Self {
        Self {
            parent: Some(parent),
            drop_behavior: None,
        }
    }

    /// Creates a new scope which is detached from any parent scope.
    /// Tasks spawned on this `scope` will not get cancelled if any parent scope
    /// gets cancelled. Instead those tasks would only get cancelled if the
    /// scope itself gets cancelled.
    fn detached() -> Self {
        Self {
            parent: None,
            drop_behavior: Some(ScopeDropBehavior::Panic),
        }
    }

    /// Builds the configuration for the scope
    fn build(self) -> Result<ScopeConfig, ScopeConfigBuilderError> {
        // Get defaults

        // Generate a `CancellationToken`. If a parent scope and an associated
        // cancellation token exists, we create a child token from it.
        let cancellation_token = if let Some(parent) = &self.parent {
            parent.scope.config.cancellation_token.child_token()
        } else {
            CancellationToken::new()
        };

        let mut drop_behavior = match &self.parent {
            Some(parent) => parent.scope.config.drop_behavior,
            None => ScopeDropBehavior::Panic,
        };

        // Apply overwrites
        if let Some(behavior) = self.drop_behavior {
            drop_behavior = behavior
        };

        Ok(ScopeConfig {
            cancellation_token,
            drop_behavior,
        })
    }
}

#[derive(Debug)]
enum ScopeConfigBuilderError {}

/// A builder with allows to build and enter a new task scope.
#[derive(Debug)]
struct Scope {
    /// Configuration options for the scope
    config: ScopeConfig,
}

impl Scope {
    /// Creates a new scope which is treated as a child scope of the given
    /// [`ScopeHandle`]. The new scope will inherit all properties of the parent
    /// scope. In addition tasks inside the new scope will get cancelled when
    /// the parent scope gets cancelled.
    fn with_parent(parent: ScopeHandle) -> Self {
        Self::with_config(
            ScopeConfigBuilder::with_parent(parent)
                .build()
                .expect("Inherited config can not fail"),
        )
    }

    /// Creates a new scope which is detached from any parent scope.
    /// Tasks spawned on this `scope` will not get cancelled if any parent scope
    /// gets cancelled. Instead those tasks would only get cancelled if the
    /// scope itself gets cancelled.
    fn detached() -> Self {
        Self::with_config(
            ScopeConfigBuilder::detached()
                .build()
                .expect("Default config can not fail"),
        )
    }

    /// Creates a `Scope` with the given configuration
    fn with_config(config: ScopeConfig) -> Self {
        Self { config }
    }

    /// Creates a [`scope`] with custom options
    ///
    /// The method behaves like [`scope`], but the cancellation and `Drop` behavior
    /// for the [`scope`] are configurable. See [`ScopeConfig`] for details.
    async fn enter<Fut, R>(self, scope_fut: Fut) -> R
    where
        Fut: Future<Output = R> + Send,
    {
        let scope_state = ScopeState::new(self.config);
        let wait_fut = scope_state.wait_group.wait_future();

        // This guard will be called be executed if the scope gets dropped while
        // it is still executing.
        let mut wait_for_tasks_guard = WaitForTasksToJoinGuard {
            _wait_group: &scope_state.wait_group,
            enabled: true,
            drop_behavior: scope_state.config.drop_behavior,
        };

        let scoped_result = {
            // This guard will call `.cancel()` on the `CancellationToken` we
            // just created.
            let _cancel_guard = CancelTasksGuard {
                scope: &scope_state.config.cancellation_token,
            };

            let handle = ScopeHandle {
                scope: scope_state.clone(),
            };

            // Execute the scope handler, which gets passed a handle to the newly
            // created scope
            CURRENT_SCOPE.scope(handle, scope_fut).await
        };

        // Wait for all remaining tasks inside the scope to complete
        wait_fut.await;

        // The tasks have completed. We do not need to wait for them to complete
        // in the `Drop` guard.
        wait_for_tasks_guard.disarm();

        scoped_result
    }
}
