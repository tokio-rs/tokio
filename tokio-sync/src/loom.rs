pub(crate) mod futures {
    pub use futures::task;
    pub use ::atomic_task::AtomicTask;
}

pub(crate) mod sync {
    pub(crate) use std::sync::atomic;

    use std::cell::UnsafeCell;

    pub struct CausalCell<T>(UnsafeCell<T>);

    impl<T> CausalCell<T> {
        pub fn new(data: T) -> CausalCell<T> {
            CausalCell(UnsafeCell::new(data))
        }

        pub fn with<F, R>(&self, f: F) -> R
        where
            F: FnOnce(*const T) -> R,
        {
            f(self.0.get())
        }

        pub fn with_mut<F, R>(&self, f: F) -> R
        where
            F: FnOnce(*mut T) -> R,
        {
            f(self.0.get())
        }
    }
}

pub fn yield_now() {}
