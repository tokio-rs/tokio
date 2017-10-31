//! Support for lazily initialized statics with destructors
//!
//! This module contains a macro to construct values of type `Statik` which are
//! lazily initialized statics that can contain arbitrary initialization
//! expressions and can also be destroyed at any time. A value of `Statik` will
//! be initialized on first use and any usage via the `with` function may fail
//! due to the static being destroyed.
//!
//! Statics are destroyed via the `drop` function which will deallocate all
//! resources associated with the static, and permanently prevent the static
//! from being accessed again. Note that `drop` is not synchronous, the actual
//! destruction may happen on another thread.
//!
//! # Examples
//!
//! ```ignore
//! statik!(static MY_VALUE: String = format!("Hello, {}", "world!"));
//!
//! // Accessing can fail if the value has been destroyed
//! match MY_VALUE.with(|s| s.clone()) {
//!     Some(s) => assert_eq!(s, "Hello, world!"),
//!     None => panic!("shouldn't be destroyed yet!"),
//! }
//!
//! // We'll get informed if we actually dropped the value when we call `drop`,
//! // and in this case we're the first so we'll get `true`.
//! assert!(MY_VALUE.drop());
//!
//! // Further attempts to access the static will all fail
//! assert!(MY_VALUE.with(|_| ()).is_none());
//! ```

use std::sync::atomic::{AtomicIsize, AtomicUsize, AtomicBool};
use std::sync::atomic::Ordering::SeqCst;

/// A type created by the `statik!` macro for a lazily initialized static which
/// can be destroyed.
pub struct Statik<T: Send + Sync + 'static> {
    #[doc(hidden)]
    pub __val: AtomicUsize,
    #[doc(hidden)]
    pub __lookers: AtomicIsize,
    #[doc(hidden)]
    pub __destroyed: AtomicBool,
    #[doc(hidden)]
    pub __mk: fn() -> T,
}

macro_rules! statik {
    (static $name:ident: $t:ty = $e:expr) => (
        static $name: $crate::statik::Statik<$t> = {
            fn __mk() -> $t { $e }

            $crate::statik::Statik {
                __val: ::std::sync::atomic::ATOMIC_USIZE_INIT,
                __lookers: ::std::sync::atomic::ATOMIC_ISIZE_INIT,
                __destroyed: ::std::sync::atomic::ATOMIC_BOOL_INIT,
                __mk: __mk,
            }
        };
    )
}

unsafe impl<T: Send + Sync + 'static> Send for Statik<T> {}
unsafe impl<T: Send + Sync + 'static> Sync for Statik<T> {}

impl<T: Send + Sync + 'static> Statik<T> {
    /// Attempts to access this static, running the closure provided if the
    /// static can be accessed.
    ///
    /// If this static value is accessed for the first time, it will be lazily
    /// initialized. Note that the initialization may run more than once in
    /// concurrent situations, but only one created value will be stored and all
    /// threads calling this function will only ever call the provided closure
    /// with one instance of `T`.
    ///
    /// Once initialized the closure `f` will be provided a reference to the
    /// type `T` and the value returned by the closure will be returned by this
    /// function.
    ///
    /// If the value in this static has been previously destroyed then the
    /// closure `f` will not be executed and `None` will be returned.
    pub fn with<F, R>(&'static self, f: F) -> Option<R>
        where F: FnOnce(&T) -> R
    {
        let mut lookers = self.__lookers.load(SeqCst);
        loop {
            if lookers < 0 {
                return None
            }
            match self.__lookers.compare_exchange(lookers, lookers + 1, SeqCst, SeqCst) {
                Ok(_) => break,
                Err(n) => lookers = n,
            }
        }

        struct MyDrop<T: Send + Sync + 'static>(&'static Statik<T>);

        impl<T: Send + Sync + 'static> Drop for MyDrop<T> {
            fn drop(&mut self) {
                self.0.unref();
            }
        }

        let _mydrop = MyDrop(self);

        unsafe {
            let mut val = self.__val.load(SeqCst);
            loop {
                match val {
                    0 => {}
                    n => return Some(f(&*(n as *mut T))),
                }
                let ptr = Box::into_raw(Box::new((self.__mk)()));
                match self.__val.compare_exchange(0, ptr as usize, SeqCst, SeqCst) {
                    Ok(_) => val = ptr as usize,
                    Err(e) => {
                        drop(Box::from_raw(ptr));
                        val = e;
                    }
                }
            }
        }
    }

    /// Flags this static value for destruction.
    ///
    /// This function may not immediately destroy the value in this static. The
    /// actual destruction may be deferred until concurrent calls to `with` all
    /// complete.
    ///
    /// This function returns `true` if it ran the destructor for the contained
    /// value, or `false` otherwise. Once this function is called and all
    /// concurrent threads have exited `with`, however, it's guaranteed that
    /// this value will be destroyed.
    #[allow(dead_code)]
    pub fn drop(&'static self) -> bool {
        if !self.__destroyed.swap(true, SeqCst) {
            self.unref()
        } else {
            false
        }
    }

    fn unref(&'static self) -> bool {
        match self.__lookers.fetch_sub(1, SeqCst) {
            0 => {}
            _ => return false,
        }

        unsafe {
            let val = self.__val.load(SeqCst);
            if val != 0 {
                drop(Box::from_raw(val as *mut T));
            }
            return true
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::*;
    use std::thread;

    #[test]
    fn smoke() {
        statik!(static A: usize = 3);

        assert_eq!(A.with(|x| *x), Some(3));
        assert!(A.drop());
        assert_eq!(A.with(|x| *x), None);
    }

    #[test]
    fn smoke2() {
        statik!(static A: usize = panic!());

        assert!(A.drop());
        assert_eq!(A.with(|x| *x), None);
    }

    #[test]
    fn drop_drops() {
        static mut HITS: usize = 0;

        struct B;

        impl Drop for B {
            fn drop(&mut self) {
                unsafe { HITS += 1; }
            }
        }

        statik!(static A: B = B);

        unsafe {
            assert_eq!(HITS, 0);
            A.with(|_| ());
            assert_eq!(HITS, 0);
            assert!(A.drop());
            assert_eq!(HITS, 1);
            assert!(A.with(|_| ()).is_none());
            assert_eq!(HITS, 1);
        }
    }

    #[test]
    fn many_threads() {
        static HIT: AtomicBool = ATOMIC_BOOL_INIT;

        const N: usize = 4;
        const M: usize = 1_000_000;

        struct B;

        impl Drop for B {
            fn drop(&mut self) {
                assert!(!HIT.swap(true, Ordering::SeqCst));
            }
        }

        statik!(static A: B = B);

        let threads = (0..N).map(|_| {
            thread::spawn(|| {
                for _ in 0..M {
                    A.with(|_| ());
                }
                A.drop();
                for _ in 0..M/2 {
                    assert!(A.with(|_| ()).is_none());
                }
            })
        });
        for thread in threads {
            thread.join().unwrap();
        }
        assert!(A.with(|_| ()).is_none());
        assert!(!A.drop());
        assert!(HIT.load(Ordering::SeqCst));
    }
}
