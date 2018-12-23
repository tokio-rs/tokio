use super::list;
use semaphore::{Semaphore, Permit};

use futures::Poll;
use futures::task::AtomicTask;

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Relaxed};

/// Channel sender
pub(crate) struct Tx<T> {
    inner: Arc<Chan<T>>,
    permit: Permit,
}

/// Channel receiver
pub(crate) struct Rx<T> {
    inner: Arc<Chan<T>>,
}

struct Chan<T> {
    /// Handle to the push half of the lock-free list.
    tx: list::Tx<T>,

    /// Channel capacity
    capacity: usize,

    /// Coordinates access to channel's capacity.
    semaphore: Semaphore,

    /// Receiver task. Notified when a value is pushed into the channel.
    rx_task: AtomicTask,

    /// Tracks the number of outstanding sender handles.
    ///
    /// When this drops to zero, the send half of the channel is closed.
    tx_count: AtomicUsize,

    /// Channel receiver. This field is only accessed by the `Receiver` type.
    rx: UnsafeCell<list::Rx<T>>,
}

pub(crate) fn channel<T>(capacity: usize) -> (Tx<T>, Rx<T>) {
    let (tx, rx) = list::channel();

    let chan = Arc::new(Chan {
        tx,
        capacity,
        semaphore: Semaphore::new(capacity),
        rx_task: AtomicTask::new(),
        tx_count: AtomicUsize::new(1),
        rx: UnsafeCell::new(rx),
    });

    (Tx::new(chan.clone()), Rx::new(chan))
}

// ===== impl Tx =====

impl<T> Tx<T> {
    fn new(chan: Arc<Chan<T>>) -> Tx<T> {
        Tx {
            inner: chan,
            permit: Permit::new(),
        }
    }

    /// TODO: Docs
    pub fn poll_ready(&mut self) -> Poll<(), ()> {
        self.permit.poll_acquire(&self.inner.semaphore)
    }

    /// Send a message and notify the receiver.
    pub fn try_send(&mut self, value: T) -> Result<(), ()> {
        self.permit.try_acquire(&self.inner.semaphore)?;

        // Push the value
        self.inner.tx.push(value);

        // Notify the rx task
        self.inner.rx_task.notify();

        // Release the permit
        self.permit.forget();

        Ok(())
    }
}

impl<T> Clone for Tx<T> {
    fn clone(&self) -> Tx<T> {
        // Using a Relaxed ordering here is sufficient as the caller holds a
        // strong ref to `self`, preventing a concurrent decrement to zero.
        self.inner.tx_count.fetch_add(1, Relaxed);

        Tx {
            inner: self.inner.clone(),
            permit: Permit::new(),
        }
    }
}

impl<T> Drop for Tx<T> {
    fn drop(&mut self) {
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

impl<T> Rx<T> {
    fn new(chan: Arc<Chan<T>>) -> Rx<T> {
        Rx { inner: chan }
    }

    /// Receive the next value
    pub fn recv(&mut self, tx: &Tx<T>) -> Poll<Option<T>, ()> {
        use super::block::Read::*;
        use futures::Async::*;

        let rx = unsafe { &mut *self.inner.rx.get() };

        macro_rules! try_recv {
            () => {
                match rx.pop(&tx.inner.tx) {
                    Some(Value(value)) => {
                        self.inner.semaphore.add_permits(1);
                        return Ok(Ready(Some(value)));
                    }
                    Some(Closed) => {
                        let permits = self.inner.semaphore.available_permits();

                        if self.inner.capacity == permits {
                            return Ok(Ready(None));
                        }
                    }
                    None => {} // fall through
                }
            }
        }

        try_recv!();

        self.inner.rx_task.register();

        // It is possible that a value was pushed between attempting to read and
        // registering the task, so we have to check the channel a second time
        // here.
        try_recv!();

        Ok(NotReady)
    }
}
