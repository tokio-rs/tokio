use io_uring::{squeue::Entry, IoUring, Probe};
use mio::unix::SourceFd;
use slab::Slab;

use crate::runtime::driver::op::CancelData;
use crate::runtime::driver::op::CqeResult;
use crate::runtime::driver::op::{Cancellable, Lifecycle};
use crate::{io::Interest, loom::sync::Mutex};

use super::{Handle, TOKEN_WAKEUP};

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::{io, mem, task::Waker};

const DEFAULT_RING_SIZE: u32 = 256;

fn register_probe(uring: &IoUring, probe: &mut Probe) -> io::Result<()> {
    match uring.submitter().register_probe(probe) {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {
            // The kernel does not support IORING_REGISTER_PROBE.
            Err(io::Error::from_raw_os_error(libc::ENOSYS))
        }
        Err(e) => Err(e),
    }
}

/// Outcome of registering an op with the io_uring.
pub(crate) enum RegisterResult {
    /// In-flight at this slab index.
    Submitted(usize),

    /// Submit failed before reaching the kernel; the slot was removed.
    NotPushed(io::Error),

    /// Submit failed after reaching the kernel; the slot is kept and the caller
    /// must cancel it to keep its resources alive until completion.
    Pushed { index: usize, error: io::Error },
}

pub(crate) struct UringContext {
    pub(crate) uring: Option<io_uring::IoUring>,
    pub(crate) ops: Slab<Lifecycle>,
}

impl UringContext {
    pub(crate) fn new() -> Self {
        Self {
            ops: Slab::new(),
            uring: None,
        }
    }

    pub(crate) fn ring(&self) -> &io_uring::IoUring {
        self.uring.as_ref().expect("io_uring not initialized")
    }

    pub(crate) fn ring_mut(&mut self) -> &mut io_uring::IoUring {
        self.uring.as_mut().expect("io_uring not initialized")
    }

    pub(crate) fn dispatch_completions(&mut self) {
        let ops = &mut self.ops;
        let Some(uring) = self.uring.as_mut() else {
            // Uring is not initialized yet.
            return;
        };

        let cq = uring.completion();

        for cqe in cq {
            let idx = cqe.user_data() as usize;

            match ops.get_mut(idx) {
                Some(lifecycle @ Lifecycle::Waiting(_)) => {
                    let Lifecycle::Waiting(waker) =
                        mem::replace(lifecycle, Lifecycle::Completed(cqe))
                    else {
                        unreachable!()
                    };
                    waker.wake_by_ref();
                }
                Some(Lifecycle::Cancelled(cancel_data)) => {
                    if let CancelData::Open(_) = cancel_data {
                        if let Ok(fd) = CqeResult::from(cqe).result {
                            // SAFETY: the successful CQE result provides
                            // a non-negative integer, and the event is
                            // related to an open operation.
                            unsafe { OwnedFd::from_raw_fd(fd as i32) };
                        }
                    }
                    // Op future was cancelled, so we discard the result.
                    ops.remove(idx);
                }
                Some(other) => {
                    panic!("unexpected lifecycle for slot {idx}: {other:?}");
                }
                None => {
                    panic!("no op at index {idx}");
                }
            }
        }

        // `cq`'s drop gets called here, updating the latest head pointer
    }

    pub(crate) fn submit(&mut self) -> io::Result<()> {
        loop {
            // Errors from io_uring_enter: https://man7.org/linux/man-pages/man2/io_uring_enter.2.html#ERRORS
            match self.ring().submit() {
                Ok(_) => {
                    if self.ring_mut().submission().is_empty() {
                        return Ok(());
                    }
                }

                // If the submission queue is full, we dispatch completions and try again.
                Err(ref e) if e.raw_os_error() == Some(libc::EBUSY) => {
                    self.dispatch_completions();
                }
                Err(ref e) if e.raw_os_error() == Some(libc::EINTR) => {}
                // For other errors, we currently return the error as is.
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    pub(crate) fn remove_op(&mut self, index: usize) -> Lifecycle {
        self.ops.remove(index)
    }
}

/// Drop the driver, cancelling any in-progress ops and waiting for them to terminate.
impl Drop for UringContext {
    fn drop(&mut self) {
        if self.uring.is_none() {
            // Uring is not initialized or not supported.
            return;
        }

        // Make sure we flush the submission queue before dropping the driver.
        if !self.ring_mut().submission().is_empty() {
            if let Err(error) = self.submit() {
                // The kernel may still be using resources from previously submitted operations.
                mem::forget(mem::take(&mut self.ops));
                panic!("Internal error when dropping driver: {error:?}");
            }
        }

        let mut ops = std::mem::take(&mut self.ops);

        // Remove all completed ops since we don't need to wait for them.
        ops.retain(|_, lifecycle| !matches!(lifecycle, Lifecycle::Completed(_)));

        while !ops.is_empty() {
            // Wait until at least one completion is available.
            match self.ring_mut().submit_and_wait(1) {
                Ok(_) => {}
                // A failed wait can still leave completions available. Drain them below
                // before retrying, retaining resources for operations that remain pending.
                Err(error)
                    if matches!(
                        error.raw_os_error(),
                        Some(libc::EINTR | libc::EBUSY | libc::EAGAIN)
                    ) => {}
                Err(error) => {
                    // The kernel may still be using the resources held by pending operations.
                    mem::forget(ops);
                    panic!("Internal error when dropping driver: {error:?}");
                }
            }

            for cqe in self.ring_mut().completion() {
                let idx = cqe.user_data() as usize;

                if let Some(Lifecycle::Cancelled(CancelData::Open(_))) = ops.get_mut(idx) {
                    if let Ok(fd) = CqeResult::from(cqe).result {
                        // SAFETY: the successful CQE result provides
                        // a non-negative integer, and the event is
                        // related to an open operation.
                        unsafe { OwnedFd::from_raw_fd(fd as i32) };
                    }
                };

                ops.remove(idx);
            }
        }
    }
}

/// Returns `true` if the current runtime has io_uring set up and `opcode` is
/// supported by the kernel.
///
/// Returns `false` if there is no current runtime, the runtime has no IO
/// driver, or io_uring or opcode is unsupported. In that case the caller
/// should fall back to the blocking implementation.
pub(crate) fn uring_supports(opcode: u8) -> bool {
    crate::runtime::Handle::try_current().is_ok_and(|handle| {
        handle
            .inner
            .driver()
            .io
            .as_ref()
            .is_some_and(|io| io.is_uring_ready(opcode))
    })
}

impl Handle {
    /// Sets up io_uring for this IO driver: creates the ring, probes the
    /// opcodes supported by the kernel, and registers the ring's file
    /// descriptor so completions wake the driver.
    ///
    /// If the system doesn't support io_uring, or io_uring syscalls are
    /// blocked, this returns early without a probe, and file system operations
    /// fall back to the blocking thread pool.
    ///
    /// An error is returned if an io_uring syscall returns an unexpected error value.
    pub(crate) fn enable_io_uring(&mut self) -> io::Result<()> {
        let mut probe = Probe::new();
        match self.try_init(&mut probe) {
            Ok(_) => {}
            // If the system doesn't support io_uring, leave the probe unset.
            Err(e) if e.raw_os_error() == Some(libc::ENOSYS) => return Ok(()),
            // If we get EPERM, io-uring syscalls may be blocked (for example, by seccomp).
            // In this case, we fall back to spawn_blocking for all operations.
            // See also: https://github.com/tokio-rs/tokio/issues/7691
            Err(e) if e.raw_os_error() == Some(libc::EPERM) => return Ok(()),
            // For other system errors, we just return it.
            Err(e) => return Err(e),
        }

        self.uring_probe = Some(probe);
        Ok(())
    }

    pub(crate) fn get_uring(&self) -> &Mutex<UringContext> {
        &self.uring_context
    }

    /// Returns `true` if io_uring is set up and the given opcode is supported.
    pub(crate) fn is_uring_ready(&self, opcode: u8) -> bool {
        self.uring_probe
            .as_ref()
            .is_some_and(|probe| probe.is_supported(opcode))
    }

    /// Initialize the io_uring context if it hasn't been initialized yet.
    ///
    /// The ring is stored only after the reactor accepts its fd. A failure
    /// there drops the ring, so the following attempt can create another one.
    /// An existing ring stays stored, and this call fills `probe` from it.
    fn try_init(&self, probe: &mut Probe) -> io::Result<()> {
        let mut guard = self.get_uring().lock();
        if let Some(uring) = guard.uring.as_ref() {
            // The stored ring is already registered. Fill this probe from it.
            return register_probe(uring, probe);
        }

        let uring = IoUring::new(DEFAULT_RING_SIZE)?;
        register_probe(&uring, probe)?;

        let fd = uring.as_raw_fd();
        let mut source = SourceFd(&fd);
        self.registry
            .register(&mut source, TOKEN_WAKEUP, Interest::READABLE.to_mio())?;
        guard.uring = Some(uring);
        Ok(())
    }

    /// Register an operation with the io_uring.
    ///
    /// Callers must check [`Handle::is_uring_ready`] for the entry's opcode first.
    ///
    /// # Safety
    ///
    /// Callers must ensure that parameters of the entry (such as buffer) are valid and will
    /// be valid for the entire duration of the operation, otherwise it may cause memory problems.
    pub(crate) unsafe fn register_op(&self, entry: Entry, waker: Waker) -> RegisterResult {
        assert!(self.uring_probe.is_some());

        // Uring is initialized.

        let mut guard = self.get_uring().lock();
        let ctx = &mut *guard;
        let index = ctx.ops.insert(Lifecycle::Waiting(waker));
        let entry = entry.user_data(index as u64);

        // `entry` has not been pushed yet, so a submit error here means it never
        // reached the kernel and we can drop the slot before returning.
        let submit_before_push = |ctx: &mut UringContext| -> io::Result<()> {
            if let Err(e) = ctx.submit() {
                ctx.remove_op(index);
                return Err(e);
            }
            Ok(())
        };

        // SAFETY: entry is valid for the entire duration of the operation
        while unsafe { ctx.ring_mut().submission().push(&entry).is_err() } {
            // If the submission queue is full, flush it to the kernel and retry.
            if let Err(e) = submit_before_push(ctx) {
                return RegisterResult::NotPushed(e);
            }
        }

        // Ensure that the completion queue is not full before submitting the entry.
        while ctx.ring_mut().completion().is_full() {
            ctx.dispatch_completions();
        }

        // Note: For now, we submit the entry immediately without utilizing batching.
        //
        // `entry` is now pushed, so the kernel may already own the buffer it
        // points at. Keep the slot on a submit error: dropping it would let the
        // `Op` free the buffer while it is still in use.
        if let Err(e) = ctx.submit() {
            return RegisterResult::Pushed { index, error: e };
        }

        RegisterResult::Submitted(index)
    }

    pub(crate) fn cancel_op<T: Cancellable>(&self, index: usize, data: Option<T>) {
        let mut guard = self.get_uring().lock();
        let ctx = &mut *guard;
        let ops = &mut ctx.ops;
        let Some(lifecycle) = ops.get_mut(index) else {
            // The corresponding index doesn't exist anymore, so this Op is already complete.
            return;
        };

        // This Op will be cancelled. Here, we don't remove the lifecycle from the slab to keep
        // uring data alive until the operation completes.

        let cancel_data = data.expect("Data should be present").cancel();
        match mem::replace(lifecycle, Lifecycle::Cancelled(cancel_data)) {
            Lifecycle::Submitted | Lifecycle::Waiting(_) => (),
            // The driver saw the completion, but it was never polled.
            Lifecycle::Completed(cqe) => {
                if let Lifecycle::Cancelled(CancelData::Open(_)) = lifecycle {
                    if let Ok(fd) = CqeResult::from(cqe).result {
                        // SAFETY: the successful CQE result provides
                        // a non-negative integer, and the event is
                        // related to an open operation.
                        unsafe { OwnedFd::from_raw_fd(fd as i32) };
                    }
                }
                // We can safely remove the entry from the slab, as it has already been completed.
                ops.remove(index);
            }
            prev => panic!("Unexpected state: {prev:?}"),
        };
    }
}

#[cfg(all(test, not(loom)))]
mod tests;
