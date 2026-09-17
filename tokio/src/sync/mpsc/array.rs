//! A concurrent, lock-free, FIFO ring buffer for bounded channels.
//!
//! The semaphore in `bounded.rs` guarantees that senders only claim a slot when
//! the ring has capacity, so receiver-side slot reuse is coordinated by permit
//! release rather than by allocating new blocks.

use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::AtomicUsize;
use crate::loom::sync::Arc;
use crate::sync::mpsc::block::Read;
use crate::sync::mpsc::{chan, TryPopResult};

use std::fmt;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};

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

    /// Bitfield tracking slots that are ready to have their values consumed.
    ///
    /// The array will never be used for more than `super::BLOCK_CAP` values.
    ready_slots: AtomicUsize,

    /// Receiver position, used for diagnostics.
    head_position: AtomicUsize,

    /// Number of slots in the ring buffer.
    capacity: usize,

    /// The position after the last valid queue position.
    ///
    /// Positions are kept in the range `0..position_wrap`. Using two laps of
    /// the ring distinguishes an empty queue from a full one while avoiding the
    /// discontinuity that integer overflow would introduce for capacities that
    /// are not powers of two.
    position_wrap: usize,
}

struct Slot<T> {
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
    debug_assert!(capacity > 0);
    debug_assert!(capacity <= usize::BITS as usize);

    let slots = (0..capacity)
        .map(|_| Slot::new())
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let shared = Arc::new(Shared {
        slots,
        ready_slots: AtomicUsize::new(0),
        head_position: AtomicUsize::new(0),
        capacity,
        position_wrap: 2 * capacity,
    });

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
        let slot_index = self.claim_position();
        let slot = self.shared.slot(slot_index);

        slot.write(value);
        self.shared.set_ready(slot_index);
    }

    fn claim_position(&self) -> usize {
        let mut tail = self.tail_position.load(Relaxed);

        loop {
            let next = self.shared.next_position(tail);

            match self
                .tail_position
                .compare_exchange_weak(tail, next, AcqRel, Acquire)
            {
                Ok(_) => return tail,
                Err(actual) => tail = actual,
            }
        }
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
        let tail_position = self.tail_position.load(Relaxed);
        let head_position = self.shared.head_position.load(Relaxed);

        fmt.debug_struct("Tx")
            .field("tail_position", &tail_position)
            .field("capacity", &self.shared.capacity)
            .field("len", &self.shared.distance(head_position, tail_position))
            .field("closed", &(self.closed.load(Relaxed) == CLOSED))
            .finish()
    }
}

impl<T> Rx<T> {
    pub(crate) fn is_empty(&self, tx: &Tx<T>) -> bool {
        self.len(tx) == 0
    }

    pub(crate) fn len(&self, tx: &Tx<T>) -> usize {
        let tail = tx.tail_position.load(Acquire);
        self.shared.distance(self.index, tail)
    }

    /// Pops the next value off the queue.
    pub(crate) fn pop(&mut self, tx: &Tx<T>) -> Option<Read<T>> {
        let mut tail = tx.tail_position.load(Acquire);

        if self.index == tail {
            if !tx.is_closed() {
                return None;
            }

            // The tail claim is sequenced before closing the final sender. Load
            // it again after observing `closed` so that a message claimed by
            // that sender cannot be mistaken for an empty, closed queue.
            tail = tx.tail_position.load(Acquire);
            if self.index == tail {
                return Some(Read::Closed);
            }
        }

        if !self.shared.is_ready(self.index) {
            return None;
        }

        let slot = self.shared.slot(self.index);
        let value = unsafe { slot.take() };
        self.shared.clear_ready(self.index);
        self.index = self.shared.next_position(self.index);
        self.shared.head_position.store(self.index, Relaxed);
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
        let mut tail = tx.tail_position.load(Acquire);

        if self.index == tail {
            if !tx.is_closed() {
                return TryPopResult::Empty;
            }

            tail = tx.tail_position.load(Acquire);
            if self.index == tail {
                return TryPopResult::Closed;
            }
        }

        if !self.shared.is_ready(self.index) {
            return TryPopResult::Busy;
        }

        let slot = self.shared.slot(self.index);
        let value = unsafe { slot.take() };
        self.shared.clear_ready(self.index);
        self.index = self.shared.next_position(self.index);
        self.shared.head_position.store(self.index, Relaxed);
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
    /// Returns the slot for a queue position.
    fn slot(&self, position: usize) -> &Slot<T> {
        // Capacity can never be 0, guarded by the constructor.
        &self.slots[position % self.capacity]
    }

    /// Returns true if the slot for a queue position is ready.
    fn is_ready(&self, position: usize) -> bool {
        self.ready_slots.load(Acquire) & self.mask(position) != 0
    }

    /// Marks the slot for a queue position ready.
    fn set_ready(&self, position: usize) {
        self.ready_slots.fetch_or(self.mask(position), Release);
    }

    /// Marks the slot for a queue position empty.
    fn clear_ready(&self, position: usize) {
        self.ready_slots.fetch_and(!self.mask(position), Release);
    }

    fn next_position(&self, position: usize) -> usize {
        if position + 1 == self.position_wrap {
            0
        } else {
            position + 1
        }
    }

    fn distance(&self, from: usize, to: usize) -> usize {
        if to >= from {
            to - from
        } else {
            self.position_wrap - from + to
        }
    }

    fn mask(&self, position: usize) -> usize {
        1 << (position % self.capacity)
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        for position in 0..self.capacity {
            if self.is_ready(position) {
                let slot = self.slot(position);
                unsafe { slot.drop_value() };
            }
        }
    }
}

impl<T> Slot<T> {
    /// Creates an empty slot.
    fn new() -> Slot<T> {
        Slot {
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Writes a value into the slot.
    fn write(&self, value: T) {
        self.value.with_mut(|ptr| unsafe {
            ptr::write(ptr, MaybeUninit::new(value));
        });
    }

    /// Reads a value out of the slot.
    unsafe fn take(&self) -> T {
        let value = self.value.with(|ptr| unsafe { ptr::read(ptr) });

        unsafe { value.assume_init() }
    }

    /// Drops the value in the slot.
    unsafe fn drop_value(&self) {
        self.value.with_mut(|ptr| unsafe {
            (*ptr).assume_init_drop();
        });
    }
}
