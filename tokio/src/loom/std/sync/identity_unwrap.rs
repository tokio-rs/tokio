use super::{Mutex, MutexGuard};

pub(crate) trait IdentityUnwrap<T> {
    /// An identify function, for compatibility with Mutex types that may be
    /// poisoned (like `std::sync::Mutex`), an are thus usually `unwrap`d to
    /// panic on poison.
    fn unwrap(self) -> T;
}

impl<T> IdentityUnwrap<Mutex<T>> for Mutex<T> {
    #[inline]
    fn unwrap(self) -> Mutex<T> {
        return self;
    }
}

impl<'a, T> IdentityUnwrap<MutexGuard<'a, T>> for MutexGuard<'a, T> {
    #[inline]
    fn unwrap(self) -> MutexGuard<'a, T> {
        return self;
    }
}
