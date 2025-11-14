cfg_rt! {
    use crate::task::Id;
    use crate::util::trace::SpawnMeta;

    #[repr(u8)]
    pub(crate) enum TaskKind {
        #[allow(dead_code)]
        BlockOn = 0,
        Spawn = 1,
        SpawnLocal = 2,
    }

    #[repr(u8)]
    pub(crate) enum TerminateKind {
        Success = 0,
        Cancelled = 1,
        Panicked = 2,
    }
}

cfg_usdt! {
    #[cfg(all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")))]
    #[path = "macos.rs"]
    mod usdt_impl;

    #[cfg(all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))]
    #[path = "stapsdt.rs"]
    mod usdt_impl;

    #[cfg(not(any(
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    )))]
    compile_error!(
        "The `usdt` feature is only currently supported on linux/macos, on `aarch64` and `x86_64`."
    );

    pub(crate) use usdt_impl::trace_root;

    cfg_rt! {
        use core::{
            pin::Pin,
            task::{Context, Poll},
        };
        use pin_project_lite::pin_project;
        use std::future::Future;

        pub(crate) use usdt_impl::{waker_clone, waker_wake, waker_drop};

        #[inline(never)]
        pub(crate) fn start_task(kind: TaskKind, meta: SpawnMeta<'_>, id: Id, size: usize) {
            usdt_impl::task_start(id.as_u64(), kind as u8, size, meta.original_size);
            usdt_impl::task_details(
                id.as_u64(),
                meta.name.unwrap_or_default(),
                meta.spawned_at.0.file(),
                meta.spawned_at.0.line(),
                meta.spawned_at.0.column(),
            );
        }

        #[inline]
        pub(crate) fn finish_task(id: Id, reason: TerminateKind) {
            usdt_impl::task_terminate(id.as_u64(), reason as u8);
        }

        /// Mark a task as polling in USDT traces
        pub(crate) struct PollGuard(Id);

        impl PollGuard {
            #[inline]
            pub(crate) fn new(id: Id) -> Self {
                usdt_impl::task_poll_start(id.as_u64());
                PollGuard(id)
            }
        }

        impl Drop for PollGuard {
            #[inline]
            fn drop(&mut self) {
                usdt_impl::task_poll_end(self.0.as_u64());
            }
        }

        #[inline]
        pub(crate) fn block_on<F>(task: F, meta: SpawnMeta<'_>, id: Id) -> BlockOn<F> {
            start_task(TaskKind::BlockOn, meta, id, std::mem::size_of::<F>());
            BlockOn { inner: task, task_id: id }
        }

        pin_project! {
            #[derive(Debug, Clone)]
            pub(crate) struct BlockOn<F> {
                #[pin]
                inner: F,
                task_id: Id,
            }
        }

        impl<F: Future> Future for BlockOn<F> {
            type Output = F::Output;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                pub(crate) struct Guard(Id);

                impl Drop for Guard {
                    fn drop(&mut self) {
                        usdt_impl::task_poll_end(self.0.as_u64());
                        finish_task(self.0, TerminateKind::Panicked);
                    }
                }

                let this = self.project();

                usdt_impl::task_poll_start(this.task_id.as_u64());
                let guard = Guard(*this.task_id);
                let res = this.inner.poll(cx);
                drop(guard);
                usdt_impl::task_poll_end(this.task_id.as_u64());

                if res.is_ready() {
                    finish_task(*this.task_id, TerminateKind::Success);
                }

                res
            }
        }
    }
}

cfg_not_usdt! {
    #[inline]
    pub(crate) fn trace_root() {}

    cfg_rt! {
        #[inline]
        pub(crate) fn start_task(_kind: TaskKind, _meta: SpawnMeta<'_>, _id: Id, _size: usize) {}

        #[inline]
        pub(crate) fn finish_task(_id: Id, _reason: TerminateKind) {}

        /// Mark a task as polling in USDT traces
        pub(crate) struct PollGuard();

        impl PollGuard {
            #[inline]
            pub(crate) fn new(_id: Id) -> Self {
                PollGuard()
            }
        }
    }
}
