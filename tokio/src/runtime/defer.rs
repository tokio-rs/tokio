use std::collections::VecDeque;
use std::task::Waker;

pub(crate) struct Defer {
    deferred: VecDeque<Waker>,
}

impl Defer {
    pub(crate) fn new() -> Defer {
        Defer {
            deferred: Default::default(),
        }
    }

    pub(crate) fn defer(&mut self, waker: Waker) {
        self.deferred.push_back(waker);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.deferred.is_empty()
    }

    pub(crate) fn wake(&mut self) {
        for waker in self.deferred.drain(..) {
            waker.wake();
        }
    }
}
