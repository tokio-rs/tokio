use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

doc_rt_core! {
    /// Yields execution back to the Tokio runtime.
    ///
    /// A task yields by awaiting on `yield_now()`, and may resume when that
    /// future completes (with no output.) The current task will be re-added as
    /// a pending task at the _back_ of the pending queue. Any other pending
    /// tasks will be scheduled. No other waking is required for the task to
    /// continue.
    ///
    /// See also the usage example in the [task module](index.html#yield_now).
    #[must_use = "yield_now does nothing unless polled/`await`-ed"]
    pub async fn yield_now() {
        /// Yield implementation
        struct YieldNow {
            yielded: bool,
        }

        impl Future for YieldNow {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                if self.yielded {
                    return Poll::Ready(());
                }

                self.yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }

        YieldNow { yielded: false }.await
    }
}
