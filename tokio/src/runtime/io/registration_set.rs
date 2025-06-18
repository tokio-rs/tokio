use crate::loom::sync::atomic::AtomicUsize;
use crate::runtime::io::{Driver, ScheduledIo};
use crate::util::linked_list::{self, LinkedList};

use std::io;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

// Kind of arbitrary, but buffering 16 `ScheduledIo`s doesn't seem like much
const NOTIFY_AFTER: usize = 16;

pub(super) struct RegistrationSet {
    num_pending_release: AtomicUsize,
}

pub(super) struct Synced {
    // True when the I/O driver shutdown. At this point, no more registrations
    // should be added to the set.
    is_shutdown: bool,

    // List of all registrations tracked by the set
    registrations: LinkedList<Arc<ScheduledIo>, ScheduledIo>,

    // Registrations that are pending drop. When a `Registration` is dropped, it
    // stores its `ScheduledIo` in this list. The I/O driver is responsible for
    // dropping it. This ensures the `ScheduledIo` is not freed while it can
    // still be included in an I/O event.
    pending_release: Vec<Arc<ScheduledIo>>,
}

impl RegistrationSet {
    pub(super) fn new() -> (RegistrationSet, Synced) {
        let set = RegistrationSet {
            num_pending_release: AtomicUsize::new(0),
        };

        let synced = Synced {
            is_shutdown: false,
            registrations: LinkedList::new(),
            pending_release: Vec::with_capacity(NOTIFY_AFTER),
        };

        (set, synced)
    }

    pub(super) fn is_shutdown(&self, synced: &Synced) -> bool {
        synced.is_shutdown
    }

    /// Returns `true` if there are registrations that need to be released
    ///
    // This method doesn't use `_driver` directly, but it is passed
    // to ensure that the caller holds the I/O driver lock,
    // which make the safety more clear.
    pub(super) fn needs_release(&self, _driver: &mut Driver) -> bool {
        // `Relaxed` is sufficient here because:
        //   - This method is only called with the I/O driver locked.
        //   - AND the method `Self::release` is also only called
        //     with the both I/O driver and `Synced` locked.
        //
        // So there are three possibilities to get `0` here:
        //   1. `num_pending_release` was never changed,
        //     which means that `0` is the initial value
        //     that was assigned in `Self::new`.
        //   2. There was a call to `Self::release` that happens-before this call.
        //   3. There was a call to `Self::deregister`, but its increment of
        //     `num_pending_release` is not yet visible to this thread.
        //
        // For (1), apparently, nothing need to be synchronized.
        // For (2), `0` is absolutely latest value, so no need to worry about it.
        // For (3), this doesn't cause use-after-free, because
        //   - It is impossible to call this method concurrently with the driver
        //     lock held.
        //   - AND `0` means the caller will not call `Self::release`, so nothing
        //     will be free.
        //
        // To recap, it is impossible to return `true` but nothing is pending release.
        //
        // There are only one possibility to get `> 0` here:
        //   1. There was a call to `Self::deregister` that appears in this call.
        //
        // For (1), this doesn't cause use-after-free, because
        //   - It is already deregistered out of the mio.
        //   - AND this call is sequenced-before polling the mio.
        //
        // To recap, it is impossible to return `false` but there are pending releases,
        // but this is harmless, and the `Self::deregister` will notify the driver
        // if there are too many pending releases.
        self.num_pending_release.load(Relaxed) != 0
    }

    pub(super) fn allocate(&self, synced: &mut Synced) -> io::Result<Arc<ScheduledIo>> {
        if synced.is_shutdown {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                crate::util::error::RUNTIME_SHUTTING_DOWN_ERROR,
            ));
        }

        let ret = Arc::new(ScheduledIo::default());

        // Push a ref into the list of all resources.
        synced.registrations.push_front(ret.clone());

        Ok(ret)
    }

    // Returns `true` if the caller should unblock the I/O driver to purge
    // registrations pending release.
    pub(super) fn deregister(&self, synced: &mut Synced, registration: &Arc<ScheduledIo>) -> bool {
        synced.pending_release.push(registration.clone());

        let len = synced.pending_release.len();
        // `Relaxed` is sufficient because this will become
        // the side-effect of unlocking the `Synced`, and
        // `Self::release` will always observe this store.
        //
        // The `Self::needs_release` might observe the old value,
        // but it is harmless, please checkout the comment
        // in `Self::needs_release` for detailed explanation
        // if you are interested.
        self.num_pending_release.store(len, Relaxed);

        len == NOTIFY_AFTER
    }

    pub(super) fn shutdown(&self, synced: &mut Synced) -> Vec<Arc<ScheduledIo>> {
        if synced.is_shutdown {
            return vec![];
        }

        synced.is_shutdown = true;
        synced.pending_release.clear();

        // Building a vec of all outstanding I/O handles could be expensive, but
        // this is the shutdown operation. In theory, shutdowns should be
        // "clean" with no outstanding I/O resources. Even if it is slow, we
        // aren't optimizing for shutdown.
        let mut ret = vec![];

        while let Some(io) = synced.registrations.pop_back() {
            ret.push(io);
        }

        ret
    }

    // This method doesn't use `_driver` directly, but it is passed
    // to ensure that the caller holds the I/O driver lock,
    // which make the safety more clear.
    pub(super) fn release(&self, synced: &mut Synced, _driver: &mut Driver) {
        let pending = std::mem::take(&mut synced.pending_release);

        for io in pending {
            // safety: the registration is part of our list
            unsafe { self.remove(synced, &io) }
        }

        // `Relaxed` is sufficient here because this method is only called
        // with both the I/O driver and `Synced` locked.
        //
        // With the I/O driver locked, the next call to
        // method `Self::needs_release` will observe this store
        // or other stores (Self::deregister) that follow this store
        // in modification order.
        //
        // If `Self::needs_release` observes this store, apparently,
        // the driver will not release anything.
        //
        // If `Self::needs_release` observes a stores that follows this store
        // in modification order, this is harmless, please checkout the comment
        // in `Self::needs_release` for detailed explanation if you are interested.
        self.num_pending_release.store(0, Relaxed);
    }

    // This function is marked as unsafe, because the caller must make sure that
    // `io` is part of the registration set.
    pub(super) unsafe fn remove(&self, synced: &mut Synced, io: &Arc<ScheduledIo>) {
        // SAFETY: Pointers into an Arc are never null.
        let io = unsafe { NonNull::new_unchecked(Arc::as_ptr(io).cast_mut()) };

        super::EXPOSE_IO.unexpose_provenance(io.as_ptr());
        let _ = synced.registrations.remove(io);
    }
}

// Safety: `Arc` pins the inner data
unsafe impl linked_list::Link for Arc<ScheduledIo> {
    type Handle = Arc<ScheduledIo>;
    type Target = ScheduledIo;

    fn as_raw(handle: &Self::Handle) -> NonNull<ScheduledIo> {
        // safety: Arc::as_ptr never returns null
        unsafe { NonNull::new_unchecked(Arc::as_ptr(handle) as *mut _) }
    }

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Arc<ScheduledIo> {
        // safety: the linked list currently owns a ref count
        unsafe { Arc::from_raw(ptr.as_ptr() as *const _) }
    }

    unsafe fn pointers(
        target: NonNull<Self::Target>,
    ) -> NonNull<linked_list::Pointers<ScheduledIo>> {
        NonNull::new_unchecked(target.as_ref().linked_list_pointers.get())
    }
}
