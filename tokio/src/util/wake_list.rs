use core::mem::{self, MaybeUninit};
use std::task::Waker;

const NUM_WAKERS: usize = 32;

pub(crate) struct WakeList {
    inner: [MaybeUninit<Waker>; NUM_WAKERS],
    curr: usize,
}

impl WakeList {
    pub(crate) fn new() -> Self {
        Self {
            inner: unsafe { MaybeUninit::uninit().assume_init() },
            curr: 0,
        }
    }

    pub(crate) fn can_push(&self) -> bool {
        self.curr < NUM_WAKERS - 1
    }

    pub(crate) fn push(&mut self, val: Waker) {
        debug_assert!(self.can_push());

        self.inner[self.curr] = MaybeUninit::new(val);
        self.curr += 1;
    }

    pub(crate) fn wake_all(&mut self) {
        for waker in &mut self.inner[..self.curr] {
            unsafe { mem::replace(waker, MaybeUninit::uninit()).assume_init() }.wake()
        }
        self.curr = 0;
    }
}

impl Drop for WakeList {
    fn drop(&mut self) {
        for waker in &mut self.inner[..self.curr] {
            mem::drop(unsafe { mem::replace(waker, MaybeUninit::uninit()).assume_init() });
        }
    }
}
