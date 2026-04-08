use crate::loom::sync::Arc;
use crate::runtime::context;
use crate::runtime::scheduler::{self, current_thread, Inject};
use crate::task::Id;

use backtrace::BacktraceFrame;
use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::ptr::NonNull;
use std::task::{self, Poll};

mod symbol;
mod trace_impl;
mod tree;

use symbol::Symbol;
use tree::Tree;

use super::{Notified, OwnedTasks, Schedule};

type Backtrace = Vec<BacktraceFrame>;
type SymbolTrace = Vec<Symbol>;

/// The ambient backtracing context.
pub(crate) struct Context {
    /// The address of [`Trace::root`] establishes an upper unwinding bound on
    /// the backtraces in `Trace`.
    active_frame: Cell<Option<NonNull<Frame>>>,

    /// The function that is invoked at each leaf future inside of Tokio
    ///
    /// For example, within tokio::time:sleep, sockets. etc.
    trace_leaf_fn: Cell<Option<fn(&TraceMeta)>>,
}

/// A [`Frame`] in an intrusive, doubly-linked tree of [`Frame`]s.
struct Frame {
    /// The location associated with this frame.
    inner_addr: *const c_void,

    /// The parent frame, if any.
    ///
    /// Tracking parent allows nested `Root` futures to correctly manage their boundaries
    parent: Option<NonNull<Frame>>,
}

/// An tree execution trace.
///
/// Traces are captured with [`Trace::capture`], rooted with [`Trace::root`]
/// and leaved with [`trace_leaf`].
#[derive(Clone, Debug)]
pub(crate) struct Trace {
    // The linear backtraces that comprise this trace. These linear traces can
    // be re-knitted into a tree.
    backtraces: Vec<Backtrace>,
}

pin_project_lite::pin_project! {
    #[derive(Debug, Clone)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    /// A future wrapper that roots traces (captured with [`Trace::capture`]).
    pub struct Root<T> {
        #[pin]
        future: T,
    }
}

const FAIL_NO_THREAD_LOCAL: &str = "The Tokio thread-local has been destroyed \
                                    as part of shutting down the current \
                                    thread, so collecting a taskdump is not \
                                    possible.";

impl Context {
    pub(crate) const fn new() -> Self {
        Context {
            active_frame: Cell::new(None),
            trace_leaf_fn: Cell::new(None),
        }
    }

    /// SAFETY: Callers of this function must ensure that trace frames always
    /// form a valid linked list.
    unsafe fn try_with_current<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&Self) -> R,
    {
        unsafe { crate::runtime::context::with_trace(f) }
    }

    /// SAFETY: Callers of this function must ensure that trace frames always
    /// form a valid linked list.
    unsafe fn with_current_frame<F, R>(f: F) -> R
    where
        F: FnOnce(&Cell<Option<NonNull<Frame>>>) -> R,
    {
        unsafe {
            Self::try_with_current(|context| f(&context.active_frame)).expect(FAIL_NO_THREAD_LOCAL)
        }
    }

    fn current_frame_addr() -> Option<*const c_void> {
        // SAFETY: This call does not modify the linked list structure
        unsafe {
            Context::try_with_current(|ctx| {
                ctx.active_frame
                    .get()
                    .map(|frame| frame.as_ref().inner_addr)
            })
            .flatten()
        }
    }

    fn try_with_current_trace_leaf_fn<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&Cell<Option<fn(&TraceMeta)>>) -> R,
    {
        // SAFETY: This call can only access the trace_leaf_fn field, so it cannot
        // break the trace frame linked list.
        unsafe { Self::try_with_current(|context| f(&context.trace_leaf_fn)) }
    }

    /// Produces `true` if the current task is being traced; otherwise false.
    pub(crate) fn is_tracing() -> bool {
        Self::try_with_current_trace_leaf_fn(|maybe_trace_leaf| maybe_trace_leaf.get().is_some())
            .unwrap_or(false)
    }
}

/// Metadata passed into the `trace_leaf` callback for [`trace_with`]
#[non_exhaustive]
#[derive(Debug)]
pub struct TraceMeta {
    /// The root boundary address set by [`Root::poll`] if any.
    ///
    /// When using unwinding the stack, this is the address at which
    /// stack walking should stop. It corresponds to the `Root::poll` function pointer.
    pub root_addr: Option<*const c_void>,

    /// The address of the internal `trace_leaf` function that triggered this callback.
    ///
    /// When capturing a backtrace, use this as the lower bound — frames at or below
    /// this address are internal implementation details and should be excluded.
    pub trace_leaf_addr: *const c_void,
}

/// Runs `f`. If `f` hits a Tokio yield point `trace_leaf` will be invoked.
///
/// This allows taking a task dump with caller-provided task dump machinery. If `f` is the poll function of a future
/// and that future returns `Poll::Pending`, then `trace_leaf` will be invoked. `trace_leaf` can then take a backtrace
/// to determine exactly where the yield occurred.
///
/// `trace_leaf` is a function pointer (`fn`) rather than a closure (`Fn`) because it must be stored
/// in thread-local state via a `Cell`. Use thread-locals to communicate between the callback and
/// calling code (see example below).
///
/// # Example
///
/// ```
/// use std::future::Future;
/// use std::task::Poll;
/// use tokio::runtime::dump::{trace_with, Trace, TraceMeta};
///
/// // Thread-local storage for the custom trace function.
/// std::thread_local! {
///     static LEAF_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
/// }
///
/// fn my_trace_leaf(_meta: &TraceMeta) {
///     LEAF_COUNT.with(|c| c.set(c.get() + 1));
/// }
///
/// # async fn example() {
/// let mut fut = std::pin::pin!(async {
///     tokio::task::yield_now().await;
/// });
///
/// LEAF_COUNT.with(|c| c.set(0));
///
/// Trace::root(std::future::poll_fn(|cx| {
///     trace_with(|| { let _ = fut.as_mut().poll(cx); }, my_trace_leaf);
///     Poll::Ready(())
/// })).await;
///
/// let count = LEAF_COUNT.with(|c| c.get());
/// assert!(count > 0);
/// # }
/// ```
pub fn trace_with<F, R>(f: F, trace_leaf: fn(&TraceMeta)) -> R
where
    F: FnOnce() -> R,
{
    // store our new trace_leaf function
    let previous =
        Context::try_with_current_trace_leaf_fn(|current| current.replace(Some(trace_leaf)));

    // restore previous on drop. This is ensures state remains consistent
    // even if the trace_leaf function panics
    let _restore = defer(move || {
        if let Some(previous) = previous {
            Context::try_with_current_trace_leaf_fn(|current| current.set(previous));
        }
    });

    f()
}

impl Trace {
    /// Invokes `f`, returning both its result and the collection of backtraces
    /// captured at each sub-invocation of [`trace_leaf`].
    #[inline(never)]
    pub(crate) fn capture<F, R>(f: F) -> (R, Trace)
    where
        F: FnOnce() -> R,
    {
        trace_impl::capture(f)
    }

    pub(crate) fn empty() -> Self {
        Self { backtraces: vec![] }
    }

    fn push_backtrace(&mut self, bt: Vec<BacktraceFrame>) {
        self.backtraces.push(bt);
    }

    /// The root of a trace.
    #[inline(never)]
    pub(crate) fn root<F>(future: F) -> Root<F> {
        Root { future }
    }

    pub(crate) fn backtraces(&self) -> &[Backtrace] {
        &self.backtraces
    }
}

/// If this is a sub-invocation of [`Trace::capture`], capture a backtrace.
///
/// The captured backtrace will be returned by [`Trace::capture`].
///
/// Invoking this function does nothing when it is not a sub-invocation
/// [`Trace::capture`].
// This function is marked `#[inline(never)]` to ensure that it gets a distinct `Frame` in the
// backtrace, below which frames should not be included in the backtrace (since they reflect the
// internal implementation details of this crate).
#[inline(never)]
pub(crate) fn trace_leaf(cx: &mut task::Context<'_>) -> Poll<()> {
    let trace_leaf_fn = Context::try_with_current_trace_leaf_fn(|cell| cell.get()).flatten();
    if let Some(trace_leaf_fn) = trace_leaf_fn {
        let meta = TraceMeta {
            root_addr: Context::current_frame_addr(),
            trace_leaf_addr: trace_leaf as *const c_void,
        };
        trace_leaf_fn(&meta);

        // Use the same logic that `yield_now` uses to send out wakeups after
        // the task yields.
        context::with_scheduler(|scheduler| {
            if let Some(scheduler) = scheduler {
                match scheduler {
                    scheduler::Context::CurrentThread(s) => s.defer.defer(cx.waker()),
                    #[cfg(feature = "rt-multi-thread")]
                    scheduler::Context::MultiThread(s) => s.defer.defer(cx.waker()),
                }
            }
        });

        Poll::Pending
    } else {
        Poll::Ready(())
    }
}

impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Tree::from_trace(self.clone()).fmt(f)
    }
}

fn defer<F: FnOnce() -> R, R>(f: F) -> impl Drop {
    use std::mem::ManuallyDrop;

    struct Defer<F: FnOnce() -> R, R>(ManuallyDrop<F>);

    impl<F: FnOnce() -> R, R> Drop for Defer<F, R> {
        #[inline(always)]
        fn drop(&mut self) {
            unsafe {
                ManuallyDrop::take(&mut self.0)();
            }
        }
    }

    Defer(ManuallyDrop::new(f))
}

impl<T: Future> Future for Root<T> {
    type Output = T::Output;

    #[inline(never)]
    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // SAFETY: The context's current frame is restored to its original state
        // before `frame` is dropped.
        unsafe {
            let mut frame = Frame {
                inner_addr: Self::poll as *const c_void,
                parent: None,
            };

            Context::with_current_frame(|current| {
                frame.parent = current.take();
                current.set(Some(NonNull::from(&frame)));
            });

            let _restore = defer(|| {
                Context::with_current_frame(|current| {
                    current.set(frame.parent);
                });
            });

            let this = self.project();
            this.future.poll(cx)
        }
    }
}

/// Trace and poll all tasks of the `current_thread` runtime.
pub(in crate::runtime) fn trace_current_thread(
    owned: &OwnedTasks<Arc<current_thread::Handle>>,
    local: &mut VecDeque<Notified<Arc<current_thread::Handle>>>,
    injection: &Inject<Arc<current_thread::Handle>>,
) -> Vec<(Id, Trace)> {
    // clear the local and injection queues

    let mut dequeued = Vec::new();

    while let Some(task) = local.pop_back() {
        dequeued.push(task);
    }

    while let Some(task) = injection.pop() {
        dequeued.push(task);
    }

    // precondition: We have drained the tasks from the injection queue.
    trace_owned(owned, dequeued)
}

cfg_rt_multi_thread! {
    use crate::loom::sync::Mutex;
    use crate::runtime::scheduler::multi_thread;
    use crate::runtime::scheduler::multi_thread::Synced;
    use crate::runtime::scheduler::inject::Shared;

    /// Trace and poll all tasks of the `current_thread` runtime.
    ///
    /// ## Safety
    ///
    /// Must be called with the same `synced` that `injection` was created with.
    pub(in crate::runtime) unsafe fn trace_multi_thread(
        owned: &OwnedTasks<Arc<multi_thread::Handle>>,
        local: &mut multi_thread::queue::Local<Arc<multi_thread::Handle>>,
        synced: &Mutex<Synced>,
        injection: &Shared<Arc<multi_thread::Handle>>,
    ) -> Vec<(Id, Trace)> {
        let mut dequeued = Vec::new();

        // clear the local queue
        while let Some(notified) = local.pop() {
            dequeued.push(notified);
        }

        // clear the injection queue
        let mut synced = synced.lock();
        // Safety: exactly the same safety requirements as `trace_multi_thread` function.
        while let Some(notified) = unsafe { injection.pop(&mut synced.inject) } {
            dequeued.push(notified);
        }

        drop(synced);

        // precondition: we have drained the tasks from the local and injection
        // queues.
        trace_owned(owned, dequeued)
    }
}

/// Trace the `OwnedTasks`.
///
/// # Preconditions
///
/// This helper presumes exclusive access to each task. The tasks must not exist
/// in any other queue.
fn trace_owned<S: Schedule>(owned: &OwnedTasks<S>, dequeued: Vec<Notified<S>>) -> Vec<(Id, Trace)> {
    let mut tasks = dequeued;
    // Notify and trace all un-notified tasks. The dequeued tasks are already
    // notified and so do not need to be re-notified.
    owned.for_each(|task| {
        // Notify the task (and thus make it poll-able) and stash it. This fails
        // if the task is already notified. In these cases, we skip tracing the
        // task.
        if let Some(notified) = task.notify_for_tracing() {
            tasks.push(notified);
        }
        // We do not poll tasks here, since we hold a lock on `owned` and the
        // task may complete and need to remove itself from `owned`. Polling
        // such a task here would result in a deadlock.
    });

    tasks
        .into_iter()
        .map(|task| {
            let local_notified = owned.assert_owner(task);
            let id = local_notified.task.id();
            let ((), trace) = Trace::capture(|| local_notified.run());
            (id, trace)
        })
        .collect()
}
