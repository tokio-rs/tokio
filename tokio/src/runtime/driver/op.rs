use crate::io::uring::open::Open;
use crate::io::uring::read::Read;
use crate::io::uring::write::Write;
use crate::runtime::Handle;

use io_uring::{cqueue, squeue};
use std::future::Future;
use std::io::{self};
use std::mem;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

const DEFAULT_BATCH_SIZE: usize = u8::MAX as usize;

// This field isn't accessed directly, but it holds cancellation data,
// so `#[allow(dead_code)]` is needed.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum CancelData {
    Open(Open),
    Write(Write),
    Read(Read),
}

#[derive(Debug)]
pub(crate) enum Lifecycle {
    /// The operation has been submitted to uring and is currently in-flight
    Submitted,

    /// The submitter is waiting for the completion of the operation
    Waiting(Waker),

    /// The submitter no longer has interest in the operation result. The state
    /// must be passed to the driver and held until the operation completes.
    Cancelled(
        // This field isn't accessed directly, but it holds cancellation data,
        // so `#[allow(dead_code)]` is needed.
        #[allow(dead_code)] Arc<CancelData>,
    ),

    /// The operation has completed with a single cqe result
    Completed(io_uring::cqueue::Entry),
}

/// Const parameter is only used for Batch
pub(crate) enum State<const N: usize> {
    // Single operation state
    Initialize(Option<squeue::Entry>),
    Polled(usize),
    // Batch operation state
    InitializeBatch([squeue::Entry; N]),
    PolledBatch([usize; N]),
    // Batch or single operation is completed
    Complete,
}

pub(crate) struct Op<T: Cancellable, const N: usize = DEFAULT_BATCH_SIZE> {
    // Handle to the runtime
    handle: Handle,
    // State of this Op
    state: State<N>,
    // Per operation data.
    data: Option<T>,
    // Completed CQEs stored for checking batch completion
    completed: [i32; N],
}

impl<T: Cancellable, const N: usize> Op<T, N> {
    /// # Safety
    ///
    /// Callers must ensure that parameters of the entry (such as buffer) are valid and will
    /// be valid for the entire duration of the operation, otherwise it may cause memory problems.
    pub(crate) unsafe fn new(entry: squeue::Entry, data: T) -> Self {
        let handle = Handle::current();

        Self {
            handle,
            data: Some(data),
            state: State::Initialize(Some(entry)),
            completed: [0; N],
        }
    }

    pub(crate) unsafe fn batch(entries: [squeue::Entry; N], data: T) -> Self {
        let handle = Handle::current();

        Self {
            handle,
            data: Some(data),
            state: State::InitializeBatch(entries),
            completed: [0; N],
        }
    }

    pub(crate) fn take_data(&mut self) -> Option<T> {
        self.data.take()
    }
}

impl<T: Cancellable, const N: usize> Drop for Op<T, N> {
    fn drop(&mut self) {
        match self.state {
            // We've already dropped this Op.
            State::Complete => return,
            // This Op has not been polled yet.
            // We don't need to do anything here.
            State::Initialize(_) | State::InitializeBatch(_) => return,
            _ => (),
        }

        let data = self.take_data();
        let handle = &mut self.handle;

        match &self.state {
            // We will cancel this Op.
            State::Polled(index) => {
                handle.inner.driver().io().cancel_op(*index, data);
            }
            State::PolledBatch(ids) => {
                handle.inner.driver().io().cancel_batched_op(ids, data);
            }
            _ => (),
        }
    }
}

/// Result of an completed operation
pub(crate) enum CqeResult<const N: usize = DEFAULT_BATCH_SIZE> {
    Single(io::Result<u32>),
    Batch([i32; N]),
    // This is used when you want to terminate an operation with an error.
    InitErr(io::Error),
}

impl<const N: usize> From<cqueue::Entry> for CqeResult<N> {
    fn from(cqe: cqueue::Entry) -> Self {
        CqeResult::Single(cqe_to_result(cqe))
    }
}

/// A trait that converts a CQE result into a usable value for each operation.
pub(crate) trait Completable<const N: usize = DEFAULT_BATCH_SIZE> {
    type Output;

    // Called when a single or batch operation is completed
    fn complete(self, res: CqeResult<N>) -> Self::Output;
}

/// Extracts the `CancelData` needed to safely cancel an in-flight io_uring operation.
pub(crate) trait Cancellable {
    fn cancel(self) -> CancelData;
}

impl<T: Cancellable, const N: usize> Unpin for Op<T, N> {}

impl<T: Cancellable + Completable<N> + Send, const N: usize> Future for Op<T, N> {
    type Output = T::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let handle = &mut this.handle;
        let driver = handle.inner.driver().io();
        let state = &mut this.state;

        match state {
            State::Initialize(entry_opt) => {
                let entry = entry_opt.take().expect("Entry must be present");
                let waker = cx.waker().clone();

                // SAFETY: entry is valid for the entire duration of the operation
                match unsafe { driver.register_op(entry, waker) } {
                    Ok(idx) => this.state = State::Polled(idx),
                    Err(err) => {
                        let data = this
                            .take_data()
                            .expect("Data must be present on Initialization");

                        this.state = State::Complete;

                        return Poll::Ready(data.complete(CqeResult::InitErr(err)));
                    }
                };

                Poll::Pending
            }

            State::InitializeBatch(entries) => {
                let waker = cx.waker().clone();

                // SAFETY: entry is valid for the entire duration of the operation
                match unsafe { driver.register_batch(entries, waker) } {
                    Ok(ids) => this.state = State::PolledBatch(ids),
                    Err(err) => {
                        let data = this
                            .take_data()
                            .expect("Data must be present on Initalization");

                        this.state = State::Complete;

                        return Poll::Ready(data.complete(CqeResult::InitErr(err)));
                    }
                };

                Poll::Pending
            }

            State::PolledBatch(ids) => {
                let mut ctx = driver.get_uring().lock();
                let completed = &mut this.completed;

                for (i, idx) in ids.iter().enumerate() {
                    let lifecycle = if let Some(lifecycle) = ctx.ops.get_mut(*idx) {
                        lifecycle
                    } else {
                        continue;
                    };

                    match mem::replace(lifecycle, Lifecycle::Submitted) {
                        // Only replace the stored waker if it wouldn't wake the new one
                        Lifecycle::Waiting(prev) if !prev.will_wake(cx.waker()) => {
                            let waker = cx.waker().clone();
                            *lifecycle = Lifecycle::Waiting(waker);
                        }

                        Lifecycle::Waiting(prev) => {
                            *lifecycle = Lifecycle::Waiting(prev);
                        }

                        Lifecycle::Completed(cqe) => {
                            // Clean up and complete the future
                            ctx.remove_op(*idx);
                            // SAFETY: ids array and completed array of size N
                            // it is 0oed in the initialization
                            *unsafe { completed.get_mut(i).unwrap_unchecked() } = cqe.result();
                        }

                        Lifecycle::Submitted => {
                            unreachable!("Submitted lifecycle should never be seen here");
                        }

                        Lifecycle::Cancelled(_) => {
                            unreachable!("Cancelled lifecycle should never be seen here");
                        }
                    }
                }

                if ctx.check_slab_entry(&*ids) {
                    this.state = State::Complete;
                    drop(ctx);

                    let data = this
                        .take_data()
                        .expect("Data must be present on completion");

                    return Poll::Ready(data.complete(CqeResult::Batch(this.completed)));
                }

                Poll::Pending
            }

            State::Polled(idx) => {
                let mut ctx = driver.get_uring().lock();
                let lifecycle = ctx.ops.get_mut(*idx).expect("Lifecycle must be present");

                match mem::replace(lifecycle, Lifecycle::Submitted) {
                    // Only replace the stored waker if it wouldn't wake the new one
                    Lifecycle::Waiting(prev) if !prev.will_wake(cx.waker()) => {
                        let waker = cx.waker().clone();
                        *lifecycle = Lifecycle::Waiting(waker);
                        Poll::Pending
                    }

                    Lifecycle::Waiting(prev) => {
                        *lifecycle = Lifecycle::Waiting(prev);
                        Poll::Pending
                    }

                    Lifecycle::Completed(cqe) => {
                        // Clean up and complete the future
                        ctx.remove_op(*idx);

                        this.state = State::Complete;

                        drop(ctx);

                        let data = this
                            .take_data()
                            .expect("Data must be present on completion");

                        Poll::Ready(data.complete(CqeResult::Single(cqe_to_result(cqe))))
                    }

                    Lifecycle::Submitted => {
                        unreachable!("Submitted lifecycle should never be seen here");
                    }

                    Lifecycle::Cancelled(_) => {
                        unreachable!("Cancelled lifecycle should never be seen here");
                    }
                }
            }

            State::Complete => {
                panic!("Future polled after completion");
            }
        }
    }
}

fn cqe_to_result(cqe: cqueue::Entry) -> io::Result<u32> {
    let res = cqe.result();
    if res >= 0 {
        Ok(res as u32)
    } else {
        Err(io::Error::from_raw_os_error(-res))
    }
}
