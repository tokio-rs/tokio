use crate::executor::task::raw::RawTask;
use crate::loom::alloc::Track;

use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// An owned permission to join on a task (await its termination).
pub struct JoinHandle<T> {
    raw: Option<RawTask>,
    _p: PhantomData<T>,
}

impl<T> JoinHandle<T> {
    pub(super) fn new(raw: RawTask) -> JoinHandle<T> {
        JoinHandle {
            raw: Some(raw),
            _p: PhantomData,
        }
    }
}

impl<T> Unpin for JoinHandle<T> {}

impl<T> Future for JoinHandle<T> {
    type Output = super::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem::MaybeUninit;

        // Raw should always be set
        let raw = self.raw.as_ref().unwrap();

        // Load the current task state
        let mut state = raw.header().state.load();

        debug_assert!(state.is_join_interested());

        if state.is_active() {
            state = if state.has_join_waker() {
                raw.swap_join_waker(cx.waker(), state)
            } else {
                raw.store_join_waker(cx.waker())
            };

            if state.is_active() {
                return Poll::Pending;
            }
        }

        let mut out = MaybeUninit::<Track<Self::Output>>::uninit();

        unsafe {
            // This could result in the task being freed.
            raw.read_output(out.as_mut_ptr() as *mut (), state);

            self.raw = None;

            Poll::Ready(out.assume_init().into_inner())
        }
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            if raw.header().state.drop_join_handle_fast() {
                return;
            }

            raw.drop_join_handle_slow();
        }
    }
}

impl<T> fmt::Debug for JoinHandle<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("JoinHandle").finish()
    }
}
