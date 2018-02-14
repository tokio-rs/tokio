//! A (mostly) lock-free concurrent work-stealing deque
//!
//! This module contains an implementation of the Chase-Lev work stealing deque
//! described in "Dynamic Circular Work-Stealing Deque". The implementation is
//! heavily based on the implementation using C11 atomics in "Correct and
//! Efficient Work Stealing for Weak Memory Models".
//!
//! The only potentially lock-synchronized portion of this deque is the
//! occasional call to the memory allocator when growing the deque. Otherwise
//! all operations are lock-free.

pub use self::Poll::*;

use std::mem::forget;
use std::ptr;

use std::sync::atomic::{AtomicIsize, AtomicPtr, fence};
use std::sync::atomic::Ordering::{SeqCst, Acquire, Release, Relaxed};

// Initial size for a buffer.
// TODO: Much too small...
static MIN_SIZE: usize = 32;

#[derive(Debug)]
pub struct Deque<T: Send> {
    bottom: AtomicIsize,
    top: AtomicIsize,
    array: AtomicPtr<Buffer<T>>,
}

/// When stealing some data, this is an enumeration of the possible outcomes.
#[derive(Debug, PartialEq)]
pub enum Poll<T> {
    /// The deque was empty at the time of stealing
    Empty,
    /// The stealer lost the race for stealing data, and a retry may return more
    /// data.
    Inconsistent,
    /// The stealer has successfully stolen some data.
    Data(T),
}

/// An internal buffer used by the chase-lev deque. This structure is actually
/// implemented as a circular buffer, and is used as the intermediate storage of
/// the data in the deque.
///
/// This type is implemented with *T instead of Vec<T> for two reasons:
///
///   1. There is nothing safe about using this buffer. This easily allows the
///      same value to be read twice in to rust, and there is nothing to
///      prevent this. The usage by the deque must ensure that one of the
///      values is forgotten. Furthermore, we only ever want to manually run
///      destructors for values in this buffer (on drop) because the bounds
///      are defined by the deque it's owned by.
///
///   2. We can certainly avoid bounds checks using *T instead of Vec<T>, although
///      LLVM is probably pretty good at doing this already.
///
/// Note that we keep old buffers around after growing because stealers may still
/// be concurrently accessing them. The buffers are kept in a linked list, with
/// each buffer pointing to the previous, smaller buffer. This doesn't leak any
/// memory because all buffers in the list are freed when the deque is dropped.
#[derive(Debug)]
struct Buffer<T: Send> {
    storage: *mut T,
    size: usize,
    prev: Option<Box<Buffer<T>>>,
}

impl<T: Send> Deque<T> {
    pub fn new() -> Deque<T> {
        let buf = Box::new(unsafe { Buffer::new(MIN_SIZE) });
        Deque {
            bottom: AtomicIsize::new(0),
            top: AtomicIsize::new(0),
            array: AtomicPtr::new(Box::into_raw(buf)),
        }
    }

    #[inline]
    pub unsafe fn push(&self, data: T) {
        let b = self.bottom.load(Relaxed);
        let t = self.top.load(Acquire);
        let mut a = self.array.load(Relaxed);

        // Grow the buffer if it is full.
        let size = b.wrapping_sub(t);
        if size == (*a).size() {
            a = Box::into_raw(Box::from_raw(a).grow(b, t));
            self.array.store(a, Release);
        }

        (*a).put(b, data);
        fence(Release);
        self.bottom.store(b.wrapping_add(1), Relaxed);
    }

    #[inline]
    pub fn poll(&self) -> Poll<T> {
        unsafe {
            // Make sure top is read before bottom.
            let t = self.top.load(Acquire);
            fence(SeqCst);
            let b = self.bottom.load(Acquire);

            // Exit if the queue is empty.
            let size = b.wrapping_sub(t);
            if size <= 0 {
                return Empty;
            }

            // Fetch the element from the queue.
            let a = self.array.load(Acquire);
            let data = (*a).get(t);

            // Attempt to increment top.
            if self.top.compare_and_swap(t, t.wrapping_add(1), SeqCst) == t {
                Data(data)
            } else {
                forget(data); // Someone else stole this value
                Inconsistent
            }
        }
    }
}

impl<T: Send> Drop for Deque<T> {
    fn drop(&mut self) {
        let t = self.top.load(Relaxed);
        let b = self.bottom.load(Relaxed);
        let a = self.array.load(Relaxed);

        // Free whatever is leftover in the deque, and then free the buffer.
        // This will also free all linked buffers.
        let mut i = t;
        while i != b {
            unsafe { (*a).get(i) };
            i = i.wrapping_add(1);
        }
        unsafe { Box::from_raw(a) };
    }
}

#[inline]
unsafe fn take_ptr_from_vec<T>(mut buf: Vec<T>) -> *mut T {
    let ptr = buf.as_mut_ptr();
    forget(buf);
    ptr
}

#[inline]
unsafe fn allocate<T>(number: usize) -> *mut T {
    let v = Vec::with_capacity(number);
    take_ptr_from_vec(v)
}

#[inline]
unsafe fn deallocate<T>(ptr: *mut T, number: usize) {
    Vec::from_raw_parts(ptr, 0, number);
}

impl<T: Send> Buffer<T> {
    unsafe fn new(size: usize) -> Buffer<T> {
        Buffer {
            storage: allocate(size),
            size: size,
            prev: None,
        }
    }

    fn size(&self) -> isize { self.size as isize }

    fn mask(&self) -> isize { self.size as isize - 1 }

    unsafe fn elem(&self, i: isize) -> *mut T {
        self.storage.offset(i & self.mask())
    }

    // This does not protect against loading duplicate values of the same cell,
    // nor does this clear out the contents contained within. Hence, this is a
    // very unsafe method which the caller needs to treat specially in case a
    // race is lost.
    unsafe fn get(&self, i: isize) -> T {
        ptr::read(self.elem(i))
    }

    // Unsafe because this unsafely overwrites possibly uninitialized or
    // initialized data.
    unsafe fn put(&self, i: isize, t: T) {
        ptr::write(self.elem(i), t);
    }

    // Again, unsafe because this has incredibly dubious ownership violations.
    // It is assumed that this buffer is immediately dropped.
    unsafe fn grow(self: Box<Buffer<T>>, b: isize, t: isize) -> Box<Buffer<T>> {
        let mut buf = Box::new(Buffer::new(self.size * 2));
        let mut i = t;
        while i != b {
            buf.put(i, self.get(i));
            i = i.wrapping_add(1);
        }
        buf.prev = Some(self);
        return buf;
    }
}

impl<T: Send> Drop for Buffer<T> {
    fn drop(&mut self) {
        // It is assumed that all buffers are empty on drop.
        unsafe { deallocate(self.storage, self.size) }
    }
}
