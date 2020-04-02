use crate::loom::cell::UnsafeCell;
use crate::loom::future::AtomicWaker;
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Arc;
use crate::sync::mpsc::error::{ClosedError, TryRecvError};
use crate::sync::mpsc::{error, list};

use std::fmt;
use std::process;
use std::sync::atomic::Ordering::{AcqRel, Relaxed};
use std::task::Poll::{Pending, Ready};
use std::task::{Context, Poll};

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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Rx").field("inner", &self.inner).finish()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TrySendError {
    Closed,
    Full,
}

impl<T> From<(T, TrySendError)> for error::SendError<T> {
    fn from(src: (T, TrySendError)) -> error::SendError<T> {
        match src.1 {
            TrySendError::Closed => error::SendError(src.0),
            TrySendError::Full => unreachable!(),
        }
    }
}

impl<T> From<(T, TrySendError)> for error::TrySendError<T> {
    fn from(src: (T, TrySendError)) -> error::TrySendError<T> {
        match src.1 {
            TrySendError::Closed => error::TrySendError::Closed(src.0),
            TrySendError::Full => error::TrySendError::Full(src.0),
        }
    }
}

pub(crate) trait Semaphore {
    type Permit;

    fn new_permit() -> Self::Permit;

    /// The permit is dropped without a value being sent. In this case, the
    /// permit must be returned to the semaphore.
    fn drop_permit(&self, permit: &mut Self::Permit);

    fn is_idle(&self) -> bool;

    fn add_permit(&self);

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        permit: &mut Self::Permit,
    ) -> Poll<Result<(), ClosedError>>;

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

    /// Receiver waker. Notified when a value is pushed into the channel.
    rx_waker: AtomicWaker,

    /// Tracks the number of outstanding sender handles.
    ///
    /// When this drops to zero, the send half of the channel is closed.
    tx_count: AtomicUsize,

    /// Only accessed by `Rx` handle.
    rx_fields: UnsafeCell<RxFields<T>>,
}

impl<T, S> fmt::Debug for Chan<T, S>
where
    S: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Chan")
            .field("tx", &self.tx)
            .field("semaphore", &self.semaphore)
            .field("rx_waker", &self.rx_waker)
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        rx_waker: AtomicWaker::new(),
        tx_count: AtomicUsize::new(1),
        rx_fields: UnsafeCell::new(RxFields {
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

    pub(crate) fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), ClosedError>> {
        self.inner.semaphore.poll_acquire(cx, &mut self.permit)
    }

    pub(crate) fn disarm(&mut self) {
        // TODO: should this error if not acquired?
        self.inner.semaphore.drop_permit(&mut self.permit)
    }

    /// Send a message and notify the receiver.
    pub(crate) fn try_send(&mut self, value: T) -> Result<(), (T, TrySendError)> {
        self.inner.try_send(value, &mut self.permit)
    }
}

impl<T> Tx<T, (crate::sync::semaphore_ll::Semaphore, usize)> {
    pub(crate) fn is_ready(&self) -> bool {
        self.permit.is_acquired()
    }
}

impl<T> Tx<T, AtomicUsize> {
    pub(crate) fn send_unbounded(&self, value: T) -> Result<(), (T, TrySendError)> {
        self.inner.try_send(value, &mut ())
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
        self.inner.rx_waker.wake();
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
    pub(crate) fn recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        use super::block::Read::*;

        // Keep track of task budget
        ready!(crate::coop::poll_proceed(cx));

        self.inner.rx_fields.with_mut(|rx_fields_ptr| {
            let rx_fields = unsafe { &mut *rx_fields_ptr };

            macro_rules! try_recv {
                () => {
                    match rx_fields.list.pop(&self.inner.tx) {
                        Some(Value(value)) => {
                            self.inner.semaphore.add_permit();
                            return Ready(Some(value));
                        }
                        Some(Closed) => {
                            // TODO: This check may not be required as it most
                            // likely can only return `true` at this point. A
                            // channel is closed when all tx handles are
                            // dropped. Dropping a tx handle releases memory,
                            // which ensures that if dropping the tx handle is
                            // visible, then all messages sent are also visible.
                            assert!(self.inner.semaphore.is_idle());
                            return Ready(None);
                        }
                        None => {} // fall through
                    }
                };
            }

            try_recv!();

            self.inner.rx_waker.register_by_ref(cx.waker());

            // It is possible that a value was pushed between attempting to read
            // and registering the task, so we have to check the channel a
            // second time here.
            try_recv!();

            if rx_fields.rx_closed && self.inner.semaphore.is_idle() {
                Ready(None)
            } else {
                Pending
            }
        })
    }

    /// Receives the next value without blocking
    pub(crate) fn try_recv(&mut self) -> Result<T, TryRecvError> {
        use super::block::Read::*;
        self.inner.rx_fields.with_mut(|rx_fields_ptr| {
            let rx_fields = unsafe { &mut *rx_fields_ptr };
            match rx_fields.list.pop(&self.inner.tx) {
                Some(Value(value)) => {
                    self.inner.semaphore.add_permit();
                    Ok(value)
                }
                Some(Closed) => Err(TryRecvError::Closed),
                None => Err(TryRecvError::Empty),
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

impl<T, S> Chan<T, S>
where
    S: Semaphore,
{
    fn try_send(&self, value: T, permit: &mut S::Permit) -> Result<(), (T, TrySendError)> {
        if let Err(e) = self.semaphore.try_acquire(permit) {
            return Err((value, e));
        }

        // Push the value
        self.tx.push(value);

        // Notify the rx task
        self.rx_waker.wake();

        // Release the permit
        self.semaphore.forget(permit);

        Ok(())
    }
}

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

use crate::sync::semaphore_ll::TryAcquireError;

impl From<TryAcquireError> for TrySendError {
    fn from(src: TryAcquireError) -> TrySendError {
        if src.is_closed() {
            TrySendError::Closed
        } else if src.is_no_permits() {
            TrySendError::Full
        } else {
            unreachable!();
        }
    }
}

// ===== impl Semaphore for (::Semaphore, capacity) =====

use crate::sync::semaphore_ll::Permit;

impl Semaphore for (crate::sync::semaphore_ll::Semaphore, usize) {
    type Permit = Permit;

    fn new_permit() -> Permit {
        Permit::new()
    }

    fn drop_permit(&self, permit: &mut Permit) {
        permit.release(1, &self.0);
    }

    fn add_permit(&self) {
        self.0.add_permits(1)
    }

    fn is_idle(&self) -> bool {
        self.0.available_permits() == self.1
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        permit: &mut Permit,
    ) -> Poll<Result<(), ClosedError>> {
        // Keep track of task budget
        ready!(crate::coop::poll_proceed(cx));

        permit
            .poll_acquire(cx, 1, &self.0)
            .map_err(|_| ClosedError::new())
    }

    fn try_acquire(&self, permit: &mut Permit) -> Result<(), TrySendError> {
        permit.try_acquire(1, &self.0)?;
        Ok(())
    }

    fn forget(&self, permit: &mut Self::Permit) {
        permit.forget(1);
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

    fn poll_acquire(
        &self,
        _cx: &mut Context<'_>,
        permit: &mut (),
    ) -> Poll<Result<(), ClosedError>> {
        Ready(self.try_acquire(permit).map_err(|_| ClosedError::new()))
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
