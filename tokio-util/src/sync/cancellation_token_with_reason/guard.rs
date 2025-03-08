use crate::sync::CancellationTokenWithReason;

/// A wrapper for cancellation token which automatically cancels
/// it on drop. It is created using `drop_guard` method on the `CancellationTokenWithReason`.
#[derive(Debug)]
pub struct DropGuardWithReason<T: Clone> {
    pub(super) inner: Option<(CancellationTokenWithReason<T>, T)>,
}

impl<T: Clone> DropGuardWithReason<T> {
    /// Returns stored cancellation token and removes this drop guard instance
    /// (i.e. it will no longer cancel token). Other guards for this token
    /// are not affected.
    pub fn disarm(mut self) -> CancellationTokenWithReason<T> {
        self.inner
            .take()
            .expect("`inner` can be only None in a destructor")
            .0
    }
}

impl<T: Clone> Drop for DropGuardWithReason<T> {
    fn drop(&mut self) {
        if let Some((inner, reason)) = self.inner.take() {
            inner.cancel(reason);
        }
    }
}
