use super::MutexGuard;

/// Extension trait for `MutexGuard` related types (libstd, parking_lot, etc.),
/// providing consistent handling of Mutex poisoning.
pub(crate) trait ExpectPoison<T> {
    /// Return the underlying `MutexGuard` or panic if the associated `Mutex`
    /// was poisoned.
    ///
    /// Poisoning is only possible with some implementations. This provides
    /// consistent access by replacing use of `lock().unwrap()` with
    /// `lock().expect_poison()` (for all `Mutex` types).
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
