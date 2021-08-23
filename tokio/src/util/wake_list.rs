use core::mem::MaybeUninit;
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

    #[inline]
    pub(crate) fn can_push(&self) -> bool {
        self.curr <= NUM_WAKERS - 1
    }

    pub(crate) fn push(&mut self, val: Waker) {
        debug_assert!(self.can_push());

        self.inner[self.curr] = MaybeUninit::new(val);
        self.curr += 1;
    }

    pub(crate) fn wake_all(&mut self) {
        assert!(self.curr <= NUM_WAKERS);
        while self.curr > 0 {
            self.curr -= 1;
            let waker = unsafe { std::ptr::read(self.inner[self.curr].as_mut_ptr()) };
            waker.wake();
        }
    }
}

impl Drop for WakeList {
    fn drop(&mut self) {
        let slice =
            std::ptr::slice_from_raw_parts_mut(self.inner.as_mut_ptr() as *mut Waker, self.curr);
        unsafe { std::ptr::drop_in_place(slice) };
    }
}
