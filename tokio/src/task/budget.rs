use std::task::{ready, Context, Poll};

/// Consumes a unit of budget and returns the execution back to the Tokio
/// runtime *if* the task's coop budget was exhausted.
///
/// The task will only yield if its entire coop budget has been exhausted.
/// This function can be used in order to insert optional yield points into long
/// computations that do not use Tokio resources like sockets or semaphores,
/// without redundantly yielding to the runtime each time.
///
/// # Examples
///
/// Make sure that a function which returns a sum of (potentially lots of)
/// iterated values is cooperative.
///
/// ```
/// async fn sum_iterator(input: &mut impl std::iter::Iterator<Item=i64>) -> i64 {
///     let mut sum: i64 = 0;
///     while let Some(i) = input.next() {
///         sum += i;
///         tokio::task::consume_budget().await
///     }
///     sum
/// }
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
pub async fn consume_budget() {
    let mut status = Poll::Pending;

    std::future::poll_fn(move |cx| {
        ready!(crate::trace::trace_leaf(cx));
        if status.is_ready() {
            return status;
        }
        status = crate::runtime::coop::poll_proceed(cx).map(|restore| {
            restore.made_progress();
        });
        status
    })
    .await
}

/// Polls to see if any budget is available or not.
///
/// See also the usage example in the [task module](index.html#poll_budget_available).
///
/// This method returns:
/// - `Poll::Pending` if the budget is depleted
/// - `Poll::Ready(())` if there is still budget left
#[cfg_attr(docsrs, doc(cfg(feature = "rt")))]
pub fn poll_budget_available(cx: &mut Context<'_>) -> Poll<()> {
    ready!(crate::trace::trace_leaf(cx));
    if crate::runtime::coop::has_budget_remaining() {
        Poll::Ready(())
    } else {
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
