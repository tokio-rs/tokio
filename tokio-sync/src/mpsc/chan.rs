use super::list;
use futures::Poll;

use loom::{
    futures::AtomicTask,
    sync::atomic::AtomicUsize,
    sync::{Arc, CausalCell},
};

use std::fmt;
use std::process;
use std::sync::atomic::Ordering::{AcqRel, Relaxed};

/// Channel sender
pub(crate) struct Tx<T, S: Semaphore> {
    inner: Arc<Chan<T, S>>,
    permit: S::Permit,
}

impl<T, S: Semaphore> fmt::Debug for Tx<T, S>
where
    S::Permit: fmt::Debug,
    S: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Tx")
            .field("inner", &self.inner)
            .field("permit", &self.permit)
            .finish()
    }
}

/// Channel receiver
pub(crate) struct Rx<T, S: Semaphore> {
    inner: Arc<Chan<T, S>>,
}

impl<T, S: Semaphore> fmt::Debug for Rx<T, S>
where
    S: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Rx").field("inner", &self.inner).finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TrySendError {
    Closed,
    NoPermits,
}

pub(crate) trait Semaphore {
    type Permit;

    fn new_permit() -> Self::Permit;

    /// The permit is dropped without a value being sent. In this case, the
    /// permit must be returned to the semaphore.
    fn drop_permit(&self, permit: &mut Self::Permit);

    fn is_idle(&self) -> bool;

    fn add_permit(&self);

    fn poll_acquire(&self, permit: &mut Self::Permit) -> Poll<(), ()>;

    fn try_acquire(&self, permit: &mut Self::Permit) -> Result<(), TrySendError>;

    /// A value was sent into the channel and the permit held by `tx` is
    /// dropped. In this case, the permit should not immeditely be returned to
    /// the semaphore. Instead, the permit is returnred to the semaphore once
    /// the sent value is read by the rx handle.
    fn forget(&self, permit: &mut Self::Permit);

    fn close(&self);
}

struct Chan<T, S> {
    /// Handle to the push half of the lock-free list.
    tx: list::Tx<T>,

    /// Coordinates access to channel's capacity.
    semaphore: S,

    /// Receiver task. Notified when a value is pushed into the channel.
    rx_task: AtomicTask,

    /// Tracks the number of outstanding sender handles.
    ///
    /// When this drops to zero, the send half of the channel is closed.
    tx_count: AtomicUsize,

    /// Only accessed by `Rx` handle.
    rx_fields: CausalCell<RxFields<T>>,
}

impl<T, S> fmt::Debug for Chan<T, S>
where
    S: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Chan")
            .field("tx", &self.tx)
            .field("semaphore", &self.semaphore)
            .field("rx_task", &self.rx_task)
            .field("tx_count", &self.tx_count)
            .field("rx_fields", &"...")
            .finish()
    }
}

/// Fields only accessed by `Rx` handle.
struct RxFields<T> {
    /// Channel receiver. This field is only accessed by the `Receiver` type.
    list: list::Rx<T>,

    /// `true` if `Rx::close` is called.
    rx_closed: bool,
}

impl<T> fmt::Debug for RxFields<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("RxFields")
            .field("list", &self.list)
            .field("rx_closed", &self.rx_closed)
            .finish()
    }
}

unsafe impl<T: Send, S: Send> Send for Chan<T, S> {}
unsafe impl<T: Send, S: Sync> Sync for Chan<T, S> {}

pub(crate) fn channel<T, S>(semaphore: S) -> (Tx<T, S>, Rx<T, S>)
where
    S: Semaphore,
{
    let (tx, rx) = list::channel();

    let chan = Arc::new(Chan {
        tx,
        semaphore,
        rx_task: AtomicTask::new(),
        tx_count: AtomicUsize::new(1),
        rx_fields: CausalCell::new(RxFields {
            list: rx,
            rx_closed: false,
        }),
    });

    (Tx::new(chan.clone()), Rx::new(chan))
}

// ===== impl Tx =====

impl<T, S> Tx<T, S>
where
    S: Semaphore,
{
    fn new(chan: Arc<Chan<T, S>>) -> Tx<T, S> {
        Tx {
            inner: chan,
            permit: S::new_permit(),
        }
    }

    /// TODO: Docs
    pub(crate) fn poll_ready(&mut self) -> Poll<(), ()> {
        self.inner.semaphore.poll_acquire(&mut self.permit)
    }

    /// Send a message and notify the receiver.
    pub(crate) fn try_send(&mut self, value: T) -> Result<(), (T, TrySendError)> {
        if let Err(e) = self.inner.semaphore.try_acquire(&mut self.permit) {
            return Err((value, e));
        }

        // Push the value
        self.inner.tx.push(value);

        // Notify the rx task
        self.inner.rx_task.notify();

        // Release the permit
        self.inner.semaphore.forget(&mut self.permit);

        Ok(())
    }
}

impl<T, S> Clone for Tx<T, S>
where
    S: Semaphore,
{
    fn clone(&self) -> Tx<T, S> {
        // Using a Relaxed ordering here is sufficient as the caller holds a
        // strong ref to `self`, preventing a concurrent decrement to zero.
        self.inner.tx_count.fetch_add(1, Relaxed);

        Tx {
            inner: self.inner.clone(),
            permit: S::new_permit(),
        }
    }
}

impl<T, S> Drop for Tx<T, S>
where
    S: Semaphore,
{
    fn drop(&mut self) {
        self.inner.semaphore.drop_permit(&mut self.permit);

        if self.inner.tx_count.fetch_sub(1, AcqRel) != 1 {
            return;
        }

        // Close the list, which sends a `Close` message
        self.inner.tx.close();

        // Notify the receiver
        self.inner.rx_task.notify();
    }
}

// ===== impl Rx =====

impl<T, S> Rx<T, S>
where
    S: Semaphore,
{
    fn new(chan: Arc<Chan<T, S>>) -> Rx<T, S> {
        Rx { inner: chan }
    }

    pub(crate) fn close(&mut self) {
        self.inner.rx_fields.with_mut(|rx_fields_ptr| {
            let rx_fields = unsafe { &mut *rx_fields_ptr };

            if rx_fields.rx_closed {
                return;
            }

            rx_fields.rx_closed = true;
        });

        self.inner.semaphore.close();
    }

    /// Receive the next value
    pub(crate) fn recv(&mut self) -> Poll<Option<T>, ()> {
        use super::block::Read::*;
        use futures::Async::*;

        self.inner.rx_fields.with_mut(|rx_fields_ptr| {
            let rx_fields = unsafe { &mut *rx_fields_ptr };

            macro_rules! try_recv {
                () => {
                    match rx_fields.list.pop(&self.inner.tx) {
                        Some(Value(value)) => {
                            self.inner.semaphore.add_permit();
                            return Ok(Ready(Some(value)));
                        }
                        Some(Closed) => {
                            // TODO: This check may not be required as it most
                            // likely can only return `true` at this point. A
                            // channel is closed when all tx handles are
                            // dropped.  Dropping a tx handle releases memory,
                            // which ensures that if dropping the tx handle is
                            // visible, then all messages sent are also visible.
                            assert!(self.inner.semaphore.is_idle());
                            return Ok(Ready(None));
                        }
                        None => {} // fall through
                    }
                };
            }

            try_recv!();

            self.inner.rx_task.register();

            // It is possible that a value was pushed between attempting to read
            // and registering the task, so we have to check the channel a
            // second time here.
            try_recv!();

            debug!(
                "recv; rx_closed = {:?}; is_idle = {:?}",
                rx_fields.rx_closed,
                self.inner.semaphore.is_idle()
            );

            if rx_fields.rx_closed && self.inner.semaphore.is_idle() {
                Ok(Ready(None))
            } else {
                Ok(NotReady)
            }
        })
    }
}

impl<T, S> Drop for Rx<T, S>
where
    S: Semaphore,
{
    fn drop(&mut self) {
        use super::block::Read::Value;

        self.close();

        self.inner.rx_fields.with_mut(|rx_fields_ptr| {
            let rx_fields = unsafe { &mut *rx_fields_ptr };

            while let Some(Value(_)) = rx_fields.list.pop(&self.inner.tx) {
                self.inner.semaphore.add_permit();
            }
        })
    }
}

// ===== impl Chan =====

impl<T, S> Drop for Chan<T, S> {
    fn drop(&mut self) {
        use super::block::Read::Value;

        // Safety: the only owner of the rx fields is Chan, and eing
        // inside its own Drop means we're the last ones to touch it.
        self.rx_fields.with_mut(|rx_fields_ptr| {
            let rx_fields = unsafe { &mut *rx_fields_ptr };

            while let Some(Value(_)) = rx_fields.list.pop(&self.tx) {}
            unsafe { rx_fields.list.free_blocks() };
        });
    }
}

use semaphore::TryAcquireError;

impl From<TryAcquireError> for TrySendError {
    fn from(src: TryAcquireError) -> TrySendError {
        if src.is_closed() {
            TrySendError::Closed
        } else if src.is_no_permits() {
            TrySendError::NoPermits
        } else {
            unreachable!();
        }
    }
}

// ===== impl Semaphore for (::Semaphore, capacity) =====

use semaphore::Permit;

impl Semaphore for (::semaphore::Semaphore, usize) {
    type Permit = Permit;

    fn new_permit() -> Permit {
        Permit::new()
    }

    fn drop_permit(&self, permit: &mut Permit) {
        permit.release(&self.0);
    }

    fn add_permit(&self) {
        self.0.add_permits(1)
    }

    fn is_idle(&self) -> bool {
        self.0.available_permits() == self.1
    }

    fn poll_acquire(&self, permit: &mut Permit) -> Poll<(), ()> {
        permit.poll_acquire(&self.0).map_err(|_| ())
    }

    fn try_acquire(&self, permit: &mut Permit) -> Result<(), TrySendError> {
        permit.try_acquire(&self.0)?;
        Ok(())
    }

    fn forget(&self, permit: &mut Self::Permit) {
        permit.forget()
    }

    fn close(&self) {
        self.0.close();
    }
}

// ===== impl Semaphore for AtomicUsize =====

use std::sync::atomic::Ordering::{Acquire, Release};
use std::usize;

impl Semaphore for AtomicUsize {
    type Permit = ();

    fn new_permit() {}

    fn drop_permit(&self, _permit: &mut ()) {}

    fn add_permit(&self) {
        let prev = self.fetch_sub(2, Release);

        if prev >> 1 == 0 {
            // Something went wrong
            process::abort();
        }
    }

    fn is_idle(&self) -> bool {
        self.load(Acquire) >> 1 == 0
    }

    fn poll_acquire(&self, permit: &mut ()) -> Poll<(), ()> {
        use futures::Async::Ready;
        self.try_acquire(permit).map(Ready).map_err(|_| ())
    }

    fn try_acquire(&self, _permit: &mut ()) -> Result<(), TrySendError> {
        let mut curr = self.load(Acquire);

        loop {
            if curr & 1 == 1 {
                return Err(TrySendError::Closed);
            }

            if curr == usize::MAX ^ 1 {
                // Overflowed the ref count. There is no safe way to recover, so
                // abort the process. In practice, this should never happen.
                process::abort()
            }

            match self.compare_exchange(curr, curr + 2, AcqRel, Acquire) {
                Ok(_) => return Ok(()),
                Err(actual) => {
                    curr = actual;
                }
            }
        }
    }

    fn forget(&self, _permit: &mut ()) {}

    fn close(&self) {
        self.fetch_or(1, Release);
    }
}
