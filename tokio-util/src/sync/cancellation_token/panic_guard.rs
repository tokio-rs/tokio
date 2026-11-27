use crate::sync::CancellationToken;

/// A wrapper for a cancellation token which cancels it when dropped while the
/// current thread is unwinding from a panic.
///
/// It is created using [`CancellationToken::panic_guard`]. Unlike [`DropGuard`],
/// dropping this guard normally, including when returning an error or aborting
/// a task, does not cancel the token. A panic caught before the guard is dropped
/// does not trigger cancellation either.
///
/// When using `panic = "abort"`, destructors do not run on panic, so this guard
/// cannot request cancellation.
///
/// [`DropGuard`]: super::DropGuard
#[derive(Debug)]
pub struct PanicGuard {
    pub(super) inner: Option<CancellationToken>,
}

impl PanicGuard {
    /// Returns a reference to the cancellation token wrapped by this guard.
    pub fn token(&self) -> &CancellationToken {
        self.inner
            .as_ref()
            .expect("`inner` can only be None in a destructor")
    }

    /// Returns the stored cancellation token and removes this guard without
    /// cancelling the token. Other guards for this token are not affected.
    pub fn disarm(mut self) -> CancellationToken {
        self.inner
            .take()
            .expect("`inner` can only be None in a destructor")
    }
}

impl Drop for PanicGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(inner) = &self.inner {
                inner.cancel();
            }
        }
    }
}
