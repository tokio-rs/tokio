use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

doc_rt_core! {
    /// Yield execution back to the Tokio runtime.
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
