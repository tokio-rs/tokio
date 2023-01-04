#![allow(dead_code)]
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Once;

pub(crate) struct OnceCell<T> {
    once: Once,
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send + Sync> Send for OnceCell<T> {}
unsafe impl<T: Send + Sync> Sync for OnceCell<T> {}

impl<T> OnceCell<T> {
    pub(crate) const fn new() -> Self {
        Self {
            once: Once::new(),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Get the value inside this cell, intiailizing it using the provided
    /// function if necessary.
    ///
    /// If the `init` closure panics, then the `OnceCell` is poisoned and all
    /// future calls to `get` will panic.
    #[inline]
    pub(crate) fn get(&self, init: impl FnOnce() -> T) -> &T {
        if !self.once.is_completed() {
            self.do_init(init);
        }

        // Safety: The `std::sync::Once` guarantees that we can only reach this
        // line if a `call_once` closure has been run exactly once and without
        // panicking. Thus, the value is not uninitialized.
        //
        // There is also no race because the only `&self` method that modifies
        // `value` is `do_init`, but if the `call_once` closure is still
        // running, then no thread has gotten past the `call_once`.
        unsafe { &*(self.value.get() as *const T) }
    }

    #[cold]
    fn do_init(&self, init: impl FnOnce() -> T) {
        let value_ptr = self.value.get() as *mut T;

        self.once.call_once(|| {
            let set_to = init();

            // Safety: The `std::sync::Once` guarantees that this initialization
            // will run at most once, and that no thread can get past the
            // `call_once` until it has run exactly once. Thus, we have
            // exclusive access to `value`.
            unsafe {
                std::ptr::write(value_ptr, set_to);
            }
        });
    }
}

impl<T> Drop for OnceCell<T> {
    fn drop(&mut self) {
        if self.once.is_completed() {
            let value_ptr = self.value.get() as *mut T;
            unsafe {
                std::ptr::drop_in_place(value_ptr);
            }
        }
    }
}
