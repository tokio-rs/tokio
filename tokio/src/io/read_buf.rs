// This lint claims ugly casting is somehow safer than transmute, but there's
// no evidence that is the case. Shush.
#![allow(clippy::transmute_ptr_to_ptr)]

use std::fmt;
use std::mem::{self, MaybeUninit};

/// A wrapper around a byte buffer that is incrementally filled and initialized.
///
/// This type is a sort of "double cursor". It tracks three regions in the
/// buffer: a region at the beginning of the buffer that has been logically
/// filled with data, a region that has been initialized at some point but not
/// yet logically filled, and a region at the end that is fully uninitialized.
/// The filled region is guaranteed to be a  subset of the initialized region.
///
/// In summary, the contents of the buffer can be visualized as:
///
/// ```not_rust
/// [             capacity              ]
/// [ filled |         unfilled         ]
/// [    initialized    | uninitialized ]
/// ```
pub struct ReadBuf<'a> {
    buf: &'a mut [MaybeUninit<u8>],
    filled: usize,
    initialized: usize,
}

impl<'a> ReadBuf<'a> {
    /// Creates a new `ReadBuf` from a fully initialized buffer.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> ReadBuf<'a> {
        let initialized = buf.len();
        let buf = unsafe { mem::transmute::<&mut [u8], &mut [MaybeUninit<u8>]>(buf) };
        ReadBuf {
            buf,
            filled: 0,
            initialized,
        }
    }

    /// Creates a new `ReadBuf` from a fully uninitialized buffer.
    ///
    /// Use `assume_init` if part of the buffer is known to be already inintialized.
    #[inline]
    pub fn uninit(buf: &'a mut [MaybeUninit<u8>]) -> ReadBuf<'a> {
        ReadBuf {
            buf,
            filled: 0,
            initialized: 0,
        }
    }

    /// Returns the total capacity of the buffer.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Returns a shared reference to the filled portion of the buffer.
    #[inline]
    pub fn filled(&self) -> &[u8] {
        let slice = &self.buf[..self.filled];
        // safety: filled describes how far into the buffer that the
        // user has filled with bytes, so it's been initialized.
        // TODO: This could use `MaybeUninit::slice_get_ref` when it is stable.
        unsafe { mem::transmute::<&[MaybeUninit<u8>], &[u8]>(slice) }
    }

    /// Returns a mutable reference to the filled portion of the buffer.
    #[inline]
    pub fn filled_mut(&mut self) -> &mut [u8] {
        let slice = &mut self.buf[..self.filled];
        // safety: filled describes how far into the buffer that the
        // user has filled with bytes, so it's been initialized.
        // TODO: This could use `MaybeUninit::slice_get_mut` when it is stable.
        unsafe { mem::transmute::<&mut [MaybeUninit<u8>], &mut [u8]>(slice) }
    }

    /// Returns a new `ReadBuf` comprised of the unfilled section up to `n`.
    #[inline]
    pub fn take(&mut self, n: usize) -> ReadBuf<'_> {
        let max = std::cmp::min(self.remaining(), n);
        // Saftey: We don't set any of the `unfilled_mut` with `MaybeUninit::uninit`.
        unsafe { ReadBuf::uninit(&mut self.unfilled_mut()[..max]) }
    }

    /// Returns a shared reference to the initialized portion of the buffer.
    ///
    /// This includes the filled portion.
    #[inline]
    pub fn initialized(&self) -> &[u8] {
        let slice = &self.buf[..self.initialized];
        // safety: initialized describes how far into the buffer that the
        // user has at some point initialized with bytes.
        // TODO: This could use `MaybeUninit::slice_get_ref` when it is stable.
        unsafe { mem::transmute::<&[MaybeUninit<u8>], &[u8]>(slice) }
    }

    /// Returns a mutable reference to the initialized portion of the buffer.
    ///
    /// This includes the filled portion.
    #[inline]
    pub fn initialized_mut(&mut self) -> &mut [u8] {
        let slice = &mut self.buf[..self.initialized];
        // safety: initialized describes how far into the buffer that the
        // user has at some point initialized with bytes.
        // TODO: This could use `MaybeUninit::slice_get_mut` when it is stable.
        unsafe { mem::transmute::<&mut [MaybeUninit<u8>], &mut [u8]>(slice) }
    }

    /// Returns a mutable reference to the unfilled part of the buffer without ensuring that it has been fully
    /// initialized.
    ///
    /// # Safety
    ///
    /// The caller must not de-initialize portions of the buffer that have already been initialized.
    #[inline]
    pub unsafe fn unfilled_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.buf[self.filled..]
    }

    /// Returns a mutable reference to the unfilled part of the buffer, ensuring it is fully initialized.
    ///
    /// Since `ReadBuf` tracks the region of the buffer that has been initialized, this is effectively "free" after
    /// the first use.
    #[inline]
    pub fn initialize_unfilled(&mut self) -> &mut [u8] {
        self.initialize_unfilled_to(self.remaining())
    }

    /// Returns a mutable reference to the first `n` bytes of the unfilled part of the buffer, ensuring it is
    /// fully initialized.
    ///
    /// # Panics
    ///
    /// Panics if `self.remaining()` is less than `n`.
    #[inline]
    pub fn initialize_unfilled_to(&mut self, n: usize) -> &mut [u8] {
        assert!(self.remaining() >= n, "n overflows remaining");

        // This can't overflow, otherwise the assert above would have failed.
        let end = self.filled + n;

        if self.initialized < end {
            unsafe {
                self.buf[self.initialized..end]
                    .as_mut_ptr()
                    .write_bytes(0, end - self.initialized);
            }
            self.initialized = end;
        }

        let slice = &mut self.buf[self.filled..end];
        // safety: just above, we checked that the end of the buf has
        // been initialized to some value.
        unsafe { mem::transmute::<&mut [MaybeUninit<u8>], &mut [u8]>(slice) }
    }

    /// Returns the number of bytes at the end of the slice that have not yet been filled.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.capacity() - self.filled
    }

    /// Clears the buffer, resetting the filled region to empty.
    ///
    /// The number of initialized bytes is not changed, and the contents of the buffer are not modified.
    #[inline]
    pub fn clear(&mut self) {
        self.filled = 0;
    }

    /// Advances the size of the filled region of the buffer.
    ///
    /// The number of initialized bytes is not changed.
    ///
    /// # Panics
    ///
    /// Panics if the filled region of the buffer would become larger than the initialized region.
    #[inline]
    pub fn advance(&mut self, n: usize) {
        let new = self.filled.checked_add(n).expect("filled overflow");
        self.set_filled(new);
    }

    /// Sets the size of the filled region of the buffer.
    ///
    /// The number of initialized bytes is not changed.
    ///
    /// Note that this can be used to *shrink* the filled region of the buffer in addition to growing it (for
    /// example, by a `AsyncRead` implementation that compresses data in-place).
    ///
    /// # Panics
    ///
    /// Panics if the filled region of the buffer would become larger than the intialized region.
    #[inline]
    pub fn set_filled(&mut self, n: usize) {
        assert!(
            n <= self.initialized,
            "filled must not become larger than initialized"
        );
        self.filled = n;
    }

    /// Asserts that the first `n` unfilled bytes of the buffer are initialized.
    ///
    /// `ReadBuf` assumes that bytes are never de-initialized, so this method does nothing when called with fewer
    /// bytes than are already known to be initialized.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `n` unfilled bytes of the buffer have already been initialized.
    #[inline]
    pub unsafe fn assume_init(&mut self, n: usize) {
        let new = self.filled + n;
        if new > self.initialized {
            self.initialized = new;
        }
    }

    /// Appends data to the buffer, advancing the written position and possibly also the initialized position.
    ///
    /// # Panics
    ///
    /// Panics if `self.remaining()` is less than `buf.len()`.
    #[inline]
    pub fn put_slice(&mut self, buf: &[u8]) {
        assert!(
            self.remaining() >= buf.len(),
            "buf.len() must fit in remaining()"
        );

        let amt = buf.len();
        // Cannot overflow, asserted above
        let end = self.filled + amt;

        // Safety: the length is asserted above
        unsafe {
            self.buf[self.filled..end]
                .as_mut_ptr()
                .cast::<u8>()
                .copy_from_nonoverlapping(buf.as_ptr(), amt);
        }

        if self.initialized < end {
            self.initialized = end;
        }
        self.filled = end;
    }
}

impl fmt::Debug for ReadBuf<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadBuf")
            .field("filled", &self.filled)
            .field("initialized", &self.initialized)
            .field("capacity", &self.capacity())
            .finish()
    }
}
