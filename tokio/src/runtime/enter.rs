use std::cell::{Cell, RefCell};
use std::fmt;
use std::marker::PhantomData;

thread_local!(static ENTERED: Cell<bool> = Cell::new(false));

/// Represents an executor context.
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
#[cfg(all(feature = "rt-threaded", feature = "blocking"))]
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
    std::mem::forget(reset);

    ENTERED.with(|c| {
        assert!(!c.get(), "closure claimed permanent executor");
        c.set(true);
    });

    ret
}

cfg_blocking_impl! {
    use crate::park::ParkError;
    use std::time::Duration;

    impl Enter {
        /// Blocks the thread on the specified future, returning the value with
        /// which that future completes.
        pub(crate) fn block_on<F>(&mut self, mut f: F) -> Result<F::Output, ParkError>
        where
            F: std::future::Future,
        {
            use crate::park::{CachedParkThread, Park};
            use std::pin::Pin;
            use std::task::Context;
            use std::task::Poll::Ready;

            let mut park = CachedParkThread::new();
            let waker = park.get_unpark()?.into_waker();
            let mut cx = Context::from_waker(&waker);

            // `block_on` takes ownership of `f`. Once it is pinned here, the original `f` binding can
            // no longer be accessed, making the pinning safe.
            let mut f = unsafe { Pin::new_unchecked(&mut f) };

            loop {
                if let Ready(v) = crate::coop::budget(|| f.as_mut().poll(&mut cx)) {
                    return Ok(v);
                }

                park.park()?;
            }
        }

        /// Blocks the thread on the specified future for **at most** `timeout`
        ///
        /// If the future completes before `timeout`, the result is returned. If
        /// `timeout` elapses, then `Err` is returned.
        pub(crate) fn block_on_timeout<F>(&mut self, mut f: F, timeout: Duration) -> Result<F::Output, ParkError>
        where
            F: std::future::Future,
        {
            use crate::park::{CachedParkThread, Park};
            use std::pin::Pin;
            use std::task::Context;
            use std::task::Poll::Ready;
            use std::time::Instant;

            let mut park = CachedParkThread::new();
            let waker = park.get_unpark()?.into_waker();
            let mut cx = Context::from_waker(&waker);

            // `block_on` takes ownership of `f`. Once it is pinned here, the original `f` binding can
            // no longer be accessed, making the pinning safe.
            let mut f = unsafe { Pin::new_unchecked(&mut f) };
            let when = Instant::now() + timeout;

            loop {
                if let Ready(v) = crate::coop::budget(|| f.as_mut().poll(&mut cx)) {
                    return Ok(v);
                }

                let now = Instant::now();

                if now >= when {
                    return Err(());
                }

                park.park_timeout(when - now)?;
            }
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
