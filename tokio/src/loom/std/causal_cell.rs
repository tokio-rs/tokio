use std::cell::UnsafeCell;

#[derive(Debug)]
pub(crate) struct CausalCell<T>(UnsafeCell<T>);

#[derive(Default)]
pub(crate) struct CausalCheck(());

impl<T> CausalCell<T> {
    pub(crate) fn new(data: T) -> CausalCell<T> {
        CausalCell(UnsafeCell::new(data))
    }

    pub(crate) fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*const T) -> R,
    {
        f(self.0.get())
    }

    pub(crate) fn with_deferred<F, R>(&self, f: F) -> (R, CausalCheck)
    where
        F: FnOnce(*const T) -> R,
    {
        (f(self.0.get()), CausalCheck::default())
    }

    pub(crate) fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(*mut T) -> R,
    {
        f(self.0.get())
    }
}

impl CausalCheck {
    pub(crate) fn check(self) {}

    pub(crate) fn join(&mut self, _other: CausalCheck) {}
}
