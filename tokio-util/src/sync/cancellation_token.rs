//! An asynchronously awaitable `CancellationToken`.
//! The token allows to signal a cancellation request to one or more tasks.
pub(crate) mod guard;
mod tree_node;

use crate::loom::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use guard::DropGuard;
use pin_project_lite::pin_project;

/// A token which can be used to signal a cancellation request to one or more
/// tasks.
///
/// Tasks can call [`CancellationToken::cancelled()`] in order to
/// obtain a Future which will be resolved when cancellation is requested.
///
/// Cancellation can be requested through the [`CancellationToken::cancel`] method.
///
/// # Examples
///
/// ```no_run
/// use tokio::select;
/// use tokio_util::sync::CancellationToken;
///
/// #[tokio::main]
/// async fn main() {
///     let token = CancellationToken::new();
///     let cloned_token = token.clone();
///
///     let join_handle = tokio::spawn(async move {
///         // Wait for either cancellation or a very long time
///         select! {
///             _ = cloned_token.cancelled() => {
///                 // The token was cancelled
///                 5
///             }
///             _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
///                 99
///             }
///         }
///     });
///
///     tokio::spawn(async move {
///         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
///         token.cancel();
///     });
///
///     assert_eq!(5, join_handle.await.unwrap());
/// }
/// ```
pub struct CancellationToken {
    inner: Arc<tree_node::TreeNode>,
}

impl std::panic::UnwindSafe for CancellationToken {}
impl std::panic::RefUnwindSafe for CancellationToken {}

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationToken`]
    /// is cancelled.
    #[must_use = "futures do nothing unless polled"]
    pub struct WaitForCancellationFuture<'a> {
        cancellation_token: &'a CancellationToken,
        #[pin]
        future: tokio::sync::futures::Notified<'a>,
    }
}

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationToken`]
    /// is cancelled.
    ///
    /// This is the counterpart to [`WaitForCancellationFuture`] that takes
    /// [`CancellationToken`] by value instead of using a reference.
    #[must_use = "futures do nothing unless polled"]
    pub struct WaitForCancellationFutureOwned {
        // Since `future` is the first field, it is dropped before the
        // cancellation_token field. This ensures that the reference inside the
        // `Notified` remains valid.
        #[pin]
        future: tokio::sync::futures::Notified<'static>,
        cancellation_token: CancellationToken,
    }
}

// ===== impl CancellationToken =====

impl core::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

impl Clone for CancellationToken {
    /// Creates a clone of the `CancellationToken` which will get cancelled
    /// whenever the current token gets cancelled, and vice versa.
    fn clone(&self) -> Self {
        tree_node::increase_handle_refcount(&self.inner);
        CancellationToken {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for CancellationToken {
    fn drop(&mut self) {
        tree_node::decrease_handle_refcount(&self.inner);
    }
}

impl Default for CancellationToken {
    fn default() -> CancellationToken {
        CancellationToken::new()
    }
}

impl CancellationToken {
    /// Creates a new CancellationToken in the non-cancelled state.
    pub fn new() -> CancellationToken {
        CancellationToken {
            inner: Arc::new(tree_node::TreeNode::new()),
        }
    }

    /// Creates a `CancellationToken` which will get cancelled whenever the
    /// current token gets cancelled. Unlike a cloned `CancellationToken`,
    /// cancelling a child token does not cancel the parent token.
    ///
    /// If the current token is already cancelled, the child token will get
    /// returned in cancelled state.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::select;
    /// use tokio_util::sync::CancellationToken;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let token = CancellationToken::new();
    ///     let child_token = token.child_token();
    ///
    ///     let join_handle = tokio::spawn(async move {
    ///         // Wait for either cancellation or a very long time
    ///         select! {
    ///             _ = child_token.cancelled() => {
    ///                 // The token was cancelled
    ///                 5
    ///             }
    ///             _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
    ///                 99
    ///             }
    ///         }
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    ///         token.cancel();
    ///     });
    ///
    ///     assert_eq!(5, join_handle.await.unwrap());
    /// }
    /// ```
    pub fn child_token(&self) -> CancellationToken {
        CancellationToken {
            inner: tree_node::child_node(&self.inner),
        }
    }

    /// Cancel the [`CancellationToken`] and all child tokens which had been
    /// derived from it.
    ///
    /// This will wake up all tasks which are waiting for cancellation.
    ///
    /// Be aware that cancellation is not an atomic operation. It is possible
    /// for another thread running in parallel with a call to `cancel` to first
    /// receive `true` from `is_cancelled` on one child node, and then receive
    /// `false` from `is_cancelled` on another child node. However, once the
    /// call to `cancel` returns, all child nodes have been fully cancelled.
    pub fn cancel(&self) {
        tree_node::cancel(&self.inner);
    }

    /// Returns `true` if the `CancellationToken` is cancelled.
    pub fn is_cancelled(&self) -> bool {
        tree_node::is_cancelled(&self.inner)
    }

    /// Returns a `Future` that gets fulfilled when cancellation is requested.
    ///
    /// The future will complete immediately if the token is already cancelled
    /// when this method is called.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe.
    pub fn cancelled(&self) -> WaitForCancellationFuture<'_> {
        WaitForCancellationFuture {
            cancellation_token: self,
            future: self.inner.notified(),
        }
    }

    /// Returns a `Future` that gets fulfilled when cancellation is requested.
    ///
    /// The future will complete immediately if the token is already cancelled
    /// when this method is called.
    ///
    /// The function takes self by value and returns a future that owns the
    /// token.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe.
    pub fn cancelled_owned(self) -> WaitForCancellationFutureOwned {
        WaitForCancellationFutureOwned::new(self)
    }

    /// Creates a `DropGuard` for this token.
    ///
    /// Returned guard will cancel this token (and all its children) on drop
    /// unless disarmed.
    pub fn drop_guard(self) -> DropGuard {
        DropGuard { inner: Some(self) }
    }
}

// ===== impl WaitForCancellationFuture =====

impl<'a> core::fmt::Debug for WaitForCancellationFuture<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitForCancellationFuture").finish()
    }
}

impl<'a> Future for WaitForCancellationFuture<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut this = self.project();
        loop {
            if this.cancellation_token.is_cancelled() {
                return Poll::Ready(());
            }

            // No wakeups can be lost here because there is always a call to
            // `is_cancelled` between the creation of the future and the call to
            // `poll`, and the code that sets the cancelled flag does so before
            // waking the `Notified`.
            if this.future.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }

            this.future.set(this.cancellation_token.inner.notified());
        }
    }
}

// ===== impl WaitForCancellationFutureOwned =====

impl core::fmt::Debug for WaitForCancellationFutureOwned {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitForCancellationFutureOwned").finish()
    }
}

impl WaitForCancellationFutureOwned {
    fn new(cancellation_token: CancellationToken) -> Self {
        WaitForCancellationFutureOwned {
            // cancellation_token holds a heap allocation and is guaranteed to have a
            // stable deref, thus it would be ok to move the cancellation_token while
            // the future holds a reference to it.
            //
            // # Safety
            //
            // cancellation_token is dropped after future due to the field ordering.
            future: unsafe { Self::new_future(&cancellation_token) },
            cancellation_token,
        }
    }

    /// # Safety
    /// The returned future must be destroyed before the cancellation token is
    /// destroyed.
    unsafe fn new_future(
        cancellation_token: &CancellationToken,
    ) -> tokio::sync::futures::Notified<'static> {
        let inner_ptr = Arc::as_ptr(&cancellation_token.inner);
        // SAFETY: The `Arc::as_ptr` method guarantees that `inner_ptr` remains
        // valid until the strong count of the Arc drops to zero, and the caller
        // guarantees that they will drop the future before that happens.
        (*inner_ptr).notified()
    }
}

impl Future for WaitForCancellationFutureOwned {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut this = self.project();

        loop {
            if this.cancellation_token.is_cancelled() {
                return Poll::Ready(());
            }

            // No wakeups can be lost here because there is always a call to
            // `is_cancelled` between the creation of the future and the call to
            // `poll`, and the code that sets the cancelled flag does so before
            // waking the `Notified`.
            if this.future.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }

            // # Safety
            //
            // cancellation_token is dropped after future due to the field ordering.
            this.future
                .set(unsafe { Self::new_future(this.cancellation_token) });
        }
    }
}
