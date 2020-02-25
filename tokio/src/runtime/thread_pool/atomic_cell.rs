use crate::loom::sync::atomic::AtomicUsize;

use std::marker::PhantomData;
use std::sync::atomic::Ordering::AcqRel;

pub(super) struct AtomicCell<T> {
    data: AtomicUsize,
    _p: PhantomData<Option<Box<T>>>,
}

unsafe impl<T: Send> Send for AtomicCell<T> {}
unsafe impl<T: Send> Sync for AtomicCell<T> {}

impl<T> AtomicCell<T> {
    pub(super) fn new(data: Option<Box<T>>) -> AtomicCell<T> {
        AtomicCell {
            data: AtomicUsize::new(to_usize(data)),
            _p: PhantomData,
        }
    }

    pub(super) fn swap(&self, val: Option<Box<T>>) -> Option<Box<T>> {
        let old = self.data.swap(to_usize(val), AcqRel);
        from_usize(old)
    }

    pub(super) fn set(&self, val: Box<T>) {
        let _ = self.swap(Some(val));
    }

    pub(super) fn take(&self) -> Option<Box<T>> {
        self.swap(None)
    }
}

fn to_usize<T>(data: Option<Box<T>>) -> usize {
    data.map(|boxed| Box::into_raw(boxed) as usize).unwrap_or(0)
}

fn from_usize<T>(val: usize) -> Option<Box<T>> {
    if val == 0 {
        None
    } else {
        Some(unsafe { Box::from_raw(val as *mut T) })
    }
}

impl<T> Drop for AtomicCell<T> {
    fn drop(&mut self) {
        // Free any data still held by the cell
        let _ = self.take();
    }
}
