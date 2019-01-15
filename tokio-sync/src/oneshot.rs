//! A channel for sending a single message between asynchronous tasks.

use loom::{
    futures::task::{self, Task},
    sync::CausalCell,
    sync::atomic::AtomicUsize,
};

use futures::{Async, Future, Poll};

use std::fmt;
use std::mem::{self, ManuallyDrop};
use std::sync::Arc;
use std::sync::atomic::Ordering::{self, Acquire, AcqRel};

/// Sends a value to the associated `Receiver`.
///
/// Instances are created by the [`channel`](fn.channel.html) function.
#[derive(Debug)]
pub struct Sender<T> {
    inner: Option<Arc<Inner<T>>>,
}

/// Receive a value from the associated `Sender`.
///
/// Instances are created by the [`channel`](fn.channel.html) function.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: Option<Arc<Inner<T>>>,
}

pub mod error {
    //! Oneshot error types

    /// Error returned by the `Future` implementation for `Receiver`.
    #[derive(Debug)]
    pub struct RecvError(pub(super) ());

    /// Error returned by the `try_recv` function on `Receiver`.
    #[derive(Debug)]
    pub struct TryRecvError(pub(super) ());
}

use self::error::*;

struct Inner<T> {
    /// Manages the state of the inner cell
    state: AtomicUsize,

    /// The value. This is set by `Sender` and read by `Receiver`. The state of
    /// the cell is tracked by `state`.
    value: CausalCell<Option<T>>,

    /// The task to notify when the receiver drops without consuming the value.
    tx_task: CausalCell<ManuallyDrop<Task>>,

    /// The task to notify when the value is sent.
    rx_task: CausalCell<ManuallyDrop<Task>>,
}

#[derive(Clone, Copy)]
struct State(usize);

/// Create a new one-shot channel for sending single values across asynchronous
/// tasks.
///
/// The function returns separate "send" and "receive" handles. The `Sender`
/// handle is used by the producer to send the value. The `Receiver` handle is
/// used by the consumer to receive the value.
///
/// Each handle can be used on separate tasks.
///
/// # Examples
///
/// ```
/// extern crate futures;
/// extern crate tokio;
///
/// use tokio::sync::oneshot;
/// use futures::Future;
/// use std::thread;
///
/// let (sender, receiver) = oneshot::channel::<i32>();
///
/// # let t =
/// thread::spawn(|| {
///     let future = receiver.map(|i| {
///         println!("got: {:?}", i);
///     });
///     // ...
/// # return future;
/// });
///
/// sender.send(3).unwrap();
/// # t.join().unwrap().wait().unwrap();
/// ```
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        state: AtomicUsize::new(State::new().as_usize()),
        value: CausalCell::new(None),
        tx_task: CausalCell::new(ManuallyDrop::new(unsafe { mem::uninitialized() })),
        rx_task: CausalCell::new(ManuallyDrop::new(unsafe { mem::uninitialized() })),
    });

    let tx = Sender { inner: Some(inner.clone()) };
    let rx = Receiver { inner: Some(inner) };

    (tx, rx)
}

impl<T> Sender<T> {
    /// Completes this oneshot with a successful result.
    ///
    /// The function consumes `self` and notifies the `Receiver` handle that a
    /// value is ready to be received.
    ///
    /// If the value is successfully enqueued for the remote end to receive,
    /// then `Ok(())` is returned. If the receiving end was dropped before this
    /// function was called, however, then `Err` is returned with the value
    /// provided.
    pub fn send(mut self, t: T) -> Result<(), T> {
        let inner = self.inner.take().unwrap();

        inner.value.with_mut(|ptr| {
            unsafe { *ptr = Some(t); }
        });

        if !inner.complete() {
            return Err(inner.value.with_mut(|ptr| {
                unsafe { (*ptr).take() }.unwrap()
            }));
        }

        Ok(())
    }

    /// Check if the associated [`Receiver`] handle has been dropped.
    ///
    /// # Return values
    ///
    /// If `Ok(Ready)` is returned then the associated `Receiver` has been
    /// dropped, which means any work required for sending should be canceled.
    ///
    /// If `Ok(NotReady)` is returned then the associated `Receiver` is still
    /// alive and may be able to receive a message if sent. The current task is
    /// registered to receive a notification if the `Receiver` handle goes away.
    ///
    /// [`Receiver`]: struct.Receiver.html
    pub fn poll_close(&mut self) -> Poll<(), ()> {
        let inner = self.inner.as_ref().unwrap();

        let mut state = State::load(&inner.state, Acquire);

        if state.is_closed() {
            return Ok(Async::Ready(()));
        }

        if state.is_tx_task_set() {
            let tx_task = unsafe { inner.tx_task() };

            if !tx_task.will_notify_current() {
                state = State::unset_tx_task(&inner.state);

                if state.is_closed() {
                    return Ok(Async::Ready(()));
                }
            }
        }

        if !state.is_tx_task_set() {
            // Attempt to set the task
            unsafe { inner.set_tx_task(); }

            // Update the state
            state = State::set_tx_task(&inner.state);

            if state.is_closed() {
                return Ok(Async::Ready(()));
            }
        }

        Ok(Async::NotReady)
    }


    /// Check if the associated [`Receiver`] handle has been dropped.
    ///
    /// Unlike [`poll_close`], this function does not register a task for
    /// wakeup upon close.
    ///
    /// [`Receiver`]: struct.Receiver.html
    /// [`poll_close`]: struct.Sender.html#method.poll_close
    pub fn is_closed(&self) -> bool {
        let inner = self.inner.as_ref().unwrap();

        let state = State::load(&inner.state, Acquire);
        state.is_closed()
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.as_ref() {
            inner.complete();
        }
    }
}

impl<T> Receiver<T> {
    /// Prevent the associated [`Sender`] handle from sending a value.
    ///
    /// Any `send` operation which happens after calling `close` is guaranteed
    /// to fail. After calling `close`, `Receiver::poll`] should be called to
    /// receive a value if one was sent **before** the call to `close`
    /// completed.
    ///
    /// [`Sender`]: struct.Sender.html
    pub fn close(&mut self) {
        let inner = self.inner.as_ref().unwrap();
        inner.close();
    }

    /// Attempts to receive a value outside of the context of a task.
    ///
    /// Does not register a task if no value has been sent.
    ///
    /// A return value of `None` must be considered immediately stale (out of
    /// date) unless [`close`] has been called first.
    ///
    /// Returns an error if the sender was dropped.
    ///
    /// [`close`]: #method.close
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let result = if let Some(inner) = self.inner.as_ref() {
            let state = State::load(&inner.state, Acquire);

            if state.is_complete() {
                match unsafe { inner.consume_value() } {
                    Some(value) => Ok(value),
                    None => Err(TryRecvError(())),
                }
            } else if state.is_closed() {
                Err(TryRecvError(()))
            } else {
                // Not ready, this does not clear `inner`
                return Err(TryRecvError(()));
            }
        } else {
            panic!("called after complete");
        };

        self.inner = None;
        result
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.as_ref() {
            inner.close();
        }
    }
}

impl<T> Future for Receiver<T> {
    type Item = T;
    type Error = RecvError;

    fn poll(&mut self) -> Poll<T, RecvError> {
        use futures::Async::{Ready, NotReady};

        // If `inner` is `None`, then `poll()` has already completed.
        let ret = if let Some(inner) = self.inner.as_ref() {
            match inner.poll_recv() {
                Ok(Ready(v)) => Ok(Ready(v)),
                Ok(NotReady) => return Ok(NotReady),
                Err(e) => Err(e),
            }
        } else {
            panic!("called after complete");
        };

        self.inner = None;
        ret
    }
}

impl<T> Inner<T> {
    fn complete(&self) -> bool {
        let prev = State::set_complete(&self.state);

        if prev.is_closed() {
            return false;
        }

        if prev.is_rx_task_set() {
            let rx_task = unsafe { self.rx_task() };
            rx_task.notify();
        }

        true
    }

    fn poll_recv(&self) -> Poll<T, RecvError> {
        use futures::Async::{Ready, NotReady};

        // Load the state
        let mut state = State::load(&self.state, Acquire);

        if state.is_complete() {
            match unsafe { self.consume_value() } {
                Some(value) => Ok(Ready(value)),
                None => Err(RecvError(())),
            }
        } else if state.is_closed() {
            Err(RecvError(()))
        } else {
            if state.is_rx_task_set() {
                let rx_task = unsafe { self.rx_task() };

                // Check if the task is still the same
                if !rx_task.will_notify_current() {
                    // Unset the task
                    state = State::unset_rx_task(&self.state);

                    if state.is_complete() {
                        return match unsafe { self.consume_value() } {
                            Some(value) => Ok(Ready(value)),
                            None => Err(RecvError(())),
                        };
                    }
                }
            }

            if !state.is_rx_task_set() {
                // Attempt to set the task
                unsafe { self.set_rx_task(); }

                // Update the state
                state = State::set_rx_task(&self.state);

                if state.is_complete() {
                    match unsafe { self.consume_value() } {
                        Some(value) => Ok(Ready(value)),
                        None => Err(RecvError(())),
                    }
                } else {
                    return Ok(NotReady);
                }
            } else {
                return Ok(NotReady);
            }
        }
    }

    /// Called by `Receiver` to indicate that the value will never be received.
    fn close(&self) {
        let prev = State::set_closed(&self.state);

        if prev.is_tx_task_set() && !prev.is_complete() {
            let tx_task = unsafe { self.tx_task() };
            tx_task.notify();
        }
    }

    /// Consume the value. This function does not check `state`.
    unsafe fn consume_value(&self) -> Option<T> {
        self.value.with_mut(|ptr| {
            (*ptr).take()
        })
    }

    unsafe fn rx_task(&self) -> &Task {
        &*self.rx_task.with(|ptr| ptr)
    }

    unsafe fn set_rx_task(&self) {
        self.rx_task.with_mut(|ptr| {
            *ptr = ManuallyDrop::new(task::current())
        });
    }

    unsafe fn tx_task(&self) -> &Task {
        &*self.tx_task.with(|ptr| ptr)
    }

    unsafe fn set_tx_task(&self) {
        self.tx_task.with_mut(|ptr| {
            *ptr = ManuallyDrop::new(task::current())
        });
    }
}

unsafe impl<T: Send> Send for Inner<T> {}
unsafe impl<T: Send> Sync for Inner<T> {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let state = State(*self.state.get_mut());

        if state.is_rx_task_set() {
            self.rx_task.with_mut(|ptr| {
                unsafe {
                    ManuallyDrop::drop(&mut *ptr);
                }
            });
        }

        if state.is_tx_task_set() {
            self.tx_task.with_mut(|ptr| {
                unsafe {
                    ManuallyDrop::drop(&mut *ptr);
                }
            });
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Inner<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::sync::atomic::Ordering::Relaxed;

        fmt.debug_struct("Inner")
            .field("state", &State::load(&self.state, Relaxed))
            .finish()
    }
}

const RX_TASK_SET: usize = 0b00001;
const VALUE_SENT: usize  = 0b00010;
const CLOSED: usize      = 0b00100;
const TX_TASK_SET: usize = 0b01000;

impl State {
    fn new() -> State {
        State(0)
    }

    fn is_complete(&self) -> bool {
        self.0 & VALUE_SENT == VALUE_SENT
    }

    fn set_complete(cell: &AtomicUsize) -> State {
        // TODO: This could be `Release`, followed by an `Acquire` fence *if*
        // the `RX_TASK_SET` flag is set. However, `loom` does not support
        // fences yet.
        let val = cell.fetch_or(VALUE_SENT, AcqRel);
        State(val)
    }

    fn is_rx_task_set(&self) -> bool {
        self.0 & RX_TASK_SET == RX_TASK_SET
    }

    fn set_rx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_or(RX_TASK_SET, AcqRel);
        State(val | RX_TASK_SET)
    }

    fn unset_rx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_and(!RX_TASK_SET, AcqRel);
        State(val & !RX_TASK_SET)
    }

    fn is_closed(&self) -> bool {
        self.0 & CLOSED == CLOSED
    }

    fn set_closed(cell: &AtomicUsize) -> State {
        // Acquire because we want all later writes (attempting to poll) to be
        // ordered after this.
        let val = cell.fetch_or(CLOSED, Acquire);
        State(val)
    }

    fn set_tx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_or(TX_TASK_SET, AcqRel);
        State(val | TX_TASK_SET)
    }

    fn unset_tx_task(cell: &AtomicUsize) -> State {
        let val = cell.fetch_and(!TX_TASK_SET, AcqRel);
        State(val & !TX_TASK_SET)
    }

    fn is_tx_task_set(&self) -> bool {
        self.0 & TX_TASK_SET == TX_TASK_SET
    }

    fn as_usize(self) -> usize {
        self.0
    }

    fn load(cell: &AtomicUsize, order: Ordering) -> State {
        let val = cell.load(order);
        State(val)
    }
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("State")
            .field("is_complete", &self.is_complete())
            .field("is_closed", &self.is_closed())
            .field("is_rx_task_set", &self.is_rx_task_set())
            .field("is_tx_task_set", &self.is_tx_task_set())
            .finish()
    }
}
