//! An asynchronously awaitable `CancellationTokenWithReason`.
//! The token allows to signal a cancellation request to one or more tasks.
pub(crate) mod guard;

use crate::loom::sync::Arc;
use crate::util::MaybeDangling;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use guard::DropGuardWithReason;
use pin_project_lite::pin_project;

use super::cancellation_token::tree_node;

/// A token which can be used to signal a cancellation request to one or more
/// tasks.
///
/// Tasks can call [`CancellationTokenWithReason::cancelled()`] in order to
/// obtain a Future which will be resolved when cancellation is requested.
///
/// Cancellation can be requested through the [`CancellationTokenWithReason::cancel`] method.
///
/// # Examples
///
/// ```no_run
/// use tokio::select;
/// use tokio_util::sync::CancellationTokenWithReason;
///
/// #[tokio::main]
/// async fn main() {
///     let token = CancellationTokenWithReason::new();
///     let cloned_token = token.clone();
///
///     let join_handle = tokio::spawn(async move {
///         // Wait for either cancellation or a very long time
///         select! {
///             cancel_value = cloned_token.cancelled() => {
///                 // The token was cancelled
///                 cancel_value
///             }
///             _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
///                 99
///             }
///         }
///     });
///
///     tokio::spawn(async move {
///         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
///         token.cancel(5);
///     });
///
///     assert_eq!(5, join_handle.await.unwrap());
/// }
/// ```
pub struct CancellationTokenWithReason<T> {
    inner: Arc<tree_node::TreeNode<T>>,
}

impl<T> std::panic::UnwindSafe for CancellationTokenWithReason<T> {}
impl<T> std::panic::RefUnwindSafe for CancellationTokenWithReason<T> {}

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationTokenWithReason`]
    /// is cancelled.
    #[must_use = "futures do nothing unless polled"]
    pub struct WaitForCancellationWithReasonFuture<'a, T> {
        cancellation_token: &'a CancellationTokenWithReason<T>,
        #[pin]
        future: tokio::sync::futures::Notified<'a>,
    }
}

pin_project! {
    /// A Future that is resolved once the corresponding [`CancellationTokenWithReason`]
    /// is cancelled.
    ///
    /// This is the counterpart to [`WaitForCancellationFuture`] that takes
    /// [`CancellationTokenWithReason`] by value instead of using a reference.
    #[must_use = "futures do nothing unless polled"]
    pub struct WaitForCancellationWithReasonFutureOwned<T> {
        // This field internally has a reference to the cancellation token, but camouflages
        // the relationship with `'static`. To avoid Undefined Behavior, we must ensure
        // that the reference is only used while the cancellation token is still alive. To
        // do that, we ensure that the future is the first field, so that it is dropped
        // before the cancellation token.
        //
        // We use `MaybeDanglingFuture` here because without it, the compiler could assert
        // the reference inside `future` to be valid even after the destructor of that
        // field runs. (Specifically, when the `WaitForCancellationFutureOwned` is passed
        // as an argument to a function, the reference can be asserted to be valid for the
        // rest of that function.) To avoid that, we use `MaybeDangling` which tells the
        // compiler that the reference stored inside it might not be valid.
        //
        // See <https://users.rust-lang.org/t/unsafe-code-review-semi-owning-weak-rwlock-t-guard/95706>
        // for more info.
        #[pin]
        future: MaybeDangling<tokio::sync::futures::Notified<'static>>,
        cancellation_token: CancellationTokenWithReason<T>,
    }
}

// ===== impl CancellationTokenWithReason =====

impl<T: core::fmt::Debug> core::fmt::Debug for CancellationTokenWithReason<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        tree_node::with_cancelled(&self.inner, |cancel| {
            f.debug_struct("CancellationTokenWithReason")
                .field("cancelled", cancel)
                .finish()
        })
    }
}

impl<T> Clone for CancellationTokenWithReason<T> {
    /// Creates a clone of the `CancellationTokenWithReason` which will get cancelled
    /// whenever the current token gets cancelled, and vice versa.
    fn clone(&self) -> Self {
        tree_node::increase_handle_refcount(&self.inner);
        CancellationTokenWithReason {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for CancellationTokenWithReason<T> {
    fn drop(&mut self) {
        tree_node::decrease_handle_refcount(&self.inner);
    }
}

impl<T> Default for CancellationTokenWithReason<T> {
    fn default() -> CancellationTokenWithReason<T> {
        CancellationTokenWithReason::new()
    }
}

impl<T> CancellationTokenWithReason<T> {
    /// Creates a new `CancellationTokenWithReason` in the non-cancelled state.
    pub fn new() -> CancellationTokenWithReason<T> {
        CancellationTokenWithReason {
            inner: Arc::new(tree_node::TreeNode::new()),
        }
    }

    /// Returns `true` if the `CancellationTokenWithReason` is cancelled.
    pub fn is_cancelled(&self) -> bool {
        tree_node::is_cancelled(&self.inner)
    }
}

impl<T: Clone> CancellationTokenWithReason<T> {
    /// Creates a `CancellationTokenWithReason` which will get cancelled whenever the
    /// current token gets cancelled. Unlike a cloned `CancellationTokenWithReason`,
    /// cancelling a child token does not cancel the parent token.
    ///
    /// If the current token is already cancelled, the child token will get
    /// returned in cancelled state.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::select;
    /// use tokio_util::sync::CancellationTokenWithReason;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let token = CancellationTokenWithReason::new();
    ///     let child_token = token.child_token();
    ///
    ///     let join_handle = tokio::spawn(async move {
    ///         // Wait for either cancellation or a very long time
    ///         select! {
    ///             cancel_value = child_token.cancelled() => {
    ///                 // The token was cancelled
    ///                 cancel_value
    ///             }
    ///             _ = tokio::time::sleep(std::time::Duration::from_secs(9999)) => {
    ///                 99
    ///             }
    ///         }
    ///     });
    ///
    ///     tokio::spawn(async move {
    ///         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    ///         token.cancel(5);
    ///     });
    ///
    ///     assert_eq!(5, join_handle.await.unwrap());
    /// }
    /// ```
    pub fn child_token(&self) -> CancellationTokenWithReason<T> {
        CancellationTokenWithReason {
            inner: tree_node::child_node(&self.inner),
        }
    }

    /// Cancel the [`CancellationTokenWithReason`] and all child tokens which had been
    /// derived from it.
    ///
    /// This will wake up all tasks which are waiting for cancellation.
    ///
    /// Be aware that cancellation is not an atomic operation. It is possible
    /// for another thread running in parallel with a call to `cancel` to first
    /// receive `true` from `is_cancelled` on one child node, and then receive
    /// `false` from `is_cancelled` on another child node. However, once the
    /// call to `cancel` returns, all child nodes have been fully cancelled.
    pub fn cancel(&self, reason: T) {
        tree_node::cancel(&self.inner, reason);
    }

    /// Returns `Some(reason)` if the `CancellationTokenWithReason` is cancelled.
    pub fn get_cancelled(&self) -> Option<T> {
        tree_node::with_cancelled(&self.inner, Clone::clone)
    }

    /// Returns a `Future` that gets fulfilled when cancellation is requested.
    ///
    /// The future will complete immediately if the token is already cancelled
    /// when this method is called.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe.
    pub fn cancelled(&self) -> WaitForCancellationWithReasonFuture<'_, T> {
        WaitForCancellationWithReasonFuture {
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
    pub fn cancelled_owned(self) -> WaitForCancellationWithReasonFutureOwned<T>
    where
        T: 'static,
    {
        WaitForCancellationWithReasonFutureOwned::new(self)
    }

    /// Creates a `DropGuard` for this token.
    ///
    /// Returned guard will cancel this token (and all its children) on drop
    /// unless disarmed.
    pub fn drop_guard(self, reason: T) -> DropGuardWithReason<T> {
        DropGuardWithReason {
            inner: Some((self, reason)),
        }
    }
}

// ===== impl WaitForCancellationFuture =====

impl<'a, T> core::fmt::Debug for WaitForCancellationWithReasonFuture<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitForCancellationWithReasonFuture")
            .finish()
    }
}

impl<'a, T: Clone> Future for WaitForCancellationWithReasonFuture<'a, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut this = self.project();
        loop {
            if let Some(cancel) = this.cancellation_token.get_cancelled() {
                return Poll::Ready(cancel);
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

impl<T> core::fmt::Debug for WaitForCancellationWithReasonFutureOwned<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WaitForCancellationWithReasonFutureOwned<T>")
            .finish()
    }
}

impl<T: 'static> WaitForCancellationWithReasonFutureOwned<T> {
    fn new(cancellation_token: CancellationTokenWithReason<T>) -> Self {
        WaitForCancellationWithReasonFutureOwned {
            // cancellation_token holds a heap allocation and is guaranteed to have a
            // stable deref, thus it would be ok to move the cancellation_token while
            // the future holds a reference to it.
            //
            // # Safety
            //
            // cancellation_token is dropped after future due to the field ordering.
            future: MaybeDangling::new(unsafe { Self::new_future(&cancellation_token) }),
            cancellation_token,
        }
    }

    /// # Safety
    /// The returned future must be destroyed before the cancellation token is
    /// destroyed.
    unsafe fn new_future(
        cancellation_token: &CancellationTokenWithReason<T>,
    ) -> tokio::sync::futures::Notified<'static> {
        let inner_ptr = Arc::as_ptr(&cancellation_token.inner);
        // SAFETY: The `Arc::as_ptr` method guarantees that `inner_ptr` remains
        // valid until the strong count of the Arc drops to zero, and the caller
        // guarantees that they will drop the future before that happens.
        (*inner_ptr).notified()
    }
}

impl<T: 'static + Clone> Future for WaitForCancellationWithReasonFutureOwned<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut this = self.project();

        loop {
            if let Some(cancel) = this.cancellation_token.get_cancelled() {
                return Poll::Ready(cancel);
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
            this.future.set(MaybeDangling::new(unsafe {
                Self::new_future(this.cancellation_token)
            }));
        }
    }
}
