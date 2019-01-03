pub use tokio_trace_core::dispatcher::*;

use std::cell::RefCell;

thread_local! {
    static CURRENT_DISPATCH: RefCell<Dispatch> = RefCell::new(Dispatch::none());
}

/// Sets this dispatch as the default for the duration of a closure.
///
/// The default dispatcher is used when creating a new [`Span`] or
/// [`Event`], _if no span is currently executing_. If a span is currently
/// executing, new spans or events are dispatched to the subscriber that
/// tagged that span, instead.
///
/// [`Span`]: ::span::Span
/// [`Subscriber`]: ::Subscriber
/// [`Event`]: ::Event
pub fn with_default<T>(dispatcher: Dispatch, f: impl FnOnce() -> T) -> T {
    let prior = CURRENT_DISPATCH.try_with(|current| current.replace(dispatcher));
    let result = f();
    if let Ok(prior) = prior {
        let _ = CURRENT_DISPATCH.try_with(|current| {
            *current.borrow_mut() = prior;
        });
    }
    result
}

pub(crate) fn with_current<T, F>(mut f: F) -> T
where
    F: FnMut(&Dispatch) -> T,
{
    CURRENT_DISPATCH
        .try_with(|current| f(&*current.borrow()))
        .unwrap_or_else(|_| f(&Dispatch::none()))
}
