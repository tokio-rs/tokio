use crate::runtime::scheduler;

use std::cell::{Cell, RefCell};
use std::fmt;
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EnterContext {
    /// Currently in a runtime context.
    #[cfg_attr(not(feature = "rt"), allow(dead_code))]
    Entered { allow_block_in_place: bool },

    /// Not in a runtime context **or** a blocking region.
    NotEntered,
}

impl EnterContext {
    pub(crate) fn is_entered(self) -> bool {
        matches!(self, EnterContext::Entered { .. })
    }
}

tokio_thread_local!(static ENTERED: Cell<EnterContext> = const { Cell::new(EnterContext::NotEntered) });

/// Guard tracking that a caller has entered a runtime context.
pub(crate) struct EnterRuntimeGuard {
    pub(crate) blocking: BlockingRegionGuard,
}

/// Guard tracking that a caller has entered a blocking region.
pub(crate) struct BlockingRegionGuard {
    _p: PhantomData<RefCell<()>>,
}

cfg_rt! {
    use crate::runtime::context;

    use std::time::Duration;

    /// Marks the current thread as being within the dynamic extent of an
    /// executor.
    #[track_caller]
    pub(crate) fn enter_runtime(handle: &scheduler::Handle, allow_block_in_place: bool) -> EnterRuntimeGuard {
        if let Some(enter) = try_enter_runtime(allow_block_in_place) {
            // Set the current runtime handle. This should not fail. A later
            // cleanup will remove the unwrap().
            context::try_set_current(handle).unwrap();
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
    fn try_enter_runtime(allow_block_in_place: bool) -> Option<EnterRuntimeGuard> {
        ENTERED.with(|c| {
            if c.get().is_entered() {
                None
            } else {
                c.set(EnterContext::Entered { allow_block_in_place });
                Some(EnterRuntimeGuard {
                    blocking: BlockingRegionGuard::new(),
                })
            }
        })
    }

    pub(crate) fn try_enter_blocking_region() -> Option<BlockingRegionGuard> {
        ENTERED.with(|c| {
            if c.get().is_entered() {
                None
            } else {
                Some(BlockingRegionGuard::new())
            }
        })
    }
}

// Forces the current "entered" state to be cleared while the closure
// is executed.
//
// # Warning
//
// This is hidden for a reason. Do not use without fully understanding
// executors. Misusing can easily cause your program to deadlock.
cfg_rt_multi_thread! {
    pub(crate) fn exit_runtime<F: FnOnce() -> R, R>(f: F) -> R {
        // Reset in case the closure panics
        struct Reset(EnterContext);
        impl Drop for Reset {
            fn drop(&mut self) {
                ENTERED.with(|c| {
                    assert!(!c.get().is_entered(), "closure claimed permanent executor");
                    c.set(self.0);
                });
            }
        }

        let was = ENTERED.with(|c| {
            let e = c.get();
            assert!(e.is_entered(), "asked to exit when not entered");
            c.set(EnterContext::NotEntered);
            e
        });

        let _reset = Reset(was);
        // dropping _reset after f() will reset ENTERED
        f()
    }
}

cfg_rt! {
    /// Disallows blocking in the current runtime context until the guard is dropped.
    pub(crate) fn disallow_block_in_place() -> DisallowBlockInPlaceGuard {
        let reset = ENTERED.with(|c| {
            if let EnterContext::Entered {
                allow_block_in_place: true,
            } = c.get()
            {
                c.set(EnterContext::Entered {
                    allow_block_in_place: false,
                });
                true
            } else {
                false
            }
        });
        DisallowBlockInPlaceGuard(reset)
    }

    pub(crate) struct DisallowBlockInPlaceGuard(bool);
    impl Drop for DisallowBlockInPlaceGuard {
        fn drop(&mut self) {
            if self.0 {
                // XXX: Do we want some kind of assertion here, or is "best effort" okay?
                ENTERED.with(|c| {
                    if let EnterContext::Entered {
                        allow_block_in_place: false,
                    } = c.get()
                    {
                        c.set(EnterContext::Entered {
                            allow_block_in_place: true,
                        });
                    }
                })
            }
        }
    }
}

cfg_rt_multi_thread! {
    /// Returns true if in a runtime context.
    pub(crate) fn context() -> EnterContext {
        ENTERED.with(|c| c.get())
    }
}

cfg_rt! {
    use crate::loom::thread::AccessError;

    impl BlockingRegionGuard {
        fn new() -> BlockingRegionGuard {
            BlockingRegionGuard { _p: PhantomData }
        }
        /// Blocks the thread on the specified future, returning the value with
        /// which that future completes.
        pub(crate) fn block_on<F>(&mut self, f: F) -> Result<F::Output, AccessError>
        where
            F: std::future::Future,
        {
            use crate::runtime::park::CachedParkThread;

            let mut park = CachedParkThread::new();
            park.block_on(f)
        }

        /// Blocks the thread on the specified future for **at most** `timeout`
        ///
        /// If the future completes before `timeout`, the result is returned. If
        /// `timeout` elapses, then `Err` is returned.
        pub(crate) fn block_on_timeout<F>(&mut self, f: F, timeout: Duration) -> Result<F::Output, ()>
        where
            F: std::future::Future,
        {
            use crate::runtime::park::CachedParkThread;
            use std::task::Context;
            use std::task::Poll::Ready;
            use std::time::Instant;

            let mut park = CachedParkThread::new();
            let waker = park.waker().map_err(|_| ())?;
            let mut cx = Context::from_waker(&waker);

            pin!(f);
            let when = Instant::now() + timeout;

            loop {
                if let Ready(v) = crate::runtime::coop::budget(|| f.as_mut().poll(&mut cx)) {
                    return Ok(v);
                }

                let now = Instant::now();

                if now >= when {
                    return Err(());
                }

                park.park_timeout(when - now);
            }
        }
    }
}

impl fmt::Debug for EnterRuntimeGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Enter").finish()
    }
}

impl Drop for EnterRuntimeGuard {
    fn drop(&mut self) {
        ENTERED.with(|c| {
            assert!(c.get().is_entered());
            c.set(EnterContext::NotEntered);
        });
    }
}
