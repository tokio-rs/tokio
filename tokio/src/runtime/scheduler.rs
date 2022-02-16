use std::marker::PhantomData;
use std::mem::ManuallyDrop;

use crate::future::Future;
use crate::loom::sync::Arc;

/// Marker trait for runtime scheduler handle.
pub(crate) trait SchedulerHandle {}

type BasicSchedulerHandle = Arc<super::basic_scheduler::Shared>;
impl SchedulerHandle for BasicSchedulerHandle {}

#[cfg(feature = "rt-multi-thread")]
type ThreadPoolHandle = Arc<super::thread_pool::Shared>;
#[cfg(feature = "rt-multi-thread")]
impl SchedulerHandle for ThreadPoolHandle {}

/// Scheduler handle wrapper which may contains a basic scheduler handle or
/// thread pool scheduler handle.
///
/// This ensures that the task cell layout is the same, no matter which scheduler
/// the runtime uses. This way, we can create a `UninitTask` and send the future to
/// the task cell immediately when spawning tasks.
#[repr(C)]
pub(crate) union Scheduler<S = ()> {
    basic: ManuallyDrop<BasicSchedulerHandle>,
    #[cfg(feature = "rt-multi-thread")]
    multi: ManuallyDrop<ThreadPoolHandle>,
    _marker: PhantomData<S>,
}

impl<F: Future> super::task::UninitTask<F, Scheduler<()>> {
    /// Transmute type after the scheduler type is determined.
    pub(crate) fn transmute<S: SchedulerHandle>(self) -> super::task::UninitTask<F, Scheduler<S>> {
        unsafe { std::mem::transmute(self) }
    }
}

impl Scheduler<BasicSchedulerHandle> {
    pub(crate) fn new(basic: BasicSchedulerHandle) -> Self {
        Self {
            basic: ManuallyDrop::new(basic),
        }
    }
}

impl AsRef<BasicSchedulerHandle> for Scheduler<BasicSchedulerHandle> {
    fn as_ref(&self) -> &BasicSchedulerHandle {
        unsafe { &*self.basic }
    }
}

#[cfg(feature = "rt-multi-thread")]
impl Scheduler<ThreadPoolHandle> {
    pub(crate) fn new(multi: ThreadPoolHandle) -> Self {
        Self {
            multi: ManuallyDrop::new(multi),
        }
    }
}

#[cfg(feature = "rt-multi-thread")]
impl AsRef<ThreadPoolHandle> for Scheduler<ThreadPoolHandle> {
    fn as_ref(&self) -> &ThreadPoolHandle {
        unsafe { &*self.multi }
    }
}

impl<S> Drop for Scheduler<S> {
    fn drop(&mut self) {
        unsafe {
            std::ptr::drop_in_place(self as *mut _ as *mut S);
        }
    }
}
