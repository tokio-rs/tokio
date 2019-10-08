use crate::loom::alloc::Track;
use crate::task::raw::RawTask;

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) struct JoinHandle<T, S: 'static> {
    raw: Option<RawTask<S>>,
    _p: PhantomData<T>,
}

impl<T, S: 'static> JoinHandle<T, S> {
    pub(super) fn new(raw: RawTask<S>) -> JoinHandle<T, S> {
        JoinHandle {
            raw: Some(raw),
            _p: PhantomData,
        }
    }
}

impl<T, S: 'static> Unpin for JoinHandle<T, S> {}

impl<T, S: 'static> Future for JoinHandle<T, S> {
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

impl<T, S: 'static> Drop for JoinHandle<T, S> {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            if raw.header().state.drop_join_handle_fast() {
                return;
            }

            raw.drop_join_handle_slow();
        }
    }
}
