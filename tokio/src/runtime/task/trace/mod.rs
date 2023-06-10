use crate::loom::sync::Arc;
use crate::runtime::context;
use crate::runtime::scheduler::{self, current_thread, Inject};

use backtrace::BacktraceFrame;
use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::ptr::{self, NonNull};
use std::task::{self, Poll};

mod symbol;
mod tree;

use symbol::Symbol;
use tree::Tree;

use super::{Notified, OwnedTasks};

type Backtrace = Vec<BacktraceFrame>;
type SymbolTrace = Vec<Symbol>;

/// The ambiant backtracing context.
pub(crate) struct Context {
    /// The address of [`Trace::root`] establishes an upper unwinding bound on
    /// the backtraces in `Trace`.
    active_frame: Cell<Option<NonNull<Frame>>>,
    /// The place to stash backtraces.
    collector: Cell<Option<Trace>>,
}

/// A [`Frame`] in an intrusive, doubly-linked tree of [`Frame`]s.
struct Frame {
    /// The location associated with this frame.
    inner_addr: *const c_void,

    /// The parent frame, if any.
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
    pub(crate) struct Root<T> {
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
            collector: Cell::new(None),
        }
    }

    /// SAFETY: Callers of this function must ensure that trace frames always
    /// form a valid linked list.
    unsafe fn try_with_current<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&Self) -> R,
    {
        crate::runtime::context::with_trace(f)
    }

    unsafe fn with_current_frame<F, R>(f: F) -> R
    where
        F: FnOnce(&Cell<Option<NonNull<Frame>>>) -> R,
    {
        Self::try_with_current(|context| f(&context.active_frame)).expect(FAIL_NO_THREAD_LOCAL)
    }

    fn with_current_collector<F, R>(f: F) -> R
    where
        F: FnOnce(&Cell<Option<Trace>>) -> R,
    {
        // SAFETY: This call can only access the collector field, so it cannot
        // break the trace frame linked list.
        unsafe {
            Self::try_with_current(|context| f(&context.collector)).expect(FAIL_NO_THREAD_LOCAL)
        }
    }
}

impl Trace {
    /// Invokes `f`, returning both its result and the collection of backtraces
    /// captured at each sub-invocation of [`trace_leaf`].
    #[inline(never)]
    pub(crate) fn capture<F, R>(f: F) -> (R, Trace)
    where
        F: FnOnce() -> R,
    {
        let collector = Trace { backtraces: vec![] };

        let previous = Context::with_current_collector(|current| current.replace(Some(collector)));

        let result = f();

        let collector =
            Context::with_current_collector(|current| current.replace(previous)).unwrap();

        (result, collector)
    }

    /// The root of a trace.
    #[inline(never)]
    pub(crate) fn root<F>(future: F) -> Root<F> {
        Root { future }
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
    // Safety: We don't manipulate the current context's active frame.
    let did_trace = unsafe {
        Context::try_with_current(|context_cell| {
            if let Some(mut collector) = context_cell.collector.take() {
                let mut frames = vec![];
                let mut above_leaf = false;

                if let Some(active_frame) = context_cell.active_frame.get() {
                    let active_frame = active_frame.as_ref();

                    backtrace::trace(|frame| {
                        let below_root = !ptr::eq(frame.symbol_address(), active_frame.inner_addr);

                        // only capture frames above `Trace::leaf` and below
                        // `Trace::root`.
                        if above_leaf && below_root {
                            frames.push(frame.to_owned().into());
                        }

                        if ptr::eq(frame.symbol_address(), trace_leaf as *const _) {
                            above_leaf = true;
                        }

                        // only continue unwinding if we're below `Trace::root`
                        below_root
                    });
                }
                collector.backtraces.push(frames);
                context_cell.collector.set(Some(collector));
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
    };

    if did_trace {
        // Use the same logic that `yield_now` uses to send out wakeups after
        // the task yields.
        context::with_scheduler(|scheduler| {
            if let Some(scheduler) = scheduler {
                match scheduler {
                    scheduler::Context::CurrentThread(s) => s.defer.defer(cx.waker()),
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
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

/// Trace and poll all tasks of the current_thread runtime.
pub(in crate::runtime) fn trace_current_thread(
    owned: &OwnedTasks<Arc<current_thread::Handle>>,
    local: &mut VecDeque<Notified<Arc<current_thread::Handle>>>,
    injection: &Inject<Arc<current_thread::Handle>>,
) -> Vec<Trace> {
    // clear the local and injection queues
    local.clear();

    while let Some(task) = injection.pop() {
        drop(task);
    }

    // notify each task
    let mut tasks = vec![];
    owned.for_each(|task| {
        // set the notified bit
        task.as_raw().state().transition_to_notified_for_tracing();
        // store the raw tasks into a vec
        tasks.push(task.as_raw());
    });

    tasks
        .into_iter()
        .map(|task| {
            let ((), trace) = Trace::capture(|| task.poll());
            trace
        })
        .collect()
}

cfg_rt_multi_thread! {
    use crate::loom::sync::Mutex;
    use crate::runtime::scheduler::multi_thread;
    use crate::runtime::scheduler::multi_thread::Synced;
    use crate::runtime::scheduler::inject::Shared;

    /// Trace and poll all tasks of the current_thread runtime.
    ///
    /// ## Safety
    ///
    /// Must be called with the same `synced` that `injection` was created with.
    pub(in crate::runtime) unsafe fn trace_multi_thread(
        owned: &OwnedTasks<Arc<multi_thread::Handle>>,
        local: &mut multi_thread::queue::Local<Arc<multi_thread::Handle>>,
        synced: &Mutex<Synced>,
        injection: &Shared<Arc<multi_thread::Handle>>,
    ) -> Vec<Trace> {
        // clear the local queue
        while let Some(notified) = local.pop() {
            drop(notified);
        }

        // clear the injection queue
        let mut synced = synced.lock();
        while let Some(notified) = injection.pop(&mut synced.inject) {
            drop(notified);
        }

        drop(synced);

        // notify each task
        let mut traces = vec![];
        owned.for_each(|task| {
            // set the notified bit
            task.as_raw().state().transition_to_notified_for_tracing();

            // trace the task
            let ((), trace) = Trace::capture(|| task.as_raw().poll());
            traces.push(trace);

            // reschedule the task
            let _ = task.as_raw().state().transition_to_notified_by_ref();
            task.as_raw().schedule();
        });

        traces
    }
}
