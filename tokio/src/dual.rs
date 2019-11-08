use std::{
    mem::{self, ManuallyDrop},
    ops::Deref,
    ptr,
    sync::atomic::{AtomicBool, Ordering::AcqRel},
};

pub(crate) unsafe trait DualDrop {
    type Inner;
    fn new(value: Self::Inner) -> Self;
    fn dual_drop(&self) -> bool;
    fn inner(&self) -> &Self::Inner;
}

unsafe impl<T> DualDrop for External<T> {
    type Inner = T;
    fn new(value: T) -> Self {
        External {
            value,
            dropped: AtomicBool::new(false),
        }
    }
    fn dual_drop(&self) -> bool {
        self.dropped.fetch_or(true, AcqRel)
    }

    fn inner(&self) -> &T {
        &self.value
    }
}

#[derive(Debug)]
pub(crate) struct External<T> {
    value: T,
    dropped: AtomicBool,
}

#[derive(Debug)]
pub(crate) struct Dual<T>(ManuallyDrop<Box<T>>)
where
    T: DualDrop;

pub(crate) type ExternalDual<T> = Dual<External<T>>;

impl<T> Drop for Dual<T>
where
    T: DualDrop,
{
    fn drop(&mut self) {
        // SAFETY Only the second instance to drop will see the `true` set by the first one to drop
        unsafe {
            if self.0.dual_drop() {
                ManuallyDrop::drop(&mut self.0);
            }
        }
    }
}

impl<T> Deref for Dual<T>
where
    T: DualDrop,
{
    type Target = T::Inner;
    fn deref(&self) -> &T::Inner {
        self.0.inner()
    }
}

impl<T> Dual<T>
where
    T: DualDrop,
{
    pub(crate) fn new(value: T::Inner) -> (Dual<T>, Dual<T>) {
        let inner = ManuallyDrop::new(Box::new(T::new(value)));

        // SAFETY Duplicating the `Box` reference is ok since ManuallyDrop::drop must be called to
        // drop it
        (Dual(unsafe { ptr::read(&inner) }), Dual(inner))
    }

    pub(crate) fn join(l: Dual<T>, r: Dual<T>) -> Result<T::Inner, (Dual<T>, Dual<T>)> {
        if ptr::eq::<T>(&**l.0, &**r.0) {
            // SAFETY `l` and `r` point to the same reference, so we can take ownership of the
            // inner `T` as `Dual::drop` is prevented from running with `mem::forget`
            unsafe {
                let value = ptr::read::<T::Inner>(&*l);
                mem::forget(l);
                mem::forget(r);
                Ok(value)
            }
        } else {
            Err((l, r))
        }
    }
}
