//! Extension traits for compatibility between Mutex (or other concurrency
//! primative) providers such as _parking_lot_ vs _libstd_.

#[cfg(feature = "parking_lot")]
use super::MutexGuard;

pub(crate) trait IdentityUnwrap<T> {
    /// An identify function, for compatibility with Mutex types that may be
    /// poisoned (like `std::sync::Mutex`), and are thus usually `unwrap`d to
    /// panic on poison. In this case they never panic.
    #[cfg(feature = "parking_lot")]
    fn unwrap(self) -> T;
}

#[cfg(feature = "parking_lot")]
impl<'a, T> IdentityUnwrap<MutexGuard<'a, T>> for MutexGuard<'a, T> {
    #[inline]
    fn unwrap(self) -> MutexGuard<'a, T> {
        return self;
    }
}
