/// Simplified version of https://github.com/Manishearth/array-init for an infallible
/// initializer
use std::mem::MaybeUninit;

#[inline]
pub(crate) fn array_init<F, T, const N: usize>(mut initializer: F) -> [T; N]
where
    F: FnMut(usize) -> T,
{
    let mut array: MaybeUninit<[T; N]> = MaybeUninit::uninit();
    // pointer to array = *mut [T; N] <-> *mut T = pointer to first element
    let mut ptr_i = array.as_mut_ptr() as *mut T;
    for i in 0..N {
        let value_i = initializer(i);
        // SAFETY: We are initialising the array entry by entry
        unsafe {
            ptr_i.write(value_i);
            ptr_i = ptr_i.add(1);
        }
    }
    // SAFETY: We have finished initialising the array.
    unsafe { array.assume_init() }
}
