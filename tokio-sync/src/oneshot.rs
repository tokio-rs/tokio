
use loom::{
    futures::task::{self, Task},
    sync::CausalCell,
    sync::atomic::AtomicUsize,
};

use futures::{Async, Future, Poll};

use std::cell::UnsafeCell;
use std::mem::{self, ManuallyDrop};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic;
use std::sync::atomic::Ordering::{self, Acquire, Release, AcqRel};

pub struct Sender<T> {
    inner: Arc<Inner<T>>,
}

pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
}

#[derive(Debug)]
pub struct Error {}

struct Inner<T> {
    /// Manages the state of the inner cell
    state: AtomicUsize,

    /// The value. This is set by `Sender` and read by `Receiver`. The state of
    /// the cell is tracked by `state`.
    value: CausalCell<ManuallyDrop<T>>,

    /// The task to notify when the receiver drops without consuming the value.
    tx_task: CausalCell<ManuallyDrop<Task>>,

    /// The task to notify when the value is sent.
    rx_task: CausalCell<ManuallyDrop<Task>>,

    /// Fields only accessed by receiver
    rx_fields: UnsafeCell<RxFields>,
}

struct RxFields {
    consumed: bool,
}

/// States:
///
/// * rx_task_set
/// * value complete: once set, rx_task can no longer be modified.
#[derive(Clone, Copy)]
struct State(usize);

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        state: AtomicUsize::new(State::new().as_usize()),
        value: CausalCell::new(ManuallyDrop::new(unsafe { mem::uninitialized() })),
        tx_task: CausalCell::new(ManuallyDrop::new(unsafe { mem::uninitialized() })),
        rx_task: CausalCell::new(ManuallyDrop::new(unsafe { mem::uninitialized() })),
        rx_fields: UnsafeCell::new(RxFields {
            consumed: false,
        }),
    });

    let tx = Sender { inner: inner.clone() };
    let rx = Receiver { inner };

    (tx, rx)
}

impl<T> Sender<T> {
    pub fn send(self, t: T) -> Result<(), T> {
        self.inner.value.with_mut(|ptr| {
            unsafe { ptr::write(ptr, ManuallyDrop::new(t)); }
        });

        let prev = State::set_complete(&self.inner.state);

        if prev.is_rx_task_set() {
            let rx_task = unsafe { self.inner.rx_task() };
            rx_task.notify();
        }

        Ok(())
    }

    pub fn poll_cancel(&mut self) -> Poll<(), ()> {
        unimplemented!();
    }

    pub fn is_canceled(&self) -> bool {
        unimplemented!();
    }
}

impl<T> Receiver<T> {
    pub fn close(&mut self) {
        unimplemented!();
    }

    pub fn try_recv(&mut self) -> Result<Option<T>, Error> {
        unimplemented!();
    }

    /// Consume the value. This function does not check `state`.
    unsafe fn consume_value(&mut self) -> T {
        let rx_fields = &mut *self.inner.rx_fields.get();

        assert!(!rx_fields.consumed);
        rx_fields.consumed = true;

        self.inner.value.with(|ptr| {
            ManuallyDrop::into_inner(ptr::read(ptr))
        })
    }

    unsafe fn set_rx_task(&self) {
        self.inner.rx_task.with_mut(|ptr| {
            *ptr = ManuallyDrop::new(task::current())
        });
    }
}

impl<T> Future for Receiver<T> {
    type Item = T;
    type Error = Error;

    fn poll(&mut self) -> Poll<T, Error> {
        let mut state = State::load(&self.inner.state, Acquire);

        if state.is_complete() {
            let value = unsafe { self.consume_value() };
            return Ok(Async::Ready(value));
        }

        if state.is_rx_task_set() {
            let rx_task = unsafe { self.inner.rx_task() };
            // Check if the task is still the same
            if !rx_task.will_notify_current() {
                // Unset the task
                unimplemented!();
            }
        }

        if !state.is_rx_task_set() {
            // Attempt to set the task
            unsafe { self.set_rx_task(); }

            // Update the state
            state = State::set_rx_task(&self.inner.state);

            if state.is_complete() {
                let value = unsafe { self.consume_value() };
                return Ok(Async::Ready(value));
            }
        }

        Ok(Async::NotReady)
    }
}

impl<T> Inner<T> {
    unsafe fn rx_task(&self) -> &Task {
        &*self.rx_task.with(|ptr| ptr)
    }
}

const RX_TASK_SET: usize = 0b0001;
const VALUE_SENT: usize  = 0b0010;

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
        State(val)
    }

    fn as_usize(self) -> usize {
        self.0
    }

    fn load(cell: &AtomicUsize, order: Ordering) -> State {
        let val = cell.load(order);
        State(val)
    }
}
