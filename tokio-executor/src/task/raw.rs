use crate::loom::alloc::{self, Track};
use crate::loom::cell::CausalCell;
use crate::task::{Harness, Header, Schedule, Snapshot, State, Trailer, Vtable};

use std::alloc::Layout;
use std::cell::UnsafeCell;
use std::cmp;
use std::future::Future;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};
use std::task::Waker;

/// Raw task handle
pub(super) struct RawTask<S: 'static> {
    ptr: NonNull<Header<S>>,
}

/// Wrap a raw pointer with a harness, providing task functionality.
pub(super) unsafe fn harness<T, S>(ptr: *const ()) -> Harness<T, S>
where
    T: Future,
    S: 'static,
{
    debug_assert!(!ptr.is_null());

    let (_, future_offset, trailer_offset) = layout_task::<T, S>();

    let header = ptr as *mut Header<S>;
    let trailer = (ptr as *mut u8).add(trailer_offset) as *mut Trailer;
    let future_or_output = (header as *mut u8).add(future_offset) as *mut ();

    Harness::from_ptr(
        NonNull::new_unchecked(header),
        NonNull::new_unchecked(trailer),
        future_or_output,
    )
}

/// Deallocate task struct referenced by `ptr`.
///
/// All drop operations must have already been performed on data contained by
/// the struct.
pub(super) unsafe fn dealloc<T, S>(ptr: NonNull<Header<S>>)
where
    T: Future,
    S: 'static,
{
    let (layout, _, _) = layout_task::<T, S>();
    alloc::dealloc(ptr.as_ptr() as *mut u8, layout);
}

impl<S> RawTask<S> {
    pub(super) fn new_background<T>(task: T) -> RawTask<S>
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        RawTask::new(task, State::new_background())
    }

    pub(super) fn new_joinable<T>(task: T) -> RawTask<S>
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        RawTask::new(task, State::new_joinable())
    }

    fn new<T>(task: T, state: State) -> RawTask<S>
    where
        T: Future + Send + 'static,
        S: Schedule,
    {
        let (layout, offset_future, offset_trailer) = layout_task::<T, S>();

        unsafe {
            // Allocate memory for the task
            let ptr = alloc::alloc(layout);

            ptr::write(
                ptr as *mut Header<S>,
                Header {
                    state,
                    executor: CausalCell::new(None),
                    queue_next: UnsafeCell::new(ptr::null()),
                    owned_next: UnsafeCell::new(ptr::null()),
                    owned_prev: UnsafeCell::new(ptr::null()),
                    vtable: &Vtable {
                        poll: poll::<T, S>,
                        drop_task: drop_task::<T, S>,
                        read_output: read_output::<T, S>,
                        store_join_waker: store_join_waker::<T, S>,
                        swap_join_waker: swap_join_waker::<T, S>,
                        drop_join_handle_slow: drop_join_handle_slow::<T, S>,
                        cancel: cancel::<T, S>,
                    },
                    future_causality: CausalCell::new(()),
                },
            );

            // Write future
            ptr::write(ptr.add(offset_future) as *mut Track<T>, Track::new(task));

            // Write trailer
            ptr::write(
                ptr.add(offset_trailer) as *mut Trailer,
                Trailer {
                    waker: CausalCell::new(MaybeUninit::new(None)),
                },
            );

            let ptr = NonNull::new_unchecked(ptr as *mut Header<S>);

            RawTask { ptr }
        }
    }

    pub(super) unsafe fn from_raw(ptr: NonNull<Header<S>>) -> RawTask<S> {
        RawTask { ptr }
    }

    /// Returns a reference to the task's meta structure.
    ///
    /// Safe as `Header` is `Sync`.
    pub(super) fn header(&self) -> &Header<S> {
        unsafe { self.ptr.as_ref() }
    }

    /// Returns a raw pointer to the task's meta structure.
    pub(super) fn header_ptr(&self) -> NonNull<Header<S>> {
        self.ptr
    }

    /// Safety: mutual exclusion is required to call this function.
    ///
    /// Returns `true` if the task needs to be scheduled again.
    pub(super) unsafe fn poll(&self, executor: NonNull<S>) -> bool {
        // Get the vtable without holding a ref to the meta struct. This is done
        // because a mutable reference to the task is passed into the poll fn.
        let vtable = self.header().vtable;

        (vtable.poll)(self.ptr.as_ptr() as *mut (), executor)
    }

    pub(super) fn drop_task(&self) {
        let vtable = self.header().vtable;
        unsafe {
            (vtable.drop_task)(self.ptr.as_ptr() as *mut ());
        }
    }

    pub(super) unsafe fn read_output(&self, dst: *mut (), state: Snapshot) {
        let vtable = self.header().vtable;
        (vtable.read_output)(self.ptr.as_ptr() as *mut (), dst, state);
    }

    pub(super) fn store_join_waker(&self, waker: &Waker) -> Snapshot {
        let vtable = self.header().vtable;
        unsafe { (vtable.store_join_waker)(self.ptr.as_ptr() as *mut (), waker) }
    }

    pub(super) fn swap_join_waker(&self, waker: &Waker, prev: Snapshot) -> Snapshot {
        let vtable = self.header().vtable;
        unsafe { (vtable.swap_join_waker)(self.ptr.as_ptr() as *mut (), waker, prev) }
    }

    pub(super) fn drop_join_handle_slow(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.drop_join_handle_slow)(self.ptr.as_ptr() as *mut ()) }
    }

    pub(super) fn cancel_from_queue(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.cancel)(self.ptr.as_ptr() as *mut (), true) }
    }
}

impl<S: 'static> Clone for RawTask<S> {
    fn clone(&self) -> Self {
        RawTask { ptr: self.ptr }
    }
}

impl<S: 'static> Copy for RawTask<S> {}

unsafe fn poll<T: Future, S: Schedule>(ptr: *mut (), executor: NonNull<S>) -> bool {
    let harness = harness::<T, S>(ptr);
    harness.poll(executor)
}

unsafe fn drop_task<T: Future, S: Schedule>(ptr: *mut ()) {
    let harness = harness::<T, S>(ptr);
    harness.drop_task();
}

unsafe fn read_output<T: Future, S: Schedule>(ptr: *mut (), dst: *mut (), state: Snapshot) {
    let harness = harness::<T, S>(ptr);
    harness.read_output(dst as *mut Track<super::Result<T::Output>>, state);
}

unsafe fn store_join_waker<T: Future, S: Schedule>(ptr: *mut (), waker: &Waker) -> Snapshot {
    let harness = harness::<T, S>(ptr);
    harness.store_join_waker(waker)
}

unsafe fn swap_join_waker<T: Future, S: Schedule>(
    ptr: *mut (),
    waker: &Waker,
    prev: Snapshot,
) -> Snapshot {
    let harness = harness::<T, S>(ptr);
    harness.swap_join_waker(waker, prev)
}

unsafe fn drop_join_handle_slow<T: Future, S: Schedule>(ptr: *mut ()) {
    let harness = harness::<T, S>(ptr);
    harness.drop_join_handle_slow()
}

unsafe fn cancel<T: Future, S: Schedule>(ptr: *mut (), from_queue: bool) {
    let harness = harness::<T, S>(ptr);
    harness.cancel(from_queue)
}

/// Returns the layout of the combined meta and future as well as the offset to
/// the union of the future / output.
fn layout_task<T, S>() -> (Layout, usize, usize)
where
    T: Future,
    S: 'static,
{
    // Compute the layout of the union of the task and the output.
    let layout_union_task_ret = layout_union(
        Layout::new::<Track<T>>(),
        Layout::new::<Track<super::Result<T::Output>>>(),
    );

    let (layout, offset_future) = layout_extend(Layout::new::<Header<S>>(), layout_union_task_ret);

    let (layout, offset_trailer) = layout_extend(layout, Layout::new::<Trailer>());

    (layout, offset_future, offset_trailer)
}

fn layout_union(first: Layout, second: Layout) -> Layout {
    let size = cmp::max(first.size(), second.size());
    let align = cmp::max(first.align(), second.align());

    Layout::from_size_align(size, align).unwrap()
}

/// Returns the layout of `first` followed by `second as well as the offset at
/// which `second` can be found in the returned layout.
fn layout_extend(first: Layout, next: Layout) -> (Layout, usize) {
    let new_align = cmp::max(first.align(), next.align());
    let pad = layout_padding_needed_for(first, next.align());

    let offset = first.size().checked_add(pad).unwrap();

    let new_size = offset.checked_add(next.size()).unwrap();

    let layout = Layout::from_size_align(new_size, new_align).unwrap();
    (layout, offset)
}

fn layout_padding_needed_for(layout: Layout, align: usize) -> usize {
    let len = layout.size();

    // Rounded up value is:
    //   len_rounded_up = (len + align - 1) & !(align - 1);
    // and then we return the padding difference: `len_rounded_up - len`.
    //
    // We use modular arithmetic throughout:
    //
    // 1. align is guaranteed to be > 0, so align - 1 is always
    //    valid.
    //
    // 2. `len + align - 1` can overflow by at most `align - 1`,
    //    so the &-mask wth `!(align - 1)` will ensure that in the
    //    case of overflow, `len_rounded_up` will itself be 0.
    //    Thus the returned padding, when added to `len`, yields 0,
    //    which trivially satisfies the alignment `align`.
    //
    // (Of course, attempts to allocate blocks of memory whose
    // size and padding overflow in the above manner should cause
    // the allocator to yield an error anyway.)

    let len_rounded_up = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);

    len_rounded_up.wrapping_sub(len)
}
