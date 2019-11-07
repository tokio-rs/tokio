use std::cell::{Cell, RefCell};
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;

thread_local!(static ENTERED: Cell<bool> = Cell::new(false));

/// Represents an executor context.
///
/// For more details, see [`enter` documentation](fn.enter.html)
pub(crate) struct Enter {
    _p: PhantomData<RefCell<()>>,
}

/// Marks the current thread as being within the dynamic extent of an
/// executor.
pub(crate) fn enter() -> Enter {
    if let Some(enter) = try_enter() {
        return enter;
    }

    panic!(
        "Cannot start a runtime from within a runtime. This happens \
         because a function (like `block_on`) attempted to block the \
         current thread while the thread is being used to drive \
         asynchronous tasks."
    );
}

/// Tries to enter a runtime context, returns `None` if already in a runtime
/// context.
pub(crate) fn try_enter() -> Option<Enter> {
    ENTERED.with(|c| {
        if c.get() {
            None
        } else {
            c.set(true);
            Some(Enter { _p: PhantomData })
        }
    })
}

// Forces the current "entered" state to be cleared while the closure
// is executed.
//
// # Warning
//
// This is hidden for a reason. Do not use without fully understanding
// executors. Misuing can easily cause your program to deadlock.
#[cfg(feature = "rt-full")]
pub(crate) fn exit<F: FnOnce() -> R, R>(f: F) -> R {
    // Reset in case the closure panics
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ENTERED.with(|c| {
                c.set(true);
            });
        }
    }

    ENTERED.with(|c| {
        debug_assert!(c.get());
        c.set(false);
    });

    let reset = Reset;
    let ret = f();
    ::std::mem::forget(reset);

    ENTERED.with(|c| {
        assert!(!c.get(), "closure claimed permanent executor");
        c.set(true);
    });

    ret
}

impl Enter {
    /// Blocks the thread on the specified future, returning the value with
    /// which that future completes.
    pub(crate) fn block_on<F: Future>(&mut self, mut f: F) -> F::Output {
        use crate::runtime::park::{CachedParkThread, Park};
        use std::pin::Pin;
        use std::task::Context;
        use std::task::Poll::Ready;

        let mut park = CachedParkThread::new();
        let waker = park.unpark().into_waker();
        let mut cx = Context::from_waker(&waker);

        // `block_on` takes ownership of `f`. Once it is pinned here, the original `f` binding can
        // no longer be accessed, making the pinning safe.
        let mut f = unsafe { Pin::new_unchecked(&mut f) };

        loop {
            if let Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
            park.park().unwrap();
        }
    }
}

impl fmt::Debug for Enter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Enter").finish()
    }
}

impl Drop for Enter {
    fn drop(&mut self) {
        ENTERED.with(|c| {
            assert!(c.get());
            c.set(false);
        });
    }
}
