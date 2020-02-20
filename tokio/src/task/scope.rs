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
    task::{JoinError, JoinHandle}
};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

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

    fn is_cancelled(&self) -> bool {
        self.config.cancellation_token.is_cancelled()
    }
}

pin_project! {
    /// Allows to wait for a child task to join
    pub struct ScopedJoinHandle<'scope, T> {
        #[pin]
        handle: JoinHandle<T>,
        phantom: core::marker::PhantomData<&'scope ()>,
    }
}

impl<'scope, T> Future for ScopedJoinHandle<'scope, T> {
    // The actual type is Result<Result<T, CancellableFutureError>, JoinError>
    // However the cancellation will only happen at the exit of the scope. This
    // means in all cases the user still has a handle to the task, the task can
    // not be cancelled yet.
    type Output = Result<T, JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().handle.poll(cx)
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
    wait_group: &'a SharedWaitGroup,
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
            ScopeDropBehavior::BlockToCompletion => {
                let wait_fut = self.wait_group.wait_future();

                // TODOs:
                // - This should not have a futures dependency
                // - This might block multithreaded runtimes, since the tasks might need
                //   the current executor thread to make progress, due to dependening on
                //   its IO handles. We need to do something along task::block_in_place
                //   to solve this.
                futures::executor::block_on(wait_fut);
            }
            ScopeDropBehavior::Panic => {
                panic!("Scope was dropped before child tasks run to completion");
            }
            ScopeDropBehavior::Abort => {
                eprintln!("[ERROR] A scope was dropped without being awaited");
                std::process::abort();
            }
            ScopeDropBehavior::ContinueTasks => {
                // Do nothing
            }
        }
    }
}

/// A handle to the scope, which allows to spawn child tasks
#[derive(Clone)]
pub struct ScopeHandle {
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
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.scope.config.cancellation_token
    }

    /// Creates a child scope.
    /// The child scope inherit all properties of this scope
    pub fn child(&self) -> Scope {
        Scope::with_parent(self.clone())
    }

    /// Spawns a task on the scope
    pub fn spawn<'inner, T, R>(&'inner self, task: T) -> ScopedJoinHandle<'inner, R>
    where
        T: Future<Output = R> + Send + 'static,
        R: Send + 'static,
        T: 'inner,
    {
        let spawn_handle =
            crate::runtime::context::spawn_handle().expect("Spawn handle must be available");

        let releaser = self.scope.wait_group.add();

        let child_task = {
            spawn_handle.spawn(async move {
                // Drop this at the end of the task to signal we are done and unblock
                // the WaitGroup
                let _wait_group_releaser = releaser;

                // Execute the child task
                task.await
            })
        };

        // Since `Scope` is `Sync` and `Send`, cancellations can happen at any time
        // in case of invalid use. Therefore we only check cancellations once:
        // After the task has been spawned. Since the cancellation is already set,
        // we need to wait for the task to complete. Then we panic due to invalid
        // API usage.
        if self.scope.is_cancelled() {
            futures::executor::block_on(async {
                let _ = child_task.await;
            });
            panic!("Spawn on cancelled Scope");
        }

        ScopedJoinHandle {
            handle: child_task,
            phantom: core::marker::PhantomData,
        }
    }

    /// Spawns a task on the scope, which will get automatically cancelled if
    /// the `CancellationToken` which is associated with the current `ScopeHandle`
    /// gets cancelled.
    pub fn spawn_cancellable<'inner, T, R>(
        &'inner self,
        task: T,
    ) -> ScopedJoinHandle<'inner, Result<R, CancellationError>>
    where
        T: Future<Output = R> + Send + 'static,
        R: Send + 'static,
        T: 'inner,
    {
        let cancel_token = self.cancellation_token().clone();

        self.spawn(async move {
            crate::pin!(task);
            use futures::FutureExt;

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
    /// When a scope is dropped while tasks are outstanding, the process will be
    /// aborted.
    Abort,
    /// When a scope is dropped while tasks are outstanding, the current thread
    /// will be blocked until the tasks in the `scope` completed. This option
    /// is only available in multithreaded tokio runtimes, and is the default there.
    BlockToCompletion,
    /// Ignore that the scope got dropped and continue to run the child tasks.
    /// Choosing this option will break structured concurrency. It is therefore
    /// not recommended to pick the option.
    ContinueTasks,
}

/// Advanced configuration options for `scope`
#[derive(Debug, Clone)]
pub struct ScopeConfig {
    drop_behavior: ScopeDropBehavior,
    cancellation_token: CancellationToken,
}

/// Allows to configure a new `Scope`
#[derive(Debug)]
pub struct ScopeConfigBuilder {
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
    pub fn with_parent(parent: ScopeHandle) -> Self {
        Self {
            parent: Some(parent),
            drop_behavior: None,
        }
    }

    /// Creates a new scope which is detached from any parent scope.
    /// Tasks spawned on this `scope` will not get cancelled if any parent scope
    /// gets cancelled. Instead those tasks would only get cancelled if the
    /// scope itself gets cancelled.
    pub fn detached() -> Self {
        Self {
            parent: None,
            drop_behavior: Some(ScopeDropBehavior::Panic),
        }
    }

    /// If the scope is dropped instead of being awaited, the thread which
    /// performing the drop of the scope will block until all child tasks in the
    /// scope will have run to completion.
    pub fn block_to_completion(&mut self) {
        self.drop_behavior = Some(ScopeDropBehavior::BlockToCompletion);
    }

    /// If the scope is dropped instead of being awaited, child tasks will
    /// continue to run. This breaks "structured concurrency", since child tasks
    /// are now able to outlive the parent task.
    pub fn continue_tasks_on_drop(&mut self) {
        self.drop_behavior = Some(ScopeDropBehavior::ContinueTasks);
    }

    /// If a scope `Future` gets dropped instead of awaited, the current process
    /// will be aborted. This settings tries to provide higher guarantees about
    /// child tasks not outliving their parent tasks.
    pub fn abort_on_drop(&mut self) {
        self.drop_behavior = Some(ScopeDropBehavior::Abort);
    }
    
    /// If a scope `Future` gets dropped instead of awaited, the current
    /// thread will `panic!` in order to indicate that the scope did not
    /// correctly wait for child tasks to complete, and that there exist detached
    /// child tasks as a result of this action.
    pub fn panic_on_drop(&mut self) {
        self.drop_behavior = Some(ScopeDropBehavior::Panic);
    }

    /// Builds the configuration for the scope
    pub fn build(self) -> Result<ScopeConfig, ScopeConfigBuilderError> {
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
pub enum ScopeConfigBuilderError {
}

/// A builder with allows to build and enter a new task scope.
#[derive(Debug)]
pub struct Scope {
    /// Configuration options for the scope
    config: ScopeConfig,
}

impl Scope {
    /// Creates a new scope which is treated as a child scope of the given
    /// [`ScopeHandle`]. The new scope will inherit all properties of the parent
    /// scope. In addition tasks inside the new scope will get cancelled when
    /// the parent scope gets cancelled.
    pub fn with_parent(parent: ScopeHandle) -> Self {
        Self::with_config(ScopeConfigBuilder::with_parent(parent).build().expect("Inherited config can not fail"))
    }

    /// Creates a new scope which is detached from any parent scope.
    /// Tasks spawned on this `scope` will not get cancelled if any parent scope
    /// gets cancelled. Instead those tasks would only get cancelled if the
    /// scope itself gets cancelled.
    pub fn detached() -> Self {
        Self::with_config(ScopeConfigBuilder::detached().build().expect("Default config can not fail"))
    }

    /// Creates a `Scope` with the given configuration
    pub fn with_config(config: ScopeConfig) -> Self {
        Self {
            config
        }
    }

    /// Creates a [`scope`] with custom options
    ///
    /// The method behaves like [`scope`], but the cancellation and `Drop` behavior
    /// for the [`scope`] are configurable. See [`ScopeConfig`] for details.
    pub async fn enter<F, Fut, R>(self, scope_func: F) -> R
    where
        F: FnOnce(ScopeHandle) -> Fut,
        Fut: Future<Output = R> + Send,
    {
        let scope_state = ScopeState::new(self.config);
        let wait_fut = scope_state.wait_group.wait_future();

        // This guard will be called be executed if the scope gets dropped while
        // it is still executing.
        let mut wait_for_tasks_guard = WaitForTasksToJoinGuard {
            wait_group: &scope_state.wait_group,
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
            scope_func(handle).await
        };

        // Wait for all remaining tasks inside the scope to complete
        wait_fut.await;

        // The tasks have completed. We do not need to wait for them to complete
        // in the `Drop` guard.
        wait_for_tasks_guard.disarm();

        scoped_result
    }
}

/// Creates a task scope with default options.
///
/// The `scope` allows to spawn child tasks so that the lifetime of child tasks
/// is constrained within the scope.
///
/// A closure which accepts a [`ScopeHandle`] object and which returns a [`Future`]
/// needs to be passed to `scope`. The [`ScopeHandle`] can be used to spawn child
/// tasks.
///
/// If the provided `Future` had been polled to completion, all child tasks which
/// have been spawned via the `ScopeHandle` will be cancelled.
///
/// `scope` returns a [`Future`] which should be awaited. The `await` will only
/// complete once all child tasks that have been spawned via the provided
/// [`ScopeHandle`] have joined. Thereby the `scope` does not allow child tasks
/// to outlive their parent task, as long as the `scope` is awaited.
pub async fn scope<F, Fut, R>(scope_func: F) -> R
where
    F: FnOnce(ScopeHandle) -> Fut,
    Fut: Future<Output = R> + Send,
{
    Scope::detached().enter(scope_func).await
}

/// Error type which is returned when a task is cancelled
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct CancellationError {}
