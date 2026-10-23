use core::mem::{self, MaybeUninit};
use core::{ptr, slice};
use std::task::Waker;

const NUM_WAKERS: usize = 32;

/// A list of wakers to be woken.
///
/// # Invariants
///
/// The first `curr` elements of `inner` are initialized.
pub(crate) struct WakeList {
    inner: [MaybeUninit<Waker>; NUM_WAKERS],
    curr: usize,
}

struct Drain<'a> {
    inner: slice::IterMut<'a, MaybeUninit<Waker>>,
}

impl Iterator for Drain<'_> {
    type Item = Waker;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|waker| {
            // SAFETY: `Drain` only iterates over initialized wakers.
            unsafe { waker.assume_init_read() }
        })
    }
}

impl Drop for Drain<'_> {
    fn drop(&mut self) {
        let len = self.inner.len();
        let ptr = self.inner.as_slice().as_ptr().cast::<Waker>().cast_mut();

        // SAFETY: All remaining wakers in the iterator are initialized.
        unsafe { ptr::drop_in_place(ptr::slice_from_raw_parts_mut(ptr, len)) };
    }
}

impl WakeList {
    pub(crate) fn new() -> Self {
        const UNINIT_WAKER: MaybeUninit<Waker> = MaybeUninit::uninit();

        Self {
            inner: [UNINIT_WAKER; NUM_WAKERS],
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

    fn drain(&mut self) -> Drain<'_> {
        debug_assert!(self.curr <= NUM_WAKERS);

        let curr = mem::take(&mut self.curr);

        Drain {
            inner: self.inner[..curr].iter_mut(),
        }
    }

    pub(crate) fn wake_all(&mut self) {
        for waker in self.drain() {
            // If this panics, then `Drain` will drop the remaining wakers.
            waker.wake();
        }
    }
}

impl Drop for WakeList {
    fn drop(&mut self) {
        let slice =
            ptr::slice_from_raw_parts_mut(self.inner.as_mut_ptr().cast::<Waker>(), self.curr);
        // SAFETY: The first `curr` elements are initialized, so we can drop them.
        unsafe { ptr::drop_in_place(slice) };
    }
}
