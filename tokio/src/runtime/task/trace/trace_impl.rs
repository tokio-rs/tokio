//! Current `backtrace::trace` + collector based backtrace implementation
//!
//! This implementation may eventually be extracted into a separate `tokio-taskdump` crate.

use std::{cell::Cell, ptr};

use crate::runtime::task::trace::{trace_with, Trace, TraceMeta};

use super::defer;

/// Thread local state used to communicate between calling the trace and the interior `trace_leaf` function
struct TraceContext {
    collector: Cell<Option<Trace>>,
}

thread_local! {
    static TRACE_CONTEXT: TraceContext = const {
        TraceContext {
            collector: Cell::new(None),
        }
    };
}

/// Capture using the default `backtrace::trace`-based implementation.
#[inline(never)]
pub(super) fn capture<F, R>(f: F) -> (R, Trace)
where
    F: FnOnce() -> R,
{
    let collector = Trace::empty();

    let previous = TRACE_CONTEXT.with(|state| state.collector.replace(Some(collector)));

    // restore previous collector on drop even if the callback panics
    let _restore = defer(move || {
        TRACE_CONTEXT.with(|state| state.collector.set(previous));
    });

    let result = trace_with(f, trace_leaf);

    // take the collector before _restore runs
    let collector = TRACE_CONTEXT.with(|state| state.collector.take()).unwrap();

    (result, collector)
}

/// Capture a backtrace via `backtrace::trace` and collect it into `STATE`
#[inline(never)]
pub(crate) fn trace_leaf(meta: &TraceMeta) {
    TRACE_CONTEXT.with(|state| {
        if let Some(mut collector) = state.collector.take() {
            let mut frames: Vec<backtrace::BacktraceFrame> = vec![];
            let mut above_leaf = false;

            if let Some(root_addr) = meta.root_addr {
                backtrace::trace(|frame| {
                    let below_root = !ptr::eq(frame.symbol_address(), root_addr);

                    if above_leaf && below_root {
                        frames.push(frame.to_owned().into());
                    }

                    if ptr::eq(frame.symbol_address(), meta.trace_leaf_addr) {
                        above_leaf = true;
                    }

                    below_root
                });
            }
            collector.push_backtrace(frames);
            state.collector.set(Some(collector));
        }
    });
}
