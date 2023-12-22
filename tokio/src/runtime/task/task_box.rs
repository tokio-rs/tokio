//! Helper module for allocating and deallocating tasks.

use crate::future::Future;
use crate::runtime::task::core::{Cell, Core, Header, Trailer};
use crate::runtime::task::Schedule;

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::marker::PhantomData;
use std::mem::{align_of, size_of, ManuallyDrop};
use std::ptr::{drop_in_place, NonNull};

fn layout_of<T: Future, S: Schedule>(scheduler: &S) -> Layout {
    let size = std::mem::size_of::<Cell<T, S>>();
    let mut align = std::mem::align_of::<Cell<T, S>>();
    let min_align = scheduler.min_align();
    if align < min_align {
        align = min_align;
    }
    match Layout::from_size_align(size, align) {
        Ok(layout) => layout,
        Err(_) => panic!("Failed to build layout of type."),
    }
}

/// A `Box<Cell<T, S>>` with an alignment of at least `s.min_align()`.
pub(super) struct TaskBox<T: Future, S: Schedule> {
    ptr: NonNull<Cell<T, S>>,
    _phantom: PhantomData<Cell<T, S>>,
}

impl<T: Future, S: Schedule> TaskBox<T, S> {
    /// Creates a new task allocation.
    pub(super) fn new(header: Header, core: Core<T, S>) -> Self {
        let layout = layout_of::<T, S>(&core.scheduler);

        assert_eq!(size_of::<Cell<T, S>>(), layout.size());
        assert_ne!(size_of::<Cell<T, S>>(), 0);
        assert!(align_of::<Cell<T, S>>() <= layout.align());

        // SAFETY: The size of `layout` is non-zero as checked above.
        let ptr = unsafe { alloc(layout) } as *mut Cell<T, S>;

        let ptr = match NonNull::new(ptr) {
            Some(ptr) => ptr,
            None => handle_alloc_error(layout),
        };

        // SAFETY: We just allocated memory with the same size and a compatible
        // alignment for `Cell<T, S>`.
        unsafe {
            ptr.as_ptr().write(Cell {
                header,
                core,
                trailer: Trailer::new(),
            });
        };

        Self {
            ptr,
            _phantom: PhantomData,
        }
    }

    /// Convert this allocation into a raw pointer.
    pub(super) fn into_raw(self) -> NonNull<Cell<T, S>> {
        let me = ManuallyDrop::new(self);
        me.ptr
    }

    /// Convert this allocation back into a `TaskBox`.
    ///
    /// # Safety
    ///
    /// The provided pointer must originate from a previous call to `into_raw`,
    /// and the raw pointer must not be used again after this call.
    pub(super) unsafe fn from_raw(ptr: NonNull<Cell<T, S>>) -> Self {
        Self {
            ptr,
            _phantom: PhantomData,
        }
    }
}

impl<T: Future, S: Schedule> std::ops::Deref for TaskBox<T, S> {
    type Target = Cell<T, S>;

    fn deref(&self) -> &Cell<T, S> {
        // SAFETY: This box always points at a valid cell.
        unsafe { &*self.ptr.as_ptr() }
    }
}

impl<T: Future, S: Schedule> Drop for TaskBox<T, S> {
    fn drop(&mut self) {
        let ptr = self.ptr.as_ptr();

        // SAFETY: The task is still valid, so we can dereference the pointer.
        let layout = layout_of::<T, S>(unsafe { &(*ptr).core.scheduler });

        // SAFETY: The pointer was allocated with this layout. (The return value
        // of `min_align` doesn't change.)
        let _drop_helper = DropHelper {
            layout,
            ptr: ptr as *mut u8,
        };

        // SAFETY: A task box contains a pointer to a valid cell, and we have
        // not dropped the allocation yet.
        unsafe { drop_in_place(self.ptr.as_ptr()) };
    }
}

struct DropHelper {
    ptr: *mut u8,
    layout: Layout,
}

impl Drop for DropHelper {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: See `TaskBox::drop`.
        unsafe { dealloc(self.ptr, self.layout) };
    }
}
