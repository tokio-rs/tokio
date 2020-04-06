//! Opt-in yield points for improved cooperative scheduling.
//!
//! A single call to [`poll`] on a top-level task may potentially do a lot of work before it
//! returns `Poll::Pending`. If a task runs for a long period of time without yielding back to the
//! executor, it can starve other tasks waiting on that executor to execute them, or drive
//! underlying resources. Since Rust does not have a runtime, it is difficult to forcibly preempt a
//! long-running task. Instead, this module provides an opt-in mechanism for futures to collaborate
//! with the executor to avoid starvation.
//!
//! Consider a future like this one:
//!
//! ```
//! # use tokio::stream::{Stream, StreamExt};
//! async fn drop_all<I: Stream + Unpin>(mut input: I) {
//!     while let Some(_) = input.next().await {}
//! }
//! ```
//!
//! It may look harmless, but consider what happens under heavy load if the input stream is
//! _always_ ready. If we spawn `drop_all`, the task will never yield, and will starve other tasks
//! and resources on the same executor. With opt-in yield points, this problem is alleviated:
//!
//! ```ignore
//! # use tokio::stream::{Stream, StreamExt};
//! async fn drop_all<I: Stream + Unpin>(mut input: I) {
//!     while let Some(_) = input.next().await {
//!         tokio::coop::proceed().await;
//!     }
//! }
//! ```
//!
//! The `proceed` future will coordinate with the executor to make sure that every so often control
//! is yielded back to the executor so it can run other tasks.
//!
//! # Placing yield points
//!
//! Voluntary yield points should be placed _after_ at least some work has been done. If they are
//! not, a future sufficiently deep in the task hierarchy may end up _never_ getting to run because
//! of the number of yield points that inevitably appear before it is reached. In general, you will
//! want yield points to only appear in "leaf" futures -- those that do not themselves poll other
//! futures. By doing this, you avoid double-counting each iteration of the outer future against
//! the cooperating budget.
//!
//!   [`poll`]: https://doc.rust-lang.org/std/future/trait.Future.html#tymethod.poll

// NOTE: The doctests in this module are ignored since the whole module is (currently) private.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Constant used to determine how much "work" a task is allowed to do without yielding.
///
/// The value itself is chosen somewhat arbitrarily. It needs to be high enough to amortize wakeup
/// and scheduling costs, but low enough that we do not starve other tasks for too long. The value
/// also needs to be high enough that particularly deep tasks are able to do at least some useful
/// work at all.
///
/// Note that as more yield points are added in the ecosystem, this value will probably also have
/// to be raised.
const BUDGET: usize = 128;

/// Constant used to determine if budgeting has been disabled.
const UNCONSTRAINED: usize = usize::max_value();

thread_local! {
    static HITS: Cell<usize> = Cell::new(UNCONSTRAINED);
}

/// Run the given closure with a cooperative task budget.
///
/// Enabling budgeting when it is already enabled is a no-op.
#[inline(always)]
pub(crate) fn budget<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    HITS.with(move |hits| {
        if hits.get() != UNCONSTRAINED {
            // We are already being budgeted.
            //
            // Arguably this should be an error, but it can happen "correctly"
            // such as with block_on + LocalSet, so we make it a no-op.
            return f();
        }

        struct Guard<'a>(&'a Cell<usize>);
        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                self.0.set(UNCONSTRAINED);
            }
        }

        hits.set(BUDGET);
        let _guard = Guard(hits);
        f()
    })
}

cfg_rt_threaded! {
    #[inline(always)]
    pub(crate) fn has_budget_remaining() -> bool {
        HITS.with(|hits| hits.get() > 0)
    }
}

cfg_blocking_impl! {
    /// Forcibly remove the budgeting constraints early.
    pub(crate) fn stop() {
        HITS.with(|hits| {
            hits.set(UNCONSTRAINED);
        });
    }
}

/// Invoke `f` with a subset of the remaining budget.
///
/// This is useful if you have sub-futures that you need to poll, but that you want to restrict
/// from using up your entire budget. For example, imagine the following future:
///
/// ```rust
/// # use std::{future::Future, pin::Pin, task::{Context, Poll}};
/// use futures::stream::FuturesUnordered;
/// struct MyFuture<F1, F2> {
///     big: FuturesUnordered<F1>,
///     small: F2,
/// }
///
/// use tokio::stream::Stream;
/// impl<F1, F2> Future for MyFuture<F1, F2>
///   where F1: Future, F2: Future
/// # , F1: Unpin, F2: Unpin
/// {
///     type Output = F2::Output;
///
///     // fn poll(...)
/// # fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F2::Output> {
/// #   let this = &mut *self;
///     let mut big = // something to pin self.big
/// #                 Pin::new(&mut this.big);
///     let small = // something to pin self.small
/// #             Pin::new(&mut this.small);
///
///     // see if any of the big futures have finished
///     while let Some(e) = futures::ready!(big.as_mut().poll_next(cx)) {
///         // do something with e
/// #       let _ = e;
///     }
///
///     // see if the small future has finished
///     small.poll(cx)
/// }
/// # }
/// ```
///
/// It could be that every time `poll` gets called, `big` ends up spending the entire budget, and
/// `small` never gets polled. That would be sad. If you want to stick up for the little future,
/// that's what `limit` is for. It lets you portion out a smaller part of the yield budget to a
/// particular segment of your code. In the code above, you would write
///
/// ```rust,ignore
/// # use std::{future::Future, pin::Pin, task::{Context, Poll}};
/// # use futures::stream::FuturesUnordered;
/// # struct MyFuture<F1, F2> {
/// #     big: FuturesUnordered<F1>,
/// #     small: F2,
/// # }
/// #
/// # use tokio::stream::Stream;
/// # impl<F1, F2> Future for MyFuture<F1, F2>
/// #   where F1: Future, F2: Future
/// # , F1: Unpin, F2: Unpin
/// # {
/// # type Output = F2::Output;
/// # fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F2::Output> {
/// #   let this = &mut *self;
/// #   let mut big = Pin::new(&mut this.big);
/// #   let small = Pin::new(&mut this.small);
/// #
///     // see if any of the big futures have finished
///     while let Some(e) = futures::ready!(tokio::coop::limit(64, || big.as_mut().poll_next(cx))) {
/// #       // do something with e
/// #       let _ = e;
/// #   }
/// #   small.poll(cx)
/// # }
/// # }
/// ```
///
/// Now, even if `big` spends its entire budget, `small` will likely be left with some budget left
/// to also do useful work. In particular, if the remaining budget was `N` at the start of `poll`,
/// `small` will have at least a budget of `N - 64`. It may be more if `big` did not spend its
/// entire budget.
///
/// Note that you cannot _increase_ your budget by calling `limit`. The budget provided to the code
/// inside the buget is the _minimum_ of the _current_ budget and the bound.
///
#[allow(unreachable_pub, dead_code)]
pub fn limit<R>(bound: usize, f: impl FnOnce() -> R) -> R {
    HITS.with(|hits| {
        let budget = hits.get();
        // with_bound cannot _increase_ the remaining budget
        let bound = std::cmp::min(budget, bound);
        // When f() exits, how much should we add to what is left?
        let floor = budget.saturating_sub(bound);
        // Make sure we restore the remaining budget even on panic
        struct RestoreBudget<'a>(&'a Cell<usize>, usize);
        impl<'a> Drop for RestoreBudget<'a> {
            fn drop(&mut self) {
                let left = self.0.get();
                self.0.set(self.1 + left);
            }
        }
        // Time to restrict!
        hits.set(bound);
        let _restore = RestoreBudget(&hits, floor);
        f()
    })
}

/// Returns `Poll::Pending` if the current task has exceeded its budget and should yield.
#[allow(unreachable_pub, dead_code)]
#[inline]
pub fn poll_proceed(cx: &mut Context<'_>) -> Poll<()> {
    HITS.with(|hits| {
        let n = hits.get();
        if n == UNCONSTRAINED {
            // opted out of budgeting
            Poll::Ready(())
        } else if n == 0 {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            hits.set(n.saturating_sub(1));
            Poll::Ready(())
        }
    })
}

/// Resolves immediately unless the current task has already exceeded its budget.
///
/// This should be placed after at least some work has been done. Otherwise a future sufficiently
/// deep in the task hierarchy may end up never getting to run because of the number of yield
/// points that inevitably appear before it is even reached. For example:
///
/// ```ignore
/// # use tokio::stream::{Stream, StreamExt};
/// async fn drop_all<I: Stream + Unpin>(mut input: I) {
///     while let Some(_) = input.next().await {
///         tokio::coop::proceed().await;
///     }
/// }
/// ```
#[allow(unreachable_pub, dead_code)]
#[inline]
pub async fn proceed() {
    use crate::future::poll_fn;
    poll_fn(|cx| poll_proceed(cx)).await;
}

pin_project_lite::pin_project! {
    /// A future that cooperatively yields to the task scheduler when polling,
    /// if the task's budget is exhausted.
    ///
    /// Internally, this is simply a future combinator which calls
    /// [`poll_proceed`] in its `poll` implementation before polling the wrapped
    /// future.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::coop::CoopFutureExt;
    ///
    /// async {  /* ... */ }
    ///     .cooperate()
    ///     .await;
    /// # }
    /// ```
    ///
    /// [`poll_proceed`]: fn@poll_proceed
    #[derive(Debug)]
    #[allow(unreachable_pub, dead_code)]
    pub struct CoopFuture<F> {
        #[pin]
        future: F,
    }
}

impl<F: Future> Future for CoopFuture<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        ready!(poll_proceed(cx));
        self.project().future.poll(cx)
    }
}

impl<F: Future> CoopFuture<F> {
    /// Returns a new `CoopFuture` wrapping the given future.
    ///
    #[allow(unreachable_pub, dead_code)]
    pub fn new(future: F) -> Self {
        Self { future }
    }
}

// Currently only used by `tokio::sync`; and if we make this combinator public,
// it should probably be on the `FutureExt` trait instead.
cfg_sync! {
    /// Extension trait providing `Future::cooperate` extension method.
    ///
    /// Note: if/when the co-op API becomes public, this method should probably be
    /// provided by `FutureExt`, instead.
    pub(crate) trait CoopFutureExt: Future {
        /// Wrap `self` to cooperatively yield to the scheduler when polling, if the
        /// task's budget is exhausted.
        fn cooperate(self) -> CoopFuture<Self>
        where
            Self: Sized,
        {
            CoopFuture::new(self)
        }
    }

    impl<F> CoopFutureExt for F where F: Future {}
}

#[cfg(all(test, not(loom)))]
mod test {
    use super::*;

    fn get() -> usize {
        HITS.with(|hits| hits.get())
    }

    #[test]
    fn bugeting() {
        use tokio_test::*;

        assert_eq!(get(), UNCONSTRAINED);
        assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
        assert_eq!(get(), UNCONSTRAINED);
        budget(|| {
            assert_eq!(get(), BUDGET);
            assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
            assert_eq!(get(), BUDGET - 1);
            assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
            assert_eq!(get(), BUDGET - 2);
        });
        assert_eq!(get(), UNCONSTRAINED);

        budget(|| {
            limit(3, || {
                assert_eq!(get(), 3);
                assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
                assert_eq!(get(), 2);
                limit(4, || {
                    assert_eq!(get(), 2);
                    assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
                    assert_eq!(get(), 1);
                });
                assert_eq!(get(), 1);
                assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
                assert_eq!(get(), 0);
                assert_pending!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
                assert_eq!(get(), 0);
                assert_pending!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
                assert_eq!(get(), 0);
            });
            assert_eq!(get(), BUDGET - 3);
            assert_ready!(task::spawn(()).enter(|cx, _| poll_proceed(cx)));
            assert_eq!(get(), BUDGET - 4);
            assert_ready!(task::spawn(proceed()).poll());
            assert_eq!(get(), BUDGET - 5);
        });
    }
}
