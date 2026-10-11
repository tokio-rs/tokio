use crate::loom::sync::atomic::AtomicUsize;
use crate::runtime::io::ScheduledIo;
use crate::util::linked_list::{self, LinkedList};

use crate::loom::sync::Arc;
use std::io;
use std::ptr::NonNull;
use std::sync::atomic::Ordering::{Acquire, Release};

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
    registrations: LinkedList<Arc<ScheduledIo>>,

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
    pub(super) fn needs_release(&self) -> bool {
        self.num_pending_release.load(Acquire) != 0
    }

    pub(super) fn allocate(&self, synced: &mut Synced) -> io::Result<Arc<ScheduledIo>> {
        if synced.is_shutdown {
            return Err(io::Error::other(
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
        self.num_pending_release.store(len, Release);

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

    pub(super) fn release(&self, synced: &mut Synced) {
        let pending = std::mem::take(&mut synced.pending_release);

        for io in pending {
            // safety: the registration is part of our list
            unsafe { self.remove(synced, &io) }
        }

        self.num_pending_release.store(0, Release);
    }

    // This function is marked as unsafe, because the caller must make sure that
    // `io` is part of the registration set.
    pub(super) unsafe fn remove(&self, synced: &mut Synced, io: &Arc<ScheduledIo>) {
        // SAFETY: Pointers into an Arc are never null.
        let io = unsafe { NonNull::new_unchecked(Arc::as_ptr(io).cast_mut()) };

        super::EXPOSE_IO.unexpose_provenance(io.as_ptr());
        // SAFETY: the caller guarantees that `io` is part of this list.
        let _ = unsafe { synced.registrations.remove(io) };
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
        // safety: `target.as_ref().linked_list_pointers` is a `UnsafeCell` that
        // always returns a non-null pointer.
        unsafe { NonNull::new_unchecked(target.as_ref().linked_list_pointers.get()) }
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use super::*;
    use crate::io::ready::Ready;
    use crate::loom::sync::Mutex;
    use crate::loom::thread;

    // Models only Tokio's registration lifetime. Mio polling and OS deregistration
    // are outside the model; the production Handle remains unchanged under Loom.
    struct Registrations {
        set: RegistrationSet,
        synced: Mutex<Synced>,
    }

    impl Registrations {
        fn new() -> Self {
            let (set, synced) = RegistrationSet::new();
            Self {
                set,
                synced: Mutex::new(synced),
            }
        }

        fn allocate(&self) -> Arc<ScheduledIo> {
            self.set.allocate(&mut self.synced.lock()).unwrap()
        }

        fn deregister(&self, io: &Arc<ScheduledIo>) {
            self.set.deregister(&mut self.synced.lock(), io);
        }

        fn release(&self) {
            if self.set.needs_release() {
                self.set.release(&mut self.synced.lock());
            }
        }
    }

    /// Models deregistration racing with delivery of an event already returned by
    /// the poller. The driver completes delivery before its next release step.
    #[test]
    fn deregister_races_with_prior_event_delivery() {
        loom::model(|| {
            let handle = Arc::new(Registrations::new());
            let scheduled_io = handle.allocate();
            let token = scheduled_io.token();

            let handle_clone = handle.clone();
            let driver_th = thread::spawn(move || {
                // Driver thread: has already obtained an event from poll() for `token`.
                // Delivers event to ScheduledIo via its exposed raw pointer token.
                //
                // SAFETY: The driver holds the registration alive in `RegistrationSet`
                // until `release()` is called. In this model, event delivery occurs
                // before the driver's release step.
                unsafe {
                    super::super::driver::dispatch_event(token, Ready::READABLE);
                }
                // Simulates deferred release performed at the start of the next turn/poll.
                handle_clone.release();
            });

            // Worker thread: deregisters the resource and drops its Arc handle.
            handle.deregister(&scheduled_io);
            drop(scheduled_io);

            driver_th.join().unwrap();

            // Flush any remaining release if worker deregistered after driver's release step.
            handle.release();

            assert!(!handle.set.needs_release());
        });
    }
}
