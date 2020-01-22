use super::{
    cancellation_token::CancellationToken,
    wait_group::{WaitGroup, WaitGroupFuture},
};
use crate::task::{JoinError, JoinHandle};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

#[derive(Clone)]
struct ScopeState {
    cancel_token: Arc<CancellationToken>,
    wait_group: SharedWaitGroup,
    options: ScopeOptions,
}

impl ScopeState {
    fn new(options: ScopeOptions) -> Self {
        Self {
            cancel_token: Arc::new(CancellationToken::new(false)),
            wait_group: SharedWaitGroup::new(),
            options,
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

pin_project! {
    /// Allows to wait for a child task to join
    pub struct ScopedJoinHandle<'scope, T> {
        #[pin]
        handle: JoinHandle<Result<T, CancellableFutureError>>,
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
        self.project()
            .handle
            .poll(cx)
            .map(|poll_res| poll_res.map(|poll_ok| poll_ok.expect("Task can not be cancelled")))
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

impl ScopeHandle {
    /// spawns a task on the scope
    pub fn spawn<'inner, T, R>(&'inner self, task: T) -> ScopedJoinHandle<'inner, R>
    where
        T: Future<Output = R> + Send + 'static,
        R: Send + 'static,
        T: 'inner,
    {
        let spawn_handle =
            crate::runtime::context::spawn_handle().expect("Spawn handle must be available");

        let releaser = self.scope.wait_group.add();
        let cancel_token = self.scope.cancel_token.clone();
        let cancel_behavior = self.scope.options.cancel_behavior;

        let child_task = {
            spawn_handle.spawn(async move {
                // Drop this at the end of the task to signal we are done and unblock
                // the WaitGroup
                let _wait_group_releaser = releaser;

                if cancel_behavior == ScopeCancelBehavior::CancelChildTasks {
                    futures::pin_mut!(task);
                    use futures::FutureExt;

                    futures::select! {
                        _ = cancel_token.wait().fuse() => {
                            // The child task was cancelled
                            return Err(CancellableFutureError::Cancelled);
                        },
                        res = task.fuse() => {
                            return Ok(res);
                        }
                    }
                } else {
                    Ok(task.await)
                }
            })
        };

        // Since `Scope` is `Sync` and `Send` cancellations can happen at any time
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
}

/// Defines how a scope will behave if the `Future` it returns get dropped
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScopeDropBehavior {
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

/// Defines how a scope should cancel its child task once the scope is exited
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ScopeCancelBehavior {
    /// Once the scope is exited, all still running child tasks will get cancelled.
    /// The cancellation is asynchronous: Tasks will only notice the
    /// cancellation the next time they are scheduled.
    /// This option is the default option.
    CancelChildTasks,
    /// Child tasks are allowed to continue to run.
    /// This option should only be chosen if it is either guaranteed that child
    /// tasks will join on their own, or if the application uses an additional
    /// mechanism (like a `CancellationToken`) to signal child tasks to return.
    ContinueChildTasks,
}

/// Advanced configuration options for `scope`
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ScopeOptions {
    /// Whether tasks should be cancelled once the scope is exited
    pub cancel_behavior: ScopeCancelBehavior,
    /// How the scope should behave if it gets dropped instead of being `await`ed
    pub drop_behavior: ScopeDropBehavior,
}

impl Default for ScopeOptions {
    fn default() -> Self {
        // TODO: We need to identify a mechanism whether this runs in a singlethreaded
        // runtime. In that case we should use the panic strategy, since blocking
        // is not viable in a singlethreaded context.
        const IS_SINGLE_THREADED: bool = false;

        let drop_behavior = if IS_SINGLE_THREADED {
            ScopeDropBehavior::Panic
        } else {
            ScopeDropBehavior::BlockToCompletion
        };

        Self {
            cancel_behavior: ScopeCancelBehavior::CancelChildTasks,
            drop_behavior,
        }
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
/// to escape their parent task, as long as the `scope` is awaited.
pub async fn scope<F, Fut, R>(scope_func: F) -> R
where
    F: FnOnce(ScopeHandle) -> Fut,
    Fut: Future<Output = R> + Send,
{
    let options = ScopeOptions::default();
    scope_with_options(options, scope_func).await
}

/// Creates a [`scope`] with custom options
///
/// The method behaves like [`scope`], but the cancellation and `Drop` behavior
/// for the [`scope`] are configurable. See [`ScopeOptions`] for details.
pub async fn scope_with_options<F, Fut, R>(options: ScopeOptions, scope_func: F) -> R
where
    F: FnOnce(ScopeHandle) -> Fut,
    Fut: Future<Output = R> + Send,
{
    let scope_state = ScopeState::new(options);
    let wait_fut = scope_state.wait_group.wait_future();

    let mut wait_for_tasks_guard = WaitForTasksToJoinGuard {
        wait_group: &scope_state.wait_group,
        enabled: true,
        drop_behavior: options.drop_behavior,
    };

    let scoped_result = {
        let _cancel_guard = CancelTasksGuard {
            scope: &scope_state.cancel_token,
        };
        if options.cancel_behavior == ScopeCancelBehavior::ContinueChildTasks {
            std::mem::forget(_cancel_guard);
        }

        let handle = ScopeHandle {
            scope: scope_state.clone(),
        };
        let scoped_result = scope_func(handle).await;

        scoped_result
    };

    wait_fut.await;

    // The tasks have completed. We do not need to wait for them to complete
    // in the `Drop` guard.
    wait_for_tasks_guard.disarm();

    scoped_result
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CancellableFutureError {
    Cancelled,
}

#[derive(Clone)]
struct SharedWaitGroup {
    inner: Arc<WaitGroup>,
}

impl SharedWaitGroup {
    fn new() -> Self {
        Self {
            inner: Arc::new(WaitGroup::new()),
        }
    }

    #[must_use]
    fn add(&self) -> WaitGroupReleaser {
        self.inner.add();
        WaitGroupReleaser {
            inner: self.inner.clone(),
        }
    }

    fn wait_future<'a>(&'a self) -> WaitGroupFuture<'a> {
        self.inner.wait()
    }
}

struct WaitGroupReleaser {
    inner: Arc<WaitGroup>,
}

impl Drop for WaitGroupReleaser {
    fn drop(&mut self) {
        self.inner.remove();
    }
}
