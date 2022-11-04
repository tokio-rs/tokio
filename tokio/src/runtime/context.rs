use crate::runtime::coop;

use std::cell::Cell;

#[cfg(any(feature = "rt", feature = "macros"))]
use crate::util::rand::{FastRand, RngSeed};

cfg_rt! {
    use crate::runtime::scheduler;

    use std::cell::RefCell;
    use std::marker::PhantomData;
    use std::time::Duration;
}

struct Context {
    /// Handle to the runtime scheduler running on the current thread.
    #[cfg(feature = "rt")]
    scheduler: RefCell<Option<scheduler::Handle>>,

    #[cfg(any(feature = "rt", feature = "macros"))]
    rng: FastRand,

    /// Tracks the amount of "work" a task may still do before yielding back to
    /// the sheduler
    budget: Cell<coop::Budget>,
}

tokio_thread_local! {
    static CONTEXT: Context = {
        Context {
            #[cfg(feature = "rt")]
            scheduler: RefCell::new(None),

            #[cfg(any(feature = "rt", feature = "macros"))]
            rng: FastRand::new(RngSeed::new()),
            budget: Cell::new(coop::Budget::unconstrained()),
        }
    }
}

#[cfg(feature = "rt")]
tokio_thread_local!(static ENTERED: Cell<EnterRuntime> = const { Cell::new(EnterRuntime::NotEntered) });

#[cfg(feature = "macros")]
pub(crate) fn thread_rng_n(n: u32) -> u32 {
    CONTEXT.with(|ctx| ctx.rng.fastrand_n(n))
}

pub(super) fn budget<R>(f: impl FnOnce(&Cell<coop::Budget>) -> R) -> R {
    CONTEXT.with(|ctx| f(&ctx.budget))
}

cfg_rt! {
    use crate::loom::thread::AccessError;
    use crate::runtime::TryCurrentError;

    use std::fmt;

    #[derive(Debug, Clone, Copy)]
    pub(crate) enum EnterRuntime {
        /// Currently in a runtime context.
        #[cfg_attr(not(feature = "rt"), allow(dead_code))]
        Entered { allow_block_in_place: bool },

        /// Not in a runtime context **or** a blocking region.
        NotEntered,
    }

    #[derive(Debug)]
    pub(crate) struct SetCurrentGuard {
        old_handle: Option<scheduler::Handle>,
        old_seed: RngSeed,
    }

    /// Guard tracking that a caller has entered a runtime context.
    pub(crate) struct EnterRuntimeGuard {
        pub(crate) blocking: BlockingRegionGuard,
    }

    /// Guard tracking that a caller has entered a blocking region.
    pub(crate) struct BlockingRegionGuard {
        _p: PhantomData<RefCell<()>>,
    }

    pub(crate) struct DisallowBlockInPlaceGuard(bool);

    pub(crate) fn try_current() -> Result<scheduler::Handle, TryCurrentError> {
        match CONTEXT.try_with(|ctx| ctx.scheduler.borrow().clone()) {
            Ok(Some(handle)) => Ok(handle),
            Ok(None) => Err(TryCurrentError::new_no_context()),
            Err(_access_error) => Err(TryCurrentError::new_thread_local_destroyed()),
        }
    }

    /// Sets this [`Handle`] as the current active [`Handle`].
    ///
    /// [`Handle`]: crate::runtime::scheduler::Handle
    pub(crate) fn try_set_current(handle: &scheduler::Handle) -> Option<SetCurrentGuard> {
        let rng_seed = handle.seed_generator().next_seed();

        CONTEXT.try_with(|ctx| {
            let old_handle = ctx.scheduler.borrow_mut().replace(handle.clone());
            let old_seed = ctx.rng.replace_seed(rng_seed);

            SetCurrentGuard {
                old_handle,
                old_seed,
            }
        }).ok()
    }


    /// Marks the current thread as being within the dynamic extent of an
    /// executor.
    #[track_caller]
    pub(crate) fn enter_runtime(handle: &scheduler::Handle, allow_block_in_place: bool) -> EnterRuntimeGuard {
        if let Some(enter) = try_enter_runtime(allow_block_in_place) {
            // Set the current runtime handle. This should not fail. A later
            // cleanup will remove the unwrap().
            try_set_current(handle).unwrap();
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
                c.set(EnterRuntime::Entered { allow_block_in_place });
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

    /// Disallows blocking in the current runtime context until the guard is dropped.
    pub(crate) fn disallow_block_in_place() -> DisallowBlockInPlaceGuard {
        let reset = ENTERED.with(|c| {
            if let EnterRuntime::Entered {
                allow_block_in_place: true,
            } = c.get()
            {
                c.set(EnterRuntime::Entered {
                    allow_block_in_place: false,
                });
                true
            } else {
                false
            }
        });
        DisallowBlockInPlaceGuard(reset)
    }

    impl Drop for SetCurrentGuard {
        fn drop(&mut self) {
            CONTEXT.with(|ctx| {
                *ctx.scheduler.borrow_mut() = self.old_handle.take();
                ctx.rng.replace_seed(self.old_seed.clone());
            });
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
                c.set(EnterRuntime::NotEntered);
            });
        }
    }

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

    impl Drop for DisallowBlockInPlaceGuard {
        fn drop(&mut self) {
            if self.0 {
                // XXX: Do we want some kind of assertion here, or is "best effort" okay?
                ENTERED.with(|c| {
                    if let EnterRuntime::Entered {
                        allow_block_in_place: false,
                    } = c.get()
                    {
                        c.set(EnterRuntime::Entered {
                            allow_block_in_place: true,
                        });
                    }
                })
            }
        }
    }

    impl EnterRuntime {
        pub(crate) fn is_entered(self) -> bool {
            matches!(self, EnterRuntime::Entered { .. })
        }
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
    /// Returns true if in a runtime context.
    pub(crate) fn current_enter_context() -> EnterRuntime {
        ENTERED.with(|c| c.get())
    }

    pub(crate) fn exit_runtime<F: FnOnce() -> R, R>(f: F) -> R {
        // Reset in case the closure panics
        struct Reset(EnterRuntime);

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
            c.set(EnterRuntime::NotEntered);
            e
        });

        let _reset = Reset(was);
        // dropping _reset after f() will reset ENTERED
        f()
    }
}
