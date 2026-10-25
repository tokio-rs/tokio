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

    pub(crate) fn wake_all(&mut self) {
        // TODO(MSRV 1.100): Switch to std::mem::DropGuard.
        struct DropGuard<'a>(slice::IterMut<'a, Waker>);

        impl Drop for DropGuard<'_> {
            fn drop(&mut self) {
                // TODO(MSRV): Drop via as_mut_slice.
                for waker in &mut self.0 {
                    // If this panics, the program is aborted.
                    // SAFETY: We own the remaining element.
                    unsafe { ptr::drop_in_place(waker) };
                }
            }
        }

        debug_assert!(self.curr <= NUM_WAKERS);

        let mut guard = {
            // SAFETY: The first `curr` elements are initialized.
            let filled = unsafe { self.inner.get_unchecked_mut(..self.curr) };
            // TODO(MSRV 1.93): Switch to assume_init_mut.
            let filled = unsafe { &mut *(filled as *mut [MaybeUninit<Waker>] as *mut [Waker]) };
            // Transfer ownership of the wakers in `inner` to `DropGuard`.
            self.curr = 0;
            DropGuard(filled.iter_mut())
        };
        // SAFETY: We own the element.
        for waker in (&mut guard.0).map(|waker| unsafe { ptr::read(waker) }) {
            // If this panics, then `guard` will clean up the remaining wakers.
            waker.wake();
        }
    }
}

impl Drop for WakeList {
    fn drop(&mut self) {
        // SAFETY: The first `curr` elements are initialized, so we can drop them.
        unsafe {
            let filled = self.inner.get_unchecked_mut(..self.curr);
            // TODO(MSRV 1.93): Switch to assume_init_drop.
            ptr::drop_in_place(filled as *mut [MaybeUninit<Waker>] as *mut [Waker]);
        }
    }
}
