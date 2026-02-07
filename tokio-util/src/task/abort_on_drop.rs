//! An [`AbortOnDropHandle`] is like a [`JoinHandle`], except that it
//! will abort the task as soon as it is dropped.
//!
//! Correspondingly, an [`AbortOnDrop`] is like a [`AbortHandle`] that will abort
//! the task as soon as it is dropped.

use tokio::task::{AbortHandle, JoinError, JoinHandle};

use std::{
    future::Future,
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll},
};

/// A wrapper around a [`tokio::task::JoinHandle`],
/// which [aborts] the task when it is dropped.
///
/// [aborts]: tokio::task::JoinHandle::abort
#[must_use = "Dropping the handle aborts the task immediately"]
pub struct AbortOnDropHandle<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDropHandle<T> {
    fn drop(&mut self) {
        self.abort()
    }
}

impl<T> AbortOnDropHandle<T> {
    /// Create an [`AbortOnDropHandle`] from a [`JoinHandle`].
    pub fn new(handle: JoinHandle<T>) -> Self {
        Self(handle)
    }

    /// Abort the task associated with this handle,
    /// equivalent to [`JoinHandle::abort`].
    #[inline]
    pub fn abort(&self) {
        self.0.abort()
    }

    /// Checks if the task associated with this handle is finished,
    /// equivalent to [`JoinHandle::is_finished`].
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    /// Returns a new [`AbortHandle`] that can be used to remotely abort this task,
    /// equivalent to [`JoinHandle::abort_handle`].
    pub fn abort_handle(&self) -> AbortHandle {
        self.0.abort_handle()
    }

    /// Cancels aborting on drop and returns the original [`JoinHandle`].
    pub fn detach(self) -> JoinHandle<T> {
        // Avoid invoking `AbortOnDropHandle`'s `Drop` impl
        let this = ManuallyDrop::new(self);
        // SAFETY: `&this.0` is a reference, so it is certainly initialized, and
        // it won't be double-dropped because it's in a `ManuallyDrop`
        unsafe { std::ptr::read(&this.0) }
    }
}

impl<T> std::fmt::Debug for AbortOnDropHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortOnDropHandle")
            .field("id", &self.0.id())
            .finish()
    }
}

impl<T> Future for AbortOnDropHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> AsRef<JoinHandle<T>> for AbortOnDropHandle<T> {
    fn as_ref(&self) -> &JoinHandle<T> {
        &self.0
    }
}

/// A wrapper around a [`tokio::task::AbortHandle`],
/// which [aborts] the task when it is dropped.
///
/// Unlike [`AbortOnDropHandle`], [`AbortOnDrop`] cannot be `.await`ed for a result.
///
/// It has no generic parameter, making it suitable when you only need to keep
/// a task handle in a struct and do not care about the output.
///
/// [aborts]: tokio::task::AbortHandle::abort
#[must_use = "Dropping the handle aborts the task immediately"]
pub struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.abort()
    }
}

impl AbortOnDrop {
    /// Create an [`AbortOnDrop`] from a [`AbortHandle`].
    pub fn new(handle: AbortHandle) -> Self {
        Self(handle)
    }

    /// Abort the task associated with this handle,
    /// equivalent to [`AbortHandle::abort`].
    #[inline]
    pub fn abort(&self) {
        self.0.abort()
    }

    /// Checks if the task associated with this handle is finished,
    /// equivalent to [`AbortHandle::is_finished`].
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    /// Cancels aborting on drop and returns the original [`AbortHandle`].
    pub fn detach(self) -> AbortHandle {
        // Avoid invoking `AbortOnDrop`'s `Drop` impl
        let this = ManuallyDrop::new(self);
        // SAFETY: `&this.0` is a reference, so it is certainly initialized, and
        // it won't be double-dropped because it's in a `ManuallyDrop`
        unsafe { std::ptr::read(&this.0) }
    }
}

impl std::fmt::Debug for AbortOnDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortOnDrop")
            .field("id", &self.0.id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple type that does not implement [`std::fmt::Debug`].
    struct NotDebug;

    fn is_debug<T: std::fmt::Debug>() {}

    #[test]
    fn assert_debug() {
        is_debug::<AbortOnDrop>();
        is_debug::<AbortOnDropHandle<NotDebug>>();
    }
}
