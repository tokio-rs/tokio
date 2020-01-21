use super::MutexGuard;

/// Extension trait for `MutexGuard` types (libstd or parking_lot), enabling
/// consistent handling of Mutex posioning.
pub(crate) trait ExpectPoison<T> {
    /// An identify function, for compatibility with Mutex types that may be
    /// poisoned (like `std::sync::Mutex`), and are thus usually `unwrap`d to
    /// panic on poison. In this case they never panic.
    fn expect_poison(self) -> T;
}

#[cfg(feature = "parking_lot")]
impl<'a, T> ExpectPoison<MutexGuard<'a, T>> for MutexGuard<'a, T> {
    #[inline]
    fn expect_poison(self) -> MutexGuard<'a, T> {
        self
    }
}

#[cfg(not(feature = "parking_lot"))]
impl<'a, T> ExpectPoison<MutexGuard<'a, T>> for std::sync::LockResult<MutexGuard<'a, T>> {
    #[inline]
    fn expect_poison(self) -> MutexGuard<'a, T> {
        self.unwrap()
    }
}
