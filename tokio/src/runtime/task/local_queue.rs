use super::{Notified, RawTask, Schedule};

use std::marker::PhantomData;

/// A singly-linked FIFO queue that reuses `Header::queue_next` for linking.
///
/// A task must not be in any other queue that uses `queue_next` (e.g., the
/// inject queue) while it is in this queue.
pub(crate) struct LocalQueue<S: 'static> {
    head: Option<RawTask>,
    tail: Option<RawTask>,
    len: usize,
    _schedule: PhantomData<S>,
}

// Safety: RawTask is !Send only because it wraps NonNull<Header>. The values
// stored here originate from Notified<S> which is Send when S: Schedule.
unsafe impl<S: Schedule> Send for LocalQueue<S> {}

impl<S: 'static> LocalQueue<S> {
    pub(crate) fn new() -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            _schedule: PhantomData,
        }
    }

    pub(crate) fn push_back(&mut self, task: Notified<S>) {
        let raw = task.into_raw();
        unsafe { raw.set_queue_next(None) };
        if let Some(tail) = self.tail {
            unsafe { tail.set_queue_next(Some(raw)) };
        } else {
            self.head = Some(raw);
        }
        self.tail = Some(raw);
        self.len += 1;
    }

    pub(crate) fn pop_front(&mut self) -> Option<Notified<S>> {
        let head = self.head?;
        self.head = unsafe { head.get_queue_next() };
        if self.head.is_none() {
            self.tail = None;
        }
        unsafe { head.set_queue_next(None) };
        self.len -= 1;
        Some(unsafe { Notified::from_raw(head) })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

impl<S: 'static> Drop for LocalQueue<S> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
    }
}
