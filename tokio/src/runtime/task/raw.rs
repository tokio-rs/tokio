use crate::future::Future;
use crate::runtime::task::core::{Core, Trailer};
use crate::runtime::task::{Cell, Harness, Header, Id, Schedule, State};

use std::ptr::NonNull;
use std::task::{Poll, Waker};

/// Raw task handle
pub(super) struct RawTask {
    ptr: NonNull<Header>,
}

pub(super) struct Vtable {
    /// Polls the future.
    pub(super) poll: unsafe fn(NonNull<Header>),

    /// Deallocates the memory.
    pub(super) dealloc: unsafe fn(NonNull<Header>),

    /// Reads the task output, if complete.
    pub(super) try_read_output: unsafe fn(NonNull<Header>, *mut (), &Waker),

    /// Try to set the waker notified when the task is complete. Returns true if
    /// the task has already completed. If this call returns false, then the
    /// waker will not be notified.
    pub(super) try_set_join_waker: unsafe fn(NonNull<Header>, &Waker) -> bool,

    /// The join handle has been dropped.
    pub(super) drop_join_handle_slow: unsafe fn(NonNull<Header>),

    /// An abort handle has been dropped.
    pub(super) drop_abort_handle: unsafe fn(NonNull<Header>),

    /// The task is remotely aborted.
    pub(super) remote_abort: unsafe fn(NonNull<Header>),

    /// Scheduler is being shutdown.
    pub(super) shutdown: unsafe fn(NonNull<Header>),

    /// The number of bytes that the `trailer` field is offset from the header.
    pub(super) trailer_offset: usize,
}

/// Get the vtable for the requested `T` and `S` generics.
pub(super) fn vtable<T: Future, S: Schedule>() -> &'static Vtable {
    &Vtable {
        poll: poll::<T, S>,
        dealloc: dealloc::<T, S>,
        try_read_output: try_read_output::<T, S>,
        try_set_join_waker: try_set_join_waker::<T, S>,
        drop_join_handle_slow: drop_join_handle_slow::<T, S>,
        drop_abort_handle: drop_abort_handle::<T, S>,
        remote_abort: remote_abort::<T, S>,
        shutdown: shutdown::<T, S>,
        trailer_offset: TrailerOffsetHelper::<T, S>::OFFSET,
    }
}

/// Calling `get_trailer_offset` directly in vtable doesn't work because it
/// prevents the vtable from being promoted to a static reference.
///
/// See this thread for more info:
/// <https://users.rust-lang.org/t/custom-vtables-with-integers/78508>
struct TrailerOffsetHelper<T, S>(T, S);
impl<T: Future, S: Schedule> TrailerOffsetHelper<T, S> {
    // Pass `size_of`/`align_of` as arguments rather than calling them directly
    // inside `get_trailer_offset` because trait bounds on generic parameters
    // of const fn are unstable on our MSRV.
    const OFFSET: usize = get_trailer_offset(
        std::mem::size_of::<Header>(),
        std::mem::size_of::<Core<T, S>>(),
        std::mem::align_of::<Core<T, S>>(),
        std::mem::align_of::<Trailer>(),
    );
}

/// Compute the offset of the `Trailer` field in `Cell<T, S>` using the
/// `#[repr(C)]` algorithm.
///
/// Pseudo-code for the `#[repr(C)]` algorithm can be found here:
/// <https://doc.rust-lang.org/reference/type-layout.html#reprc-structs>
const fn get_trailer_offset(
    header_size: usize,
    core_size: usize,
    core_align: usize,
    trailer_align: usize,
) -> usize {
    let mut offset = header_size;

    let core_misalign = offset % core_align;
    if core_misalign > 0 {
        offset += core_align - core_misalign;
    }
    offset += core_size;

    let trailer_misalign = offset % trailer_align;
    if trailer_misalign > 0 {
        offset += trailer_align - trailer_misalign;
    }

    offset
}

impl RawTask {
    pub(super) fn new<T, S>(task: T, scheduler: S, id: Id) -> RawTask
    where
        T: Future,
        S: Schedule,
    {
        let ptr = Box::into_raw(Cell::<_, S>::new(task, scheduler, State::new(), id));
        let ptr = unsafe { NonNull::new_unchecked(ptr as *mut Header) };

        RawTask { ptr }
    }

    pub(super) unsafe fn from_raw(ptr: NonNull<Header>) -> RawTask {
        RawTask { ptr }
    }

    pub(super) fn header_ptr(&self) -> NonNull<Header> {
        self.ptr
    }

    /// Returns a reference to the task's meta structure.
    ///
    /// Safe as `Header` is `Sync`.
    pub(super) fn header(&self) -> &Header {
        unsafe { self.ptr.as_ref() }
    }

    /// Safety: mutual exclusion is required to call this function.
    pub(super) fn poll(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.poll)(self.ptr) }
    }

    pub(super) fn dealloc(self) {
        let vtable = self.header().vtable;
        unsafe {
            (vtable.dealloc)(self.ptr);
        }
    }

    /// Safety: `dst` must be a `*mut Poll<super::Result<T::Output>>` where `T`
    /// is the future stored by the task.
    pub(super) unsafe fn try_read_output(self, dst: *mut (), waker: &Waker) {
        let vtable = self.header().vtable;
        (vtable.try_read_output)(self.ptr, dst, waker);
    }

    pub(super) fn try_set_join_waker(self, waker: &Waker) -> bool {
        let vtable = self.header().vtable;
        unsafe { (vtable.try_set_join_waker)(self.ptr, waker) }
    }

    pub(super) fn drop_join_handle_slow(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.drop_join_handle_slow)(self.ptr) }
    }

    pub(super) fn drop_abort_handle(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.drop_abort_handle)(self.ptr) }
    }

    pub(super) fn shutdown(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.shutdown)(self.ptr) }
    }

    pub(super) fn remote_abort(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.remote_abort)(self.ptr) }
    }

    /// Increment the task's reference count.
    ///
    /// Currently, this is used only when creating an `AbortHandle`.
    pub(super) fn ref_inc(self) {
        self.header().state.ref_inc();
    }
}

impl Clone for RawTask {
    fn clone(&self) -> Self {
        RawTask { ptr: self.ptr }
    }
}

impl Copy for RawTask {}

unsafe fn poll<T: Future, S: Schedule>(ptr: NonNull<Header>) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.poll();
}

unsafe fn dealloc<T: Future, S: Schedule>(ptr: NonNull<Header>) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.dealloc();
}

unsafe fn try_read_output<T: Future, S: Schedule>(
    ptr: NonNull<Header>,
    dst: *mut (),
    waker: &Waker,
) {
    let out = &mut *(dst as *mut Poll<super::Result<T::Output>>);

    let harness = Harness::<T, S>::from_raw(ptr);
    harness.try_read_output(out, waker);
}

unsafe fn try_set_join_waker<T: Future, S: Schedule>(ptr: NonNull<Header>, waker: &Waker) -> bool {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.try_set_join_waker(waker)
}

unsafe fn drop_join_handle_slow<T: Future, S: Schedule>(ptr: NonNull<Header>) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.drop_join_handle_slow()
}

unsafe fn drop_abort_handle<T: Future, S: Schedule>(ptr: NonNull<Header>) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.drop_reference();
}

unsafe fn remote_abort<T: Future, S: Schedule>(ptr: NonNull<Header>) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.remote_abort()
}

unsafe fn shutdown<T: Future, S: Schedule>(ptr: NonNull<Header>) {
    let harness = Harness::<T, S>::from_raw(ptr);
    harness.shutdown()
}
