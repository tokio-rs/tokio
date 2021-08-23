use core::mem::MaybeUninit;
use core::ptr;
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
        self.curr < NUM_WAKERS
    }

    pub(crate) fn push(&mut self, val: Waker) {
        debug_assert!(self.can_push());

        self.inner[self.curr] = MaybeUninit::new(val);
        self.curr += 1;
    }

    pub(crate) fn wake_all(&mut self) {
        debug_assert!(self.curr <= NUM_WAKERS);

        for waker in self.inner[..self.curr].iter_mut().rev() {
            let waker = unsafe { std::ptr::read(waker.as_mut_ptr()) };
            waker.wake();
        }
        self.curr = 0;
    }
}

impl Drop for WakeList {
    fn drop(&mut self) {
        let slice =
            ptr::slice_from_raw_parts_mut(self.inner.as_mut_ptr() as *mut Waker, self.curr);
        unsafe { ptr::drop_in_place(slice) };
    }
}
