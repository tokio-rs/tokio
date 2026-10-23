use std::mem::MaybeUninit;
use std::task::Waker;
use std::{ptr, slice};

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

impl WakeList {
    pub(crate) fn new() -> Self {
        Self {
            inner: [const { MaybeUninit::uninit() }; NUM_WAKERS],
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

    fn take(&mut self) -> &mut [Waker] {
        // SAFETY: self.inner[..curr] are initialized.
        unsafe {
            let filled = slice::from_raw_parts_mut(self.inner.as_mut_ptr(), self.curr);
            self.curr = 0;
            // TODO(MSRV 1.93): Switch to assume_init_mut.
            &mut *(filled as *mut [_] as *mut [Waker])
        }
    }

    pub(crate) fn wake_all(&mut self) {
        struct DropGuard<'a> {
            iter: slice::IterMut<'a, Waker>,
        }

        impl Drop for DropGuard<'_> {
            fn drop(&mut self) {
                // TODO(MSRV): Switch to as_mut_slice.
                for waker in &mut self.iter {
                    // SAFETY: We own the remaining element.
                    unsafe { ptr::drop_in_place(waker) };
                }
            }
        }

        debug_assert!(self.curr <= NUM_WAKERS);

        // TODO(MSRV 1.100): Switch to std::mem::DropGuard.
        let mut guard = DropGuard {
            iter: self.take().iter_mut(),
        };
        // SAFETY: We own the taken element.
        for waker in (&mut guard.iter).map(|waker| unsafe { ptr::read(waker) }) {
            // If this panics, then `guard` will clean up the remaining wakers.
            waker.wake();
        }
    }
}

impl Drop for WakeList {
    fn drop(&mut self) {
        // SAFETY: We own the taken elements.
        unsafe { ptr::drop_in_place(self.take()) };
    }
}
