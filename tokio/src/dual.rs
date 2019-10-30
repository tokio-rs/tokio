use std::{
    mem::{self, ManuallyDrop},
    ops::Deref,
    ptr,
    sync::atomic::{AtomicBool, Ordering::AcqRel},
};

#[derive(Debug)]
struct Inner<T> {
    value: T,
    dropped: AtomicBool,
}

#[derive(Debug)]
pub(crate) struct Dual<T>(ManuallyDrop<Box<Inner<T>>>);

impl<T> Drop for Dual<T> {
    fn drop(&mut self) {
        // SAFETY Only the second instance to drop will see the `true` set by the first one to drop
        unsafe {
            if self.0.dropped.fetch_or(true, AcqRel) {
                ManuallyDrop::drop(&mut self.0);
            }
        }
    }
}

impl<T> Deref for Dual<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0.value
    }
}

impl<T> Dual<T> {
    pub(crate) fn new(value: T) -> (Dual<T>, Dual<T>) {
        let inner = ManuallyDrop::new(Box::new(Inner {
            value,
            dropped: AtomicBool::new(false),
        }));

        // SAFETY Duplicating the `Box` reference is ok since ManuallyDrop::drop must be called to
        // drop it
        (Dual(unsafe { ptr::read(&inner) }), Dual(inner))
    }

    pub(crate) fn join(l: Dual<T>, r: Dual<T>) -> Result<T, (Dual<T>, Dual<T>)> {
        if ptr::eq::<T>(&*l, &*r) {
            // SAFETY `l` and `r` point to the same reference, so we can take ownership of the
            // inner `T` as `Dual::drop` is prevented from running with `mem::forget`
            unsafe {
                let value = ptr::read(&l.0.value);
                mem::forget(l);
                mem::forget(r);
                Ok(value)
            }
        } else {
            Err((l, r))
        }
    }
}
