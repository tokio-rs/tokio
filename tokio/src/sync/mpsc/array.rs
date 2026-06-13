//! A concurrent, lock-free, FIFO ring buffer for bounded channels.
//!
//! The semaphore in `bounded.rs` guarantees that senders only claim a slot when
//! the ring has capacity, so receiver-side slot reuse is coordinated by permit
//! release rather than by allocating new blocks.

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Arc;
use crate::loom::thread;
use crate::sync::mpsc::block::Read;
use crate::sync::mpsc::{chan, TryPopResult};

use std::fmt;
use std::mem::MaybeUninit;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};

const EMPTY: usize = 0;
const WRITING: usize = 1;
const READY: usize = 2;
const OPEN: usize = 0;
const CLOSED: usize = 1;

pub(crate) struct Tx<T> {
    /// Shared ring buffer slots.
    shared: Arc<Shared<T>>,

    /// Position to push the next message.
    tail_position: AtomicUsize,

    /// Tracks whether all sender handles have been dropped.
    closed: AtomicUsize,
}

pub(crate) struct Rx<T> {
    /// Shared ring buffer slots.
    shared: Arc<Shared<T>>,

    /// Next slot index to process.
    index: usize,
}

#[derive(Debug)]
pub(crate) struct Queue;

struct Shared<T> {
    /// Ring buffer slots.
    slots: Box<[Slot<T>]>,

    /// Number of slots in the ring buffer.
    capacity: usize,
}

struct Slot<T> {
    /// Tracks whether `value` is empty, being written, or ready to read.
    ready: AtomicUsize,

    /// Storage for the slot value.
    value: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T: Send> Send for Shared<T> {}
unsafe impl<T: Send> Sync for Shared<T> {}

impl<T> chan::Queue<T> for Queue {
    type Tx = Tx<T>;
    type Rx = Rx<T>;

    fn channel(bound: usize) -> (Self::Tx, Self::Rx) {
        channel(bound)
    }

    fn push(tx: &Self::Tx, value: T) {
        tx.push(value);
    }

    fn close(tx: &Self::Tx) {
        tx.close();
    }

    fn is_empty(rx: &Self::Rx, tx: &Self::Tx) -> bool {
        rx.is_empty(tx)
    }

    fn len(rx: &Self::Rx, tx: &Self::Tx) -> usize {
        rx.len(tx)
    }

    fn pop(rx: &mut Self::Rx, tx: &Self::Tx) -> Option<Read<T>> {
        rx.pop(tx)
    }

    fn try_pop(rx: &mut Self::Rx, tx: &Self::Tx) -> TryPopResult<T> {
        rx.try_pop(tx)
    }

    unsafe fn free(_rx: &mut Self::Rx) {
        // The array queue owns its slots through `Arc<Shared<T>>`, so there are
        // no receiver-owned blocks to free.
    }
}

pub(crate) fn channel<T>(capacity: usize) -> (Tx<T>, Rx<T>) {
    let slots = (0..capacity)
        .map(|_| Slot::new())
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let shared = Arc::new(Shared { slots, capacity });

    let tx = Tx {
        shared: shared.clone(),
        tail_position: AtomicUsize::new(0),
        closed: AtomicUsize::new(OPEN),
    };

    let rx = Rx { shared, index: 0 };

    (tx, rx)
}

impl<T> Tx<T> {
    /// Pushes a value into the ring buffer.
    pub(crate) fn push(&self, value: T) {
        // First, claim a slot for the value. The bounded channel semaphore
        // ensures that this slot is empty before it is reused.
        let slot_index = self.tail_position.fetch_add(1, Acquire);
        let slot = self.shared.slot(slot_index);

        while !slot.claim() {
            thread::yield_now();
        }

        slot.write(value);
    }

    /// Closes the send half of the queue.
    ///
    /// Unlike the list queue, the array queue does not need to push a fake close
    /// message. The receiver can observe this flag once it has caught up to the
    /// sender tail position.
    pub(crate) fn close(&self) {
        self.closed.store(CLOSED, Release);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Acquire) == CLOSED
    }
}

impl<T> fmt::Debug for Tx<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Tx")
            .field("tail_position", &self.tail_position.load(Relaxed))
            .field("closed", &(self.closed.load(Relaxed) == CLOSED))
            .finish()
    }
}

impl<T> Rx<T> {
    pub(crate) fn is_empty(&self, tx: &Tx<T>) -> bool {
        if self.shared.slot(self.index).is_ready() {
            return false;
        }

        // It is possible that the head slot has no value "now" but the queue is
        // still not empty. Another sender may have claimed the head slot and not
        // marked it ready yet, while a later sender has already finished.
        self.len(tx) == 0
    }

    pub(crate) fn len(&self, tx: &Tx<T>) -> usize {
        let tail = tx.tail_position.load(Acquire);
        let mut len = tail.wrapping_sub(self.index);

        if len == 0 {
            return 0;
        }

        // There are messages present in the queue. However, it's possible that
        // the last message is currently being written and not ready yet. It is
        // optional to count messages that are currently being sent, so we do not
        // count the last message if its ready bit is unset.
        if !self.shared.slot(tail.wrapping_sub(1)).is_ready() {
            len -= 1;
        }

        len
    }

    /// Pops the next value off the queue.
    pub(crate) fn pop(&mut self, tx: &Tx<T>) -> Option<Read<T>> {
        let tail = tx.tail_position.load(Acquire);

        if self.index == tail {
            return if tx.is_closed() {
                Some(Read::Closed)
            } else {
                None
            };
        }

        let slot = self.shared.slot(self.index);
        if !slot.is_ready() {
            return None;
        }

        let value = unsafe { slot.take() };
        self.index = self.index.wrapping_add(1);
        Some(Read::Value(value))
    }

    /// Pops the next value off the queue, detecting whether the queue is busy or
    /// empty on failure.
    ///
    /// This function exists because `Rx::pop` can return `None` even if the
    /// queue contains a message that has been completely written. This can
    /// happen if the fully delivered message is behind another message that is
    /// in the middle of being written, since the channel can't return messages
    /// out of order.
    pub(crate) fn try_pop(&mut self, tx: &Tx<T>) -> TryPopResult<T> {
        let tail = tx.tail_position.load(Acquire);

        if self.index == tail {
            return if tx.is_closed() {
                TryPopResult::Closed
            } else {
                TryPopResult::Empty
            };
        }

        let slot = self.shared.slot(self.index);
        if !slot.is_ready() {
            return TryPopResult::Busy;
        }

        let value = unsafe { slot.take() };
        self.index = self.index.wrapping_add(1);
        TryPopResult::Ok(value)
    }
}

impl<T> fmt::Debug for Rx<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Rx")
            .field("index", &self.index)
            .field("capacity", &self.shared.capacity)
            .finish()
    }
}

impl<T> Shared<T> {
    /// Returns the slot for an absolute queue position.
    fn slot(&self, position: usize) -> &Slot<T> {
        &self.slots[position % self.capacity]
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        for slot in &self.slots {
            if slot.is_ready() {
                unsafe { slot.drop_value() };
            }
        }
    }
}

impl<T> Slot<T> {
    /// Creates an empty slot.
    fn new() -> Slot<T> {
        Slot {
            ready: AtomicUsize::new(EMPTY),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Returns true if the slot contains an initialized value.
    fn is_ready(&self) -> bool {
        self.ready.load(Acquire) == READY
    }

    /// Claims an empty slot for writing.
    fn claim(&self) -> bool {
        self.ready
            .compare_exchange(EMPTY, WRITING, AcqRel, Acquire)
            .is_ok()
    }

    /// Writes a value into the slot and marks it ready.
    fn write(&self, value: T) {
        self.value.with_mut(|ptr| unsafe {
            (*ptr).write(value);
        });
        self.ready.store(READY, Release);
    }

    /// Reads a value out of the slot and marks it empty.
    unsafe fn take(&self) -> T {
        let value = self
            .value
            .with_mut(|ptr| unsafe { (*ptr).assume_init_read() });
        self.ready.store(EMPTY, Release);
        value
    }

    /// Drops the value in the slot and marks it empty.
    unsafe fn drop_value(&self) {
        self.value.with_mut(|ptr| unsafe {
            (*ptr).assume_init_drop();
        });
        self.ready.store(EMPTY, Relaxed);
    }
}
