use crate::loom::thread::AccessError;
use crate::runtime::coop;

use std::cell::Cell;

#[cfg(any(feature = "rt", feature = "macros"))]
use crate::util::rand::FastRand;

cfg_rt! {
    mod scoped;
    use scoped::Scoped;

    use crate::runtime::{scheduler, task::Id};

    use std::cell::RefCell;
    use std::marker::PhantomData;
    use std::task::Waker;
    use std::time::Duration;

    cfg_taskdump! {
        use crate::runtime::task::trace;
    }
}

struct Context {
    /// Uniquely identifies the current thread
    #[cfg(feature = "rt")]
    thread_id: Cell<Option<ThreadId>>,

    /// Handle to the runtime scheduler running on the current thread.
    #[cfg(feature = "rt")]
    handle: RefCell<Option<scheduler::Handle>>,

    /// Handle to the scheduler's internal "context"
    #[cfg(feature = "rt")]
    scheduler: Scoped<scheduler::Context>,

    #[cfg(feature = "rt")]
    current_task_id: Cell<Option<Id>>,

    /// Tracks if the current thread is currently driving a runtime.
    /// Note, that if this is set to "entered", the current scheduler
    /// handle may not reference the runtime currently executing. This
    /// is because other runtime handles may be set to current from
    /// within a runtime.
    #[cfg(feature = "rt")]
    runtime: Cell<EnterRuntime>,

    #[cfg(any(feature = "rt", feature = "macros"))]
    rng: Cell<Option<FastRand>>,

    /// Tracks the amount of "work" a task may still do before yielding back to
    /// the sheduler
    budget: Cell<coop::Budget>,

    #[cfg(all(
        tokio_unstable,
        tokio_taskdump,
        feature = "rt",
        target_os = "linux",
        any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")
    ))]
    trace: trace::Context,
}

tokio_thread_local! {
    static CONTEXT: Context = const {
        Context {
            #[cfg(feature = "rt")]
            thread_id: Cell::new(None),

            /// Tracks the current runtime handle to use when spawning,
            /// accessing drivers, etc...
            #[cfg(feature = "rt")]
            handle: RefCell::new(None),

            /// Tracks the current scheduler internal context
            #[cfg(feature = "rt")]
            scheduler: Scoped::new(),

            #[cfg(feature = "rt")]
            current_task_id: Cell::new(None),

            /// Tracks if the current thread is currently driving a runtime.
            /// Note, that if this is set to "entered", the current scheduler
            /// handle may not reference the runtime currently executing. This
            /// is because other runtime handles may be set to current from
            /// within a runtime.
            #[cfg(feature = "rt")]
            runtime: Cell::new(EnterRuntime::NotEntered),

            #[cfg(any(feature = "rt", feature = "macros"))]
            rng: Cell::new(None),

            budget: Cell::new(coop::Budget::unconstrained()),

            #[cfg(all(
                tokio_unstable,
                tokio_taskdump,
                feature = "rt",
                target_os = "linux",
                any(
                    target_arch = "aarch64",
                    target_arch = "x86",
                    target_arch = "x86_64"
                )
            ))]
            trace: trace::Context::new(),
        }
    }
}

#[cfg(any(feature = "macros", all(feature = "sync", feature = "rt")))]
pub(crate) fn thread_rng_n(n: u32) -> u32 {
    CONTEXT.with(|ctx| {
        let mut rng = ctx.rng.get().unwrap_or_else(FastRand::new);
        let ret = rng.fastrand_n(n);
        ctx.rng.set(Some(rng));
        ret
    })
}

pub(super) fn budget<R>(f: impl FnOnce(&Cell<coop::Budget>) -> R) -> Result<R, AccessError> {
    CONTEXT.try_with(|ctx| f(&ctx.budget))
}

cfg_rt! {
    use crate::runtime::{ThreadId, TryCurrentError};
    use crate::util::rand::RngSeed;

    use std::fmt;

    pub(crate) fn thread_id() -> Result<ThreadId, AccessError> {
        CONTEXT.try_with(|ctx| {
            match ctx.thread_id.get() {
                Some(id) => id,
                None => {
                    let id = ThreadId::next();
                    ctx.thread_id.set(Some(id));
                    id
                }
            }
        })
    }

    #[derive(Debug, Clone, Copy)]
    #[must_use]
    pub(crate) enum EnterRuntime {
        /// Currently in a runtime context.
        #[cfg_attr(not(feature = "rt"), allow(dead_code))]
        Entered { allow_block_in_place: bool },

        /// Not in a runtime context **or** a blocking region.
        NotEntered,
    }

    #[derive(Debug)]
    #[must_use]
    pub(crate) struct SetCurrentGuard {
        old_handle: Option<scheduler::Handle>,
        old_seed: RngSeed,
    }

    /// Guard tracking that a caller has entered a runtime context.
    #[must_use]
    pub(crate) struct EnterRuntimeGuard {
        /// Tracks that the current thread has entered a blocking function call.
        pub(crate) blocking: BlockingRegionGuard,

        #[allow(dead_code)] // Only tracking the guard.
        pub(crate) handle: SetCurrentGuard,
    }

    /// Guard tracking that a caller has entered a blocking region.
    #[must_use]
    pub(crate) struct BlockingRegionGuard {
        _p: PhantomData<RefCell<()>>,
    }

    pub(crate) struct DisallowBlockInPlaceGuard(bool);

    pub(crate) fn set_current_task_id(id: Option<Id>) -> Option<Id> {
        CONTEXT.try_with(|ctx| ctx.current_task_id.replace(id)).unwrap_or(None)
    }

    pub(crate) fn current_task_id() -> Option<Id> {
        CONTEXT.try_with(|ctx| ctx.current_task_id.get()).unwrap_or(None)
    }

    pub(crate) fn with_current<F, R>(f: F) -> Result<R, TryCurrentError>
    where
        F: FnOnce(&scheduler::Handle) -> R,
    {

        match CONTEXT.try_with(|ctx| ctx.handle.borrow().as_ref().map(f)) {
            Ok(Some(ret)) => Ok(ret),
            Ok(None) => Err(TryCurrentError::new_no_context()),
            Err(_access_error) => Err(TryCurrentError::new_thread_local_destroyed()),
        }
    }

    /// Sets this [`Handle`] as the current active [`Handle`].
    ///
    /// [`Handle`]: crate::runtime::scheduler::Handle
    pub(crate) fn try_set_current(handle: &scheduler::Handle) -> Option<SetCurrentGuard> {
        CONTEXT.try_with(|ctx| ctx.set_current(handle)).ok()
    }


    /// Marks the current thread as being within the dynamic extent of an
    /// executor.
    #[track_caller]
    pub(crate) fn enter_runtime(handle: &scheduler::Handle, allow_block_in_place: bool) -> EnterRuntimeGuard {
        if let Some(enter) = try_enter_runtime(handle, allow_block_in_place) {
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
    fn try_enter_runtime(handle: &scheduler::Handle, allow_block_in_place: bool) -> Option<EnterRuntimeGuard> {
        CONTEXT.with(|c| {
            if c.runtime.get().is_entered() {
                None
            } else {
                // Set the entered flag
                c.runtime.set(EnterRuntime::Entered { allow_block_in_place });

                Some(EnterRuntimeGuard {
                    blocking: BlockingRegionGuard::new(),
                    handle: c.set_current(handle),
                })
            }
        })
    }

    pub(crate) fn try_enter_blocking_region() -> Option<BlockingRegionGuard> {
        CONTEXT.try_with(|c| {
            if c.runtime.get().is_entered() {
                None
            } else {
                Some(BlockingRegionGuard::new())
            }
            // If accessing the thread-local fails, the thread is terminating
            // and thread-locals are being destroyed. Because we don't know if
            // we are currently in a runtime or not, we default to being
            // permissive.
        }).unwrap_or_else(|_| Some(BlockingRegionGuard::new()))
    }

    /// Disallows blocking in the current runtime context until the guard is dropped.
    pub(crate) fn disallow_block_in_place() -> DisallowBlockInPlaceGuard {
        let reset = CONTEXT.with(|c| {
            if let EnterRuntime::Entered {
                allow_block_in_place: true,
            } = c.runtime.get()
            {
                c.runtime.set(EnterRuntime::Entered {
                    allow_block_in_place: false,
                });
                true
            } else {
                false
            }
        });

        DisallowBlockInPlaceGuard(reset)
    }

    #[track_caller]
    pub(crate) fn defer(waker: &Waker) {
        with_scheduler(|maybe_scheduler| {
            if let Some(scheduler) = maybe_scheduler {
                scheduler.defer(waker);
            } else {
                // Called from outside of the runtime, immediately wake the
                // task.
                waker.wake_by_ref();
            }
        });
    }

    pub(super) fn set_scheduler<R>(v: &scheduler::Context, f: impl FnOnce() -> R) -> R {
        CONTEXT.with(|c| c.scheduler.set(v, f))
    }

    #[track_caller]
    pub(super) fn with_scheduler<R>(f: impl FnOnce(Option<&scheduler::Context>) -> R) -> R {
        CONTEXT.with(|c| c.scheduler.with(f))
    }

    impl Context {
        fn set_current(&self, handle: &scheduler::Handle) -> SetCurrentGuard {
            let rng_seed = handle.seed_generator().next_seed();

            let old_handle = self.handle.borrow_mut().replace(handle.clone());
            let mut rng = self.rng.get().unwrap_or_else(FastRand::new);
            let old_seed = rng.replace_seed(rng_seed);
            self.rng.set(Some(rng));

            SetCurrentGuard {
                old_handle,
                old_seed,
            }
        }
    }

    impl Drop for SetCurrentGuard {
        fn drop(&mut self) {
            CONTEXT.with(|ctx| {
                *ctx.handle.borrow_mut() = self.old_handle.take();

                let mut rng = ctx.rng.get().unwrap_or_else(FastRand::new);
                rng.replace_seed(self.old_seed.clone());
                ctx.rng.set(Some(rng));
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
            CONTEXT.with(|c| {
                assert!(c.runtime.get().is_entered());
                c.runtime.set(EnterRuntime::NotEntered);
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
                CONTEXT.with(|c| {
                    if let EnterRuntime::Entered {
                        allow_block_in_place: false,
                    } = c.runtime.get()
                    {
                        c.runtime.set(EnterRuntime::Entered {
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

    cfg_taskdump! {
        /// SAFETY: Callers of this function must ensure that trace frames always
        /// form a valid linked list.
        pub(crate) unsafe fn with_trace<R>(f: impl FnOnce(&trace::Context) -> R) -> Option<R> {
            CONTEXT.try_with(|c| f(&c.trace)).ok()
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
        CONTEXT.with(|c| c.runtime.get())
    }

    pub(crate) fn exit_runtime<F: FnOnce() -> R, R>(f: F) -> R {
        // Reset in case the closure panics
        struct Reset(EnterRuntime);

        impl Drop for Reset {
            fn drop(&mut self) {
                CONTEXT.with(|c| {
                    assert!(!c.runtime.get().is_entered(), "closure claimed permanent executor");
                    c.runtime.set(self.0);
                });
            }
        }

        let was = CONTEXT.with(|c| {
            let e = c.runtime.get();
            assert!(e.is_entered(), "asked to exit when not entered");
            c.runtime.set(EnterRuntime::NotEntered);
            e
        });

        let _reset = Reset(was);
        // dropping _reset after f() will reset ENTERED
        f()
    }
}
