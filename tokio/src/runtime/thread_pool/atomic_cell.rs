use crate::loom::sync::atomic::AtomicPtr;

use std::ptr;
use std::sync::atomic::Ordering::AcqRel;

pub(super) struct AtomicCell<T> {
    data: AtomicPtr<T>,
}

unsafe impl<T: Send> Send for AtomicCell<T> {}
unsafe impl<T: Send> Sync for AtomicCell<T> {}

impl<T> AtomicCell<T> {
    pub(super) fn new(data: Option<Box<T>>) -> AtomicCell<T> {
        AtomicCell {
            data: AtomicPtr::new(to_raw(data)),
        }
    }

    pub(super) fn swap(&self, val: Option<Box<T>>) -> Option<Box<T>> {
        let old = self.data.swap(to_raw(val), AcqRel);
        from_raw(old)
    }

    #[cfg(feature = "blocking")]
    pub(super) fn set(&self, val: Box<T>) {
        let _ = self.swap(Some(val));
    }

    pub(super) fn take(&self) -> Option<Box<T>> {
        self.swap(None)
    }
}

fn to_raw<T>(data: Option<Box<T>>) -> *mut T {
    data.map(Box::into_raw).unwrap_or(ptr::null_mut())
}

fn from_raw<T>(val: *mut T) -> Option<Box<T>> {
    if val.is_null() {
        None
    } else {
        Some(unsafe { Box::from_raw(val) })
    }
}

impl<T> Drop for AtomicCell<T> {
    fn drop(&mut self) {
        // Free any data still held by the cell
        let _ = self.take();
    }
}
