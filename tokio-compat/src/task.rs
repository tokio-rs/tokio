use futures_01::executor::{Notify, NotifyHandle, UnsafeNotify};
use std::task::Waker;

#[derive(Clone)]
pub(crate) struct CompatWaker<T> {
    waker: T,
}

// === impl CompatWaker ===

impl From<&'a Waker> for CompatWaker<&'a Waker> {
    fn from(waker: &'a Waker) -> Self {
        Self { waker }
    }
}

impl From<Waker> for CompatWaker<Waker> {
    fn from(waker: Waker) -> Self {
        Self { waker }
    }
}

impl CompatWaker<Waker> {
    fn as_ref(&self) -> CompatWaker<&Waker> {
        CompatWaker { waker: &self.waker }
    }
}

impl Notify for CompatWaker<Waker> {
    fn notify(&self, _: usize) {
        self.waker.wake_by_ref();
    }
}

unsafe impl UnsafeNotify for CompatWaker<Waker> {
    unsafe fn clone_raw(&self) -> NotifyHandle {
        self.as_ref().into()
    }

    unsafe fn drop_raw(&self) {
        drop(Box::from_raw(self as *const _ as *mut dyn UnsafeNotify));
    }
}

impl<'a> Into<NotifyHandle> for CompatWaker<&'a Waker> {
    fn into(self) -> NotifyHandle {
        let notify = Box::new(CompatWaker {
            waker: self.waker.clone(),
        });
        unsafe { NotifyHandle::new(Box::into_raw(notify)) }
    }
}
